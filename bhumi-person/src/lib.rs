//! Bhumi Person - For people/apps interacting with Bhumi devices
//!
//! This crate provides the Person type for humans or applications that
//! want to pair with and control Bhumi devices.
//!
//! # Example
//!
//! ```ignore
//! use bhumi_person::{Person, json};
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut person = Person::new("/tmp/my-identity".into());
//!
//!     // Pair with a device using an invite token
//!     person.pair("127.0.0.1:8443", "INVITE_TOKEN", "my-switch").await.unwrap();
//!
//!     // Send a command
//!     let result = person.send_command("127.0.0.1:8443", "my-switch", "status", json!({})).await.unwrap();
//!     println!("Status: {:?}", result);
//! }
//! ```

use std::path::PathBuf;

// Re-export from bhumi-node
pub use bhumi_node::{
    Connection, Request, Response,
    DeviceState, PeerRecord, PeerRole,
    SecretKey, PublicKey, JsonValue, json,
    HandshakeInit, HandshakeComplete, HANDSHAKE_ACCEPTED, HANDSHAKE_REJECTED,
    DEV_HANDSHAKE_INIT,
    load_or_create, parse_invite_token,
};

/// A Bhumi person/app that can pair with and control devices
pub struct Person {
    secret_key: SecretKey,
    pub public_key: PublicKey,
    state: DeviceState,
    state_path: PathBuf,
}

impl Person {
    /// Create or load a person's identity and state
    pub fn new(home: PathBuf) -> Self {
        std::fs::create_dir_all(&home).expect("failed to create home directory");

        let (secret_key, public_key) = bhumi_node::load_or_create(&home);
        let state_path = home.join("state.json");
        let state = DeviceState::load(&state_path);

        Self {
            secret_key,
            public_key,
            state,
            state_path,
        }
    }

    /// Get this person's public key as id52 string
    pub fn id52(&self) -> String {
        self.public_key.to_string()
    }

    fn save(&self) {
        self.state.save(&self.state_path);
    }

    fn get_commits(&self) -> Vec<[u8; 32]> {
        self.state.get_all_commits()
    }

    /// List all paired devices
    pub fn list_peers(&self) -> impl Iterator<Item = ([u8; 32], &PeerRecord)> {
        self.state.peers.iter().map(|(k, v)| (*k, v))
    }

    /// Pair with a device using an invite token
    pub async fn pair(
        &mut self,
        relay_addr: &str,
        token: &str,
        alias: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Parse invite token
        let (their_id52, their_preimage) = parse_invite_token(token)?;

        // Accept the invite (creates pending peer record)
        let (my_preimage, my_commit) = self.state.accept_invite(their_id52, their_preimage, alias);
        self.save();

        // Connect to relay
        let mut conn = Connection::connect(relay_addr, &self.secret_key, vec![my_commit]).await?;

        // Send HANDSHAKE_INIT (to_bytes() already includes the message type prefix)
        let init = HandshakeInit {
            sender_id52: self.public_key.to_bytes(),
            preimage_for_peer: my_preimage,
            relay_url: relay_addr.to_string(),
        };

        let result = conn.send(their_id52, their_preimage, init.to_bytes()).await?;

        // Check for HANDSHAKE_COMPLETE
        if result.status == bhumi_proto::SEND_OK {
            let complete = HandshakeComplete::from_bytes(&result.payload)?;

            if complete.status == HANDSHAKE_ACCEPTED {
                // Complete handshake on our side
                let relay = if complete.relay_url.is_empty() {
                    None
                } else {
                    Some(complete.relay_url)
                };

                self.state.complete_handshake_as_acceptor(
                    &their_id52,
                    complete.preimage_for_peer,
                    relay,
                );
                self.save();

                return Ok(());
            } else {
                return Err("handshake rejected".into());
            }
        } else {
            return Err(format!("send failed with status {}", result.status).into());
        }
    }

    /// Send a command to a paired device
    pub async fn send_command(
        &mut self,
        relay_addr: &str,
        device_alias: &str,
        cmd: &str,
        args: JsonValue,
    ) -> Result<JsonValue, Box<dyn std::error::Error>> {
        // Find the device
        let (device_id52, _) = self.state.find_peer_by_alias(device_alias)
            .ok_or_else(|| format!("device '{}' not found", device_alias))?;

        // Get preimage for this device
        let preimage = self.state.get_peer_preimage(&device_id52)
            .ok_or("no preimage for device")?;

        // Connect to relay
        let mut conn = Connection::connect(relay_addr, &self.secret_key, self.get_commits()).await?;

        // Create request
        let request = Request::with_args(cmd, args);
        let payload = serde_json::to_vec(&request)?;

        // Send command
        let result = conn.send(device_id52, preimage, payload).await?;

        if result.status != bhumi_proto::SEND_OK {
            return Err(format!("send failed with status {}", result.status).into());
        }

        // Parse response (may have new preimage appended)
        let response_len = result.payload.len();
        let (response_bytes, new_preimage) = if response_len > 32 {
            // Response may have 32-byte preimage at the end
            // Try to parse as JSON first
            match serde_json::from_slice::<Response>(&result.payload) {
                Ok(_) => (result.payload.as_slice(), None),
                Err(_) => {
                    // Assume last 32 bytes are new preimage
                    let split = response_len - 32;
                    let new_pre: [u8; 32] = result.payload[split..].try_into().unwrap();
                    (&result.payload[..split], Some(new_pre))
                }
            }
        } else {
            (result.payload.as_slice(), None)
        };

        let response: Response = serde_json::from_slice(response_bytes)?;

        // Update preimage if we got a new one
        if let Some(new_pre) = new_preimage {
            self.state.update_peer_preimage(&device_id52, new_pre);
            self.save();
        }

        if response.ok {
            Ok(response.data.unwrap_or(JsonValue::Null))
        } else {
            Err(response.error.unwrap_or_else(|| "unknown error".to_string()).into())
        }
    }
}
