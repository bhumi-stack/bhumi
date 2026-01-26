//! Bhumi Node - Common functionality for Bhumi P2P network participants
//!
//! This crate provides the foundational types and functions used by both
//! bhumi-device (for IoT devices) and bhumi-person (for people/apps).

mod connection;
mod identity;
mod state;

pub use connection::Connection;
pub use identity::{load_or_create_identity, load_or_create, bhumi_home};
pub use state::{
    DeviceState, PeerRecord, InviteRecord, PeerRole, PreimageLookup,
    create_invite_token, parse_invite_token,
};

// Re-export commonly used types
pub use fastn_id52::{SecretKey, PublicKey};
pub use serde_json::{json, Value as JsonValue};

pub use bhumi_proto::{
    HandshakeInit, HandshakeComplete, SendResult,
    HANDSHAKE_ACCEPTED, HANDSHAKE_REJECTED, SEND_OK,
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
