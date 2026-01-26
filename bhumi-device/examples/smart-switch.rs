//! Smart Switch - Example IoT device using bhumi-device
//!
//! This simulates a smart electrical wall switch that can be controlled
//! remotely through the Bhumi P2P network.
//!
//! Usage:
//!   SWITCH_HOME=/tmp/smart-switch cargo run --example smart-switch
//!
//! On first run, prints an invite token for the owner to pair with.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use bhumi_device::{Device, DeviceConfig, PeerRole, json};

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
    let state = SwitchState { is_on: AtomicBool::new(false) };
    let config = DeviceConfig {
        kind: "smart-switch".to_string(),
        location: String::new(),
    };
    let mut device = Device::with_state(home, config, state);

    println!("Smart Switch v0.1");
    println!("Device ID: {}", device.id52());
    println!();

    // First run - create invite for owner
    if !device.is_paired() {
        let token = device.create_invite("owner", PeerRole::Owner);
        println!("=== PAIRING MODE ===");
        println!("Share this invite token with the switch owner:");
        println!();
        println!("  {}", token);
        println!();
        println!("====================");
    } else {
        println!("Paired: {} peer(s), {} pending invite(s)",
            device.peer_count(), device.invite_count());
    }
    println!();

    // Register custom commands
    device.command("status", |_ctx, state, _args| {
        let is_on = state.is_on.load(Ordering::Relaxed);
        Ok(json!({ "is_on": is_on }))
    });

    device.command("on", |_ctx, state, _args| {
        state.is_on.store(true, Ordering::Relaxed);
        println!("[SWITCH] Turned ON");
        Ok(json!({ "is_on": true }))
    });

    device.command("off", |_ctx, state, _args| {
        state.is_on.store(false, Ordering::Relaxed);
        println!("[SWITCH] Turned OFF");
        Ok(json!({ "is_on": false }))
    });

    device.command("toggle", |_ctx, state, _args| {
        let was_on = state.is_on.fetch_xor(true, Ordering::Relaxed);
        let is_on = !was_on;
        println!("[SWITCH] Toggled to {}", if is_on { "ON" } else { "OFF" });
        Ok(json!({ "is_on": is_on }))
    });

    // Built-in commands: invite/create, invite/list, invite/delete
    // Handshakes and preimage renewal are automatic

    println!("Connecting to relay...");
    device.run(RELAY_ADDR).await?;
    println!("Disconnected.");

    Ok(())
}
