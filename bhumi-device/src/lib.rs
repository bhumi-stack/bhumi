//! Bhumi Device Library
//!
//! A library for building P2P IoT applications using the Bhumi relay protocol.
//! Similar to how hyper/actix provide HTTP primitives, bhumi-device provides
//! P2P messaging primitives with a simple command-handler pattern.
//!
//! # Example
//!
//! ```ignore
//! use bhumi_device::Device;
//! use serde_json::json;
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut device = Device::new("/tmp/my-device".into());
//!
//!     // Register custom commands
//!     device.command("status", |_ctx, _args| {
//!         Ok(json!({ "is_on": false }))
//!     });
//!
//!     // Built-in commands: invite/create, invite/list, invite/delete
//!     // Handshakes and preimage renewal are automatic
//!
//!     device.run("127.0.0.1:8443").await.unwrap();
//! }
//! ```

mod connection;
mod identity;
mod state;

pub use connection::Connection;
pub use identity::{load_or_create_identity, bhumi_home};
pub use state::{DeviceState, PeerRecord, InviteRecord, PeerRole, PreimageLookup, create_invite_token, parse_invite_token};
pub use fastn_id52::{SecretKey, PublicKey};
pub use serde_json::{json, Value as JsonValue};

use std::collections::HashMap;
use std::path::PathBuf;

pub use bhumi_proto::{
    HandshakeInit, HandshakeComplete, HANDSHAKE_ACCEPTED, HANDSHAKE_REJECTED, SEND_OK,
    DEV_HANDSHAKE_INIT, parse_device_msg_type,
};

/// Command handler function type
pub type CommandHandler<S> = Box<dyn Fn(&CommandContext, &S, JsonValue) -> Result<JsonValue, String> + Send + Sync>;

/// Context passed to command handlers
pub struct CommandContext {
    /// The peer who sent this command
    pub peer_alias: String,
    /// The peer's id52
    pub peer_id52: [u8; 32],
    /// The peer's role
    pub role: PeerRole,
}

/// Request message format
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Request {
    pub cmd: String,
    #[serde(default)]
    pub args: JsonValue,
}

/// Response message format
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(data: JsonValue) -> Self {
        Self { ok: true, data: Some(data), error: None }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self { ok: false, data: None, error: Some(msg.into()) }
    }
}

/// A Bhumi device that can communicate with peers through relays
pub struct Device<S: Send + Sync + 'static = ()> {
    secret_key: SecretKey,
    pub public_key: PublicKey,
    state: DeviceState,
    state_path: PathBuf,
    home: PathBuf,
    handlers: HashMap<String, CommandHandler<S>>,
    app_state: Option<S>,
}

impl Device<()> {
    /// Create or load a device from the given home directory (no app state)
    pub fn new(home: PathBuf) -> Self {
        Self::with_state(home, ())
    }
}

impl<S: Send + Sync + 'static> Device<S> {
    /// Create or load a device with custom app state
    pub fn with_state(home: PathBuf, app_state: S) -> Self {
        std::fs::create_dir_all(&home).expect("failed to create home directory");

        let (secret_key, public_key) = identity::load_or_create(&home);
        let state_path = home.join("state.json");
        let state = DeviceState::load(&state_path);

        Self {
            secret_key,
            public_key,
            state,
            state_path,
            home,
            handlers: HashMap::new(),
            app_state: Some(app_state),
        }
    }

    /// Get the device's public key as id52 string
    pub fn id52(&self) -> String {
        self.public_key.to_string()
    }

    /// Register a command handler
    pub fn command<F>(&mut self, name: &str, handler: F)
    where
        F: Fn(&CommandContext, &S, JsonValue) -> Result<JsonValue, String> + Send + Sync + 'static,
    {
        self.handlers.insert(name.to_string(), Box::new(handler));
    }

    /// Create an invite (can be called before run() for initial setup)
    pub fn create_invite(&mut self, alias: &str, role: PeerRole) -> String {
        let (invite, _commit) = self.state.create_invite(alias, role);
        self.save();
        create_invite_token(&self.public_key.to_bytes(), &invite.preimage)
    }

    /// Check if device has any peers or invites
    pub fn is_paired(&self) -> bool {
        !self.state.peers.is_empty() || !self.state.invites.is_empty()
    }

    /// Get number of peers
    pub fn peer_count(&self) -> usize {
        self.state.peers.len()
    }

    /// Get number of pending invites
    pub fn invite_count(&self) -> usize {
        self.state.invites.len()
    }

    fn save(&self) {
        self.state.save(&self.state_path);
    }

    fn get_commits(&self) -> Vec<[u8; 32]> {
        self.state.get_all_commits()
    }

    /// Pair with another device by accepting their invite token
    pub async fn pair(&mut self, relay_addr: &str, token: &str, alias: &str) -> Result<(), Box<dyn std::error::Error>> {
        let (their_id52, their_preimage) = parse_invite_token(token)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let (my_preimage, my_commit) = self.state.accept_invite(their_id52, their_preimage, alias);
        self.save();

        // Connect and do handshake
        let mut commits = self.get_commits();
        commits.push(my_commit);
        let mut conn = Connection::connect(relay_addr, &self.secret_key, commits).await?;

        // Send HANDSHAKE_INIT
        let init = HandshakeInit {
            sender_id52: self.public_key.to_bytes(),
            preimage_for_peer: my_preimage,
            relay_url: relay_addr.to_string(),
        };

        let result = conn.send(their_id52, their_preimage, init.to_bytes()).await?;

        if result.status != SEND_OK {
            self.state.pending_peers.remove(&their_id52);
            self.save();
            return Err(format!("handshake failed: status={}", result.status).into());
        }

        // Parse HANDSHAKE_COMPLETE
        let complete = HandshakeComplete::from_bytes(&result.payload)?;

        if complete.status != HANDSHAKE_ACCEPTED {
            self.state.pending_peers.remove(&their_id52);
            self.save();
            return Err("handshake rejected by peer".into());
        }

        // Complete handshake
        self.state.complete_handshake_as_acceptor(&their_id52, complete.preimage_for_peer, Some(complete.relay_url));
        self.save();

        Ok(())
    }

    /// Send a command to a peer and get the response
    pub async fn send_command(&mut self, relay_addr: &str, peer_alias: &str, cmd: &str, args: JsonValue) -> Result<JsonValue, Box<dyn std::error::Error>> {
        let (peer_id52, peer) = self.state.find_peer_by_alias(peer_alias)
            .ok_or_else(|| format!("peer '{}' not found", peer_alias))?;

        let their_preimage = peer.their_preimage
            .ok_or("no preimage available to contact this peer")?;

        // Connect
        let mut conn = Connection::connect(relay_addr, &self.secret_key, self.get_commits()).await?;

        // Build request
        let request = Request { cmd: cmd.to_string(), args };
        let payload = serde_json::to_vec(&request)?;

        // Send
        let result = conn.send(peer_id52, their_preimage, payload).await?;

        if result.status != SEND_OK {
            return Err(format!("send failed: status={}", result.status).into());
        }

        // Parse response - JSON followed by optional 32-byte next_preimage
        let response_len = result.payload.len();
        let (json_part, preimage_part) = if response_len >= 32 {
            // Check if last 32 bytes could be a preimage (not valid JSON typically)
            let potential_json = &result.payload[..response_len - 32];
            if serde_json::from_slice::<Response>(potential_json).is_ok() {
                (potential_json, Some(&result.payload[response_len - 32..]))
            } else {
                (&result.payload[..], None)
            }
        } else {
            (&result.payload[..], None)
        };

        let response: Response = serde_json::from_slice(json_part)?;

        // Update preimage if we got a new one
        if let Some(preimage_bytes) = preimage_part {
            let mut new_preimage = [0u8; 32];
            new_preimage.copy_from_slice(preimage_bytes);
            if new_preimage != [0u8; 32] {
                self.state.update_peer_preimage(&peer_id52, new_preimage);
                self.save();
            }
        }

        if response.ok {
            Ok(response.data.unwrap_or(json!({})))
        } else {
            Err(response.error.unwrap_or_else(|| "unknown error".to_string()).into())
        }
    }

    /// List all paired peers
    pub fn list_peers(&self) -> Vec<(&[u8; 32], &PeerRecord)> {
        self.state.peers.iter().collect()
    }

    /// Run the device, connecting to relay and handling messages
    pub async fn run(&mut self, relay_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::connect(relay_addr, &self.secret_key, self.get_commits()).await?;

        loop {
            let msg = match conn.receive().await {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => return Err(e.into()),
            };

            let msg_type = msg.msg_type;

            if msg_type == Some(DEV_HANDSHAKE_INIT) {
                self.handle_handshake(&mut conn, msg.msg_id, &msg.preimage, &msg.payload).await?;
            } else {
                self.handle_command(&mut conn, msg.msg_id, &msg.preimage, &msg.payload).await?;
            }
        }

        Ok(())
    }

    async fn handle_handshake(
        &mut self,
        conn: &mut Connection,
        msg_id: u32,
        preimage: &[u8; 32],
        payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let init = HandshakeInit::from_bytes(payload)?;

        if let Some((new_preimage, new_commit)) = self.state.complete_handshake_as_inviter(
            preimage,
            init.sender_id52,
            init.preimage_for_peer,
            Some(init.relay_url),
        ) {
            self.save();

            let complete = HandshakeComplete {
                status: HANDSHAKE_ACCEPTED,
                preimage_for_peer: new_preimage,
                relay_url: "127.0.0.1:8443".to_string(), // TODO: make configurable
            };

            conn.send_ack(msg_id, complete.to_bytes()).await?;
            conn.update_commits(vec![new_commit]).await?;
        } else {
            let complete = HandshakeComplete {
                status: HANDSHAKE_REJECTED,
                preimage_for_peer: [0u8; 32],
                relay_url: String::new(),
            };
            conn.send_ack(msg_id, complete.to_bytes()).await?;
        }

        Ok(())
    }

    async fn handle_command(
        &mut self,
        conn: &mut Connection,
        msg_id: u32,
        preimage: &[u8; 32],
        payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Identify sender - reject if unknown
        let (peer_id52, peer_alias, role) = match self.state.lookup_preimage(preimage) {
            Some(PreimageLookup::Peer(id52, peer)) => (id52, peer.alias.clone(), peer.role),
            _ => {
                let response = Response::err("unauthorized");
                conn.send_ack(msg_id, serde_json::to_vec(&response)?).await?;
                return Ok(());
            }
        };

        let ctx = CommandContext { peer_alias, peer_id52, role };

        // Parse request
        let response = match serde_json::from_slice::<Request>(payload) {
            Ok(req) => self.dispatch_command(&ctx, &req),
            Err(e) => Response::err(format!("invalid request: {}", e)),
        };

        // Serialize response
        let mut response_bytes = serde_json::to_vec(&response)?;

        // Renew preimage
        if let Some((new_preimage, new_commit)) = self.state.consume_and_renew_preimage(&peer_id52, preimage) {
            self.save();
            // Append next_preimage to response (32 bytes at end)
            response_bytes.extend_from_slice(&new_preimage);
            conn.update_commits(vec![new_commit]).await?;
        }

        conn.send_ack(msg_id, response_bytes).await?;
        Ok(())
    }

    fn dispatch_command(&mut self, ctx: &CommandContext, req: &Request) -> Response {
        // Built-in commands (owner only)
        match req.cmd.as_str() {
            "invite/create" => {
                if ctx.role != PeerRole::Owner {
                    return Response::err("permission denied: owner only");
                }
                let alias = req.args.get("alias")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user");
                let role_str = req.args.get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("reader");
                let role = match role_str {
                    "owner" => PeerRole::Owner,
                    "writer" => PeerRole::Writer,
                    _ => PeerRole::Reader,
                };
                let token = self.create_invite(alias, role);
                Response::ok(json!({ "token": token }))
            }
            "invite/list" => {
                if ctx.role != PeerRole::Owner {
                    return Response::err("permission denied: owner only");
                }
                let invites: Vec<_> = self.state.invites.iter()
                    .map(|(preimage, invite)| {
                        let id = data_encoding::HEXLOWER.encode(&preimage[..8]);
                        let role = format!("{:?}", invite.role).to_lowercase();
                        json!({ "id": id, "alias": invite.alias, "role": role })
                    })
                    .collect();
                Response::ok(json!({ "invites": invites }))
            }
            "invite/delete" => {
                if ctx.role != PeerRole::Owner {
                    return Response::err("permission denied: owner only");
                }
                let id = match req.args.get("id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => return Response::err("missing id"),
                };
                let prefix = match data_encoding::HEXLOWER.decode(id.as_bytes()) {
                    Ok(p) => p,
                    Err(_) => return Response::err("invalid id"),
                };
                let found = self.state.invites.keys()
                    .find(|p| p[..prefix.len().min(32)] == prefix[..])
                    .cloned();
                if let Some(preimage) = found {
                    self.state.invites.remove(&preimage);
                    self.save();
                    Response::ok(json!({ "deleted": true }))
                } else {
                    Response::err("invite not found")
                }
            }
            cmd => {
                // Custom command handler
                if let Some(handler) = self.handlers.get(cmd) {
                    let app_state = self.app_state.as_ref().unwrap();
                    match handler(ctx, app_state, req.args.clone()) {
                        Ok(data) => Response::ok(data),
                        Err(e) => Response::err(e),
                    }
                } else {
                    Response::err(format!("unknown command: {}", cmd))
                }
            }
        }
    }
}

/// Incoming message from a peer
pub struct IncomingMessage {
    pub msg_id: u32,
    pub preimage: [u8; 32],
    pub msg_type: Option<u8>,
    pub payload: Vec<u8>,
}

impl Connection {
    /// Receive the next incoming message
    pub async fn receive(&mut self) -> std::io::Result<IncomingMessage> {
        let deliver = self.receive_deliver().await?;
        let msg_type = parse_device_msg_type(&deliver.payload);

        Ok(IncomingMessage {
            msg_id: deliver.msg_id,
            preimage: deliver.preimage,
            msg_type,
            payload: deliver.payload,
        })
    }
}
