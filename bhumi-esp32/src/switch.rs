//! Smart switch command handlers

use crate::state::{DeviceState, PeerRole};
use crate::{IS_ON, set_led};
use serde::{Serialize, Deserialize};
use serde_json::{json, Value as JsonValue};
use std::sync::atomic::Ordering;
use log::*;

/// Command request
#[derive(Debug, Deserialize)]
pub struct Request {
    pub cmd: String,
    #[serde(default)]
    pub args: JsonValue,
}

/// Command response
#[derive(Debug, Serialize)]
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

/// Handle an incoming command
/// Returns (response_bytes, Option<(new_preimage, new_commit)>)
pub fn handle_command(
    state: &mut DeviceState,
    msg: &super::connection::ReceivedMessage,
) -> (Vec<u8>, Option<([u8; 32], [u8; 32])>) {
    // Look up sender by preimage
    let (peer_id52, peer) = match state.lookup_preimage(&msg.preimage) {
        Some((id, p)) => (id.clone(), p.clone()),
        None => {
            let response = Response::err("unauthorized");
            return (serde_json::to_vec(&response).unwrap(), None);
        }
    };

    // Parse request
    let request: Request = match serde_json::from_slice(&msg.payload) {
        Ok(r) => r,
        Err(e) => {
            let response = Response::err(format!("invalid request: {}", e));
            return (serde_json::to_vec(&response).unwrap(), None);
        }
    };

    info!("Command '{}' from {} (role: {:?})", request.cmd, peer.alias, peer.role);

    // Dispatch command
    let response = dispatch_command(state, &peer.role, &request);

    // Save LED state if it changed (for on/off/toggle commands)
    if matches!(request.cmd.as_str(), "on" | "off" | "toggle") && response.ok {
        let is_on = IS_ON.load(Ordering::Relaxed);
        state.save_led_state(is_on);
    }

    // Serialize response
    let response_bytes = serde_json::to_vec(&response).unwrap();

    // Renew preimage
    let new_preimage = state.renew_preimage(&peer_id52, &msg.preimage);

    (response_bytes, new_preimage)
}

fn dispatch_command(state: &DeviceState, role: &PeerRole, req: &Request) -> Response {
    match req.cmd.as_str() {
        // Node info - anyone can read
        "node/info" => {
            Response::ok(json!({
                "kind": "smart-switch",
                "location": "",
                "id": state.id52(),
            }))
        }

        // Switch status - anyone can read
        "status" => {
            let is_on = IS_ON.load(Ordering::Relaxed);
            Response::ok(json!({ "is_on": is_on }))
        }

        // Turn on - writer or owner
        "on" => {
            if *role == PeerRole::Reader {
                return Response::err("permission denied: writer or owner only");
            }
            IS_ON.store(true, Ordering::Relaxed);
            set_led(true);
            info!("[SWITCH] Turned ON");
            Response::ok(json!({ "is_on": true }))
        }

        // Turn off - writer or owner
        "off" => {
            if *role == PeerRole::Reader {
                return Response::err("permission denied: writer or owner only");
            }
            IS_ON.store(false, Ordering::Relaxed);
            set_led(false);
            info!("[SWITCH] Turned OFF");
            Response::ok(json!({ "is_on": false }))
        }

        // Toggle - writer or owner
        "toggle" => {
            if *role == PeerRole::Reader {
                return Response::err("permission denied: writer or owner only");
            }
            let was_on = IS_ON.fetch_xor(true, Ordering::Relaxed);
            let is_on = !was_on;
            set_led(is_on);
            info!("[SWITCH] Toggled to {}", if is_on { "ON" } else { "OFF" });
            Response::ok(json!({ "is_on": is_on }))
        }

        // Unknown command
        cmd => {
            Response::err(format!("unknown command: {}", cmd))
        }
    }
}
