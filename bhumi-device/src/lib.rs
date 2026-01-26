//! Bhumi Device - For IoT devices on the Bhumi P2P network
//!
//! This crate provides the Device type for building IoT applications.
//! Devices can register command handlers and respond to requests from
//! paired people (owners/writers/readers).
//!
//! # Example
//!
//! ```ignore
//! use bhumi_device::{Device, DeviceConfig, json};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = DeviceConfig {
//!         kind: "smart-switch".to_string(),
//!         location: "home.bedroom".to_string(),
//!     };
//!     let mut device = Device::new("/tmp/my-device".into(), config);
//!
//!     device.command("status", |_ctx, _state, _args| {
//!         Ok(json!({ "is_on": false }))
//!     });
//!
//!     device.run("127.0.0.1:8443").await.unwrap();
//! }
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

// Re-export from bhumi-node
pub use bhumi_node::{
    Connection, CommandContext, Request, Response, IncomingMessage,
    DeviceState, PeerRecord, InviteRecord, PeerRole, PreimageLookup,
    SecretKey, PublicKey, JsonValue, json,
    HandshakeInit, HandshakeComplete, HANDSHAKE_ACCEPTED, HANDSHAKE_REJECTED, SEND_OK,
    DEV_HANDSHAKE_INIT,
    load_or_create, create_invite_token, parse_invite_token,
};

/// Device configuration - metadata about the device
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceConfig {
    /// Kind of device (e.g., "smart-switch", "thermostat", "camera")
    pub kind: String,
    /// Location in dotted notation (e.g., "home-1.bedroom", "office.desk")
    pub location: String,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            kind: "unknown".to_string(),
            location: "".to_string(),
        }
    }
}

/// Command handler function type
pub type CommandHandler<S> = Box<dyn Fn(&CommandContext, &S, JsonValue) -> Result<JsonValue, String> + Send + Sync>;

/// A Bhumi IoT device
pub struct Device<S: Send + Sync + 'static = ()> {
    secret_key: SecretKey,
    pub public_key: PublicKey,
    state: DeviceState,
    state_path: PathBuf,
    config: DeviceConfig,
    config_path: PathBuf,
    relay_addr: Option<String>,
    handlers: HashMap<String, CommandHandler<S>>,
    app_state: Option<S>,
}

impl Device<()> {
    /// Create or load a device with no app state
    pub fn new(home: PathBuf, config: DeviceConfig) -> Self {
        Self::with_state(home, config, ())
    }
}

impl<S: Send + Sync + 'static> Device<S> {
    /// Create or load a device with custom app state
    pub fn with_state(home: PathBuf, config: DeviceConfig, app_state: S) -> Self {
        std::fs::create_dir_all(&home).expect("failed to create home directory");

        let (secret_key, public_key) = bhumi_node::load_or_create(&home);
        let state_path = home.join("state.json");
        let config_path = home.join("config.json");
        let state = DeviceState::load(&state_path);

        // Load or save config
        let config = if config_path.exists() {
            let data = std::fs::read_to_string(&config_path).expect("failed to read config");
            serde_json::from_str(&data).unwrap_or(config)
        } else {
            std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
                .expect("failed to write config");
            config
        };

        Self {
            secret_key,
            public_key,
            state,
            state_path,
            config,
            config_path,
            relay_addr: None,
            handlers: HashMap::new(),
            app_state: Some(app_state),
        }
    }

    /// Get the device's public key as id52 string
    pub fn id52(&self) -> String {
        self.public_key.to_string()
    }

    /// Get the device's kind
    pub fn kind(&self) -> &str {
        &self.config.kind
    }

    /// Get the device's location
    pub fn location(&self) -> &str {
        &self.config.location
    }

    /// Register a command handler
    pub fn command<F>(&mut self, name: &str, handler: F)
    where
        F: Fn(&CommandContext, &S, JsonValue) -> Result<JsonValue, String> + Send + Sync + 'static,
    {
        self.handlers.insert(name.to_string(), Box::new(handler));
    }

    /// Create an invite
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

    /// Run the device, connecting to relay and handling messages
    pub async fn run(&mut self, relay_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.relay_addr = Some(relay_addr.to_string());
        let mut conn = Connection::connect(relay_addr, &self.secret_key, self.get_commits()).await?;

        loop {
            let msg = match conn.receive().await {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => return Err(e.into()),
            };

            if msg.msg_type == Some(DEV_HANDSHAKE_INIT) {
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

            let relay_addr = self.relay_addr.clone().unwrap_or_default();
            let complete = HandshakeComplete {
                status: HANDSHAKE_ACCEPTED,
                preimage_for_peer: new_preimage,
                relay_url: relay_addr,
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
            response_bytes.extend_from_slice(&new_preimage);
            conn.update_commits(vec![new_commit]).await?;
        }

        conn.send_ack(msg_id, response_bytes).await?;
        Ok(())
    }

    fn dispatch_command(&mut self, ctx: &CommandContext, req: &Request) -> Response {
        match req.cmd.as_str() {
            // Device info - anyone can read
            "device/info" => {
                Response::ok(json!({
                    "kind": self.config.kind,
                    "location": self.config.location,
                    "id": self.id52(),
                }))
            }

            // Invite management - owner only
            "invite/create" => {
                if ctx.role != PeerRole::Owner {
                    return Response::err("permission denied: owner only");
                }
                let alias = req.args.get("alias").and_then(|v| v.as_str()).unwrap_or("user");
                let role_str = req.args.get("role").and_then(|v| v.as_str()).unwrap_or("reader");
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

            // Peer management - owner only
            "peers/list" => {
                if ctx.role != PeerRole::Owner {
                    return Response::err("permission denied: owner only");
                }
                let peers: Vec<_> = self.state.peers.iter()
                    .map(|(id52, peer)| {
                        let id_short = data_encoding::BASE32_DNSSEC.encode(&id52[..10]);
                        let role = format!("{:?}", peer.role).to_lowercase();
                        json!({
                            "id": id_short,
                            "alias": peer.alias,
                            "role": role,
                        })
                    })
                    .collect();
                Response::ok(json!({ "peers": peers }))
            }

            // Custom command
            cmd => {
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
