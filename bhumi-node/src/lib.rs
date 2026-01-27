//! Bhumi Node - Unified P2P network node
//!
//! This crate provides the Node type for building any participant on the
//! Bhumi P2P network - whether it's an IoT device, mobile app, or server.
//!
//! # Example - Device (listens for commands)
//!
//! ```ignore
//! use bhumi_node::{Node, NodeConfig, PeerRole, json};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = NodeConfig {
//!         kind: "smart-switch".to_string(),
//!         location: "home.bedroom".to_string(),
//!     };
//!     let mut node = Node::new("/tmp/my-device".into(), config);
//!
//!     // Create invite for first owner
//!     if !node.is_paired() {
//!         let token = node.create_invite("owner", PeerRole::Owner);
//!         println!("Invite: {}", token);
//!     }
//!
//!     // Register command handlers
//!     node.command("status", |_ctx, _state, _args| {
//!         Ok(json!({ "is_on": false }))
//!     });
//!
//!     // Run and handle incoming messages
//!     node.run("127.0.0.1:8443").await.unwrap();
//! }
//! ```
//!
//! # Example - Client (sends commands)
//!
//! ```ignore
//! use bhumi_node::{Node, NodeConfig, json};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = NodeConfig {
//!         kind: "mobile-app".to_string(),
//!         ..Default::default()
//!     };
//!     let mut node = Node::new("/tmp/my-app".into(), config);
//!
//!     // Pair with a device
//!     node.pair("127.0.0.1:8443", "INVITE_TOKEN", "my-switch").await.unwrap();
//!
//!     // Send commands
//!     let result = node.send("127.0.0.1:8443", "my-switch", "status", json!({})).await.unwrap();
//!     println!("Status: {:?}", result);
//! }
//! ```

mod connection;
mod identity;
mod node;
mod state;

pub use connection::Connection;
pub use identity::{load_or_create_identity, load_or_create, bhumi_home};
pub use node::{Node, NodeConfig, CommandHandler};
pub use state::{
    DeviceState, PeerRecord, InviteRecord, PeerRole, PreimageLookup,
    create_invite_token, parse_invite_token,
};

// Re-export commonly used types
pub use fastn_id52::{SecretKey, PublicKey};
pub use serde_json::{json, Value as JsonValue};

pub use bhumi_proto::{
    HandshakeInit, HandshakeComplete, SendResult,
    HANDSHAKE_ACCEPTED, HANDSHAKE_REJECTED,
    SEND_OK, SEND_ERR_NOT_CONNECTED, SEND_ERR_INVALID_PREIMAGE,
    SEND_ERR_TIMEOUT, SEND_ERR_DISCONNECTED,
    DEV_HANDSHAKE_INIT, parse_device_msg_type,
};

/// Request message format for commands
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Request {
    pub cmd: String,
    #[serde(default)]
    pub args: JsonValue,
}

impl Request {
    pub fn new(cmd: &str) -> Self {
        Self { cmd: cmd.to_string(), args: JsonValue::Null }
    }

    pub fn with_args(cmd: &str, args: JsonValue) -> Self {
        Self { cmd: cmd.to_string(), args }
    }
}

/// Response message format
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
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

/// Context for command handlers - information about the sender
#[derive(Debug, Clone)]
pub struct CommandContext {
    pub peer_alias: String,
    pub peer_id52: [u8; 32],
    pub role: PeerRole,
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
