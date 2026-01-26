//! Smart Switch - Example IoT device using bhumi-node
//!
//! This simulates a smart electrical wall switch that can be controlled
//! remotely through the Bhumi P2P network.
//!
//! Usage:
//!   SWITCH_HOME=/tmp/smart-switch cargo run --example smart-switch -p bhumi-node
//!
//! On first run, prints an invite token for the owner to pair with.

use bhumi_node::{Node, NodeConfig, PeerRole, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

const RELAY_ADDR: &str = "127.0.0.1:8443";

fn get_home() -> PathBuf {
    std::env::var("SWITCH_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/smart-switch"))
}

struct SwitchState {
    is_on: AtomicBool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = get_home();
    let state = SwitchState {
        is_on: AtomicBool::new(false),
    };
    let config = NodeConfig {
        kind: "smart-switch".to_string(),
        location: String::new(),
    };
    let mut node = Node::with_state(home, config, state);

    println!("Smart Switch v0.1");
    println!("Device ID: {}", node.id52());
    println!();

    // First run - create invite for owner
    if !node.is_paired() {
        let token = node.create_invite("owner", PeerRole::Owner);
        println!("=== PAIRING MODE ===");
        println!("Share this invite token with the switch owner:");
        println!();
        println!("  {}", token);
        println!();
        println!("====================");
    } else {
        println!(
            "Paired: {} peer(s), {} pending invite(s)",
            node.peer_count(),
            node.invite_count()
        );
    }
    println!();

    // Register custom commands
    node.command("status", |_ctx, state, _args| {
        let is_on = state.is_on.load(Ordering::Relaxed);
        Ok(json!({ "is_on": is_on }))
    });

    node.command("on", |_ctx, state, _args| {
        state.is_on.store(true, Ordering::Relaxed);
        println!("[SWITCH] Turned ON");
        Ok(json!({ "is_on": true }))
    });

    node.command("off", |_ctx, state, _args| {
        state.is_on.store(false, Ordering::Relaxed);
        println!("[SWITCH] Turned OFF");
        Ok(json!({ "is_on": false }))
    });

    node.command("toggle", |_ctx, state, _args| {
        let was_on = state.is_on.fetch_xor(true, Ordering::Relaxed);
        let is_on = !was_on;
        println!("[SWITCH] Toggled to {}", if is_on { "ON" } else { "OFF" });
        Ok(json!({ "is_on": is_on }))
    });

    // Built-in commands: node/info, invite/create, invite/list, invite/delete, peers/list
    // Handshakes and preimage renewal are automatic

    println!("Connecting to relay...");
    node.run(RELAY_ADDR).await?;
    println!("Disconnected.");

    Ok(())
}
