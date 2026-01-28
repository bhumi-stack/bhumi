//! Switch Controller - CLI to control Bhumi smart switches
//!
//! Supports both BLE provisioning and relay-based control.
//!
//! Usage:
//!   CONTROLLER_HOME=/tmp/switch-controller cargo run -p switch-controller -- <command>
//!
//! BLE Commands:
//!   ble scan                       - Scan for Bhumi devices
//!   ble provision [--device NAME] --ssid SSID --password PASS
//!   ble reset [--device NAME]      - Factory reset a device
//!
//! Relay Commands:
//!   pair <invite-token> [alias]    - Pair with a switch
//!   list                           - List paired switches
//!   <switch> status                - Get switch status
//!   <switch> on                    - Turn switch on
//!   <switch> off                   - Turn switch off
//!   <switch> toggle                - Toggle switch
//!   <switch> invite create [alias] [role]
//!   <switch> invite list
//!   <switch> invite delete <id>

use std::path::PathBuf;
use clap::{Parser, Subcommand};
use bhumi_node::{Node, NodeConfig, json};

const RELAY_ADDR: &str = "64.227.143.197:8443";

#[derive(Parser)]
#[command(name = "switch-controller")]
#[command(about = "Control Bhumi smart switches via BLE and relay")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// BLE operations (scan, provision, reset)
    Ble {
        #[command(subcommand)]
        action: BleCommands,
    },
    /// Pair with a switch using an invite token
    Pair {
        /// The invite token from the switch owner
        token: String,
        /// Local alias for the switch
        #[arg(default_value = "switch")]
        alias: String,
    },
    /// List paired switches
    List,
    /// Control a specific switch
    Switch {
        /// Switch alias or ID
        name: String,
        #[command(subcommand)]
        action: SwitchCommands,
    },
}

#[derive(Subcommand)]
enum BleCommands {
    /// Scan for Bhumi devices
    Scan {
        /// Scan duration in seconds
        #[arg(short, long, default_value = "5")]
        duration: u64,
    },
    /// Send WiFi credentials to a device
    Provision {
        /// Device name or address to connect to
        #[arg(short, long)]
        device: Option<String>,
        /// WiFi SSID
        #[arg(long)]
        ssid: String,
        /// WiFi password
        #[arg(long)]
        password: String,
    },
    /// Reset a device (clear all settings)
    Reset {
        /// Device name or address to connect to
        #[arg(short, long)]
        device: Option<String>,
    },
}

#[derive(Subcommand)]
enum SwitchCommands {
    /// Get switch status
    Status,
    /// Turn switch on
    On,
    /// Turn switch off
    Off,
    /// Toggle switch
    Toggle,
    /// Manage invites
    Invite {
        #[command(subcommand)]
        action: InviteCommands,
    },
}

#[derive(Subcommand)]
enum InviteCommands {
    /// Create a new invite
    Create {
        /// Alias for the invited user
        #[arg(default_value = "user")]
        alias: String,
        /// Role: owner, writer, or reader
        #[arg(default_value = "reader")]
        role: String,
    },
    /// List open invites
    List,
    /// Delete an invite
    Delete {
        /// Invite ID to delete
        id: String,
    },
}

fn get_home() -> PathBuf {
    std::env::var("CONTROLLER_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/switch-controller"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ble { action } => run_ble(action).await?,
        Commands::Pair { token, alias } => cmd_pair(&token, &alias).await?,
        Commands::List => cmd_list(),
        Commands::Switch { name, action } => run_switch(&name, action).await?,
    }

    Ok(())
}

// ============================================================================
// BLE Commands
// ============================================================================

async fn run_ble(cmd: BleCommands) -> Result<(), Box<dyn std::error::Error>> {
    use bhumi_ble_controller::ble;

    match cmd {
        BleCommands::Scan { duration } => {
            println!("Scanning for BLE devices ({} seconds)...", duration);
            let devices = ble::scan(duration).await?;

            let bhumi_devices: Vec<_> = devices.iter().filter(|d| d.is_bhumi).collect();
            let other_devices: Vec<_> = devices.iter().filter(|d| !d.is_bhumi).collect();

            if !bhumi_devices.is_empty() {
                println!("\nBhumi devices:");
                for device in &bhumi_devices {
                    let rssi = device.rssi.map(|r| format!(" ({}dBm)", r)).unwrap_or_default();
                    println!("  {} - {}{}", device.name, device.address, rssi);
                }
            }

            if !other_devices.is_empty() {
                println!("\nOther devices:");
                for device in &other_devices {
                    let rssi = device.rssi.map(|r| format!(" ({}dBm)", r)).unwrap_or_default();
                    println!("  {} - {}{}", device.name, device.address, rssi);
                }
            }

            if bhumi_devices.is_empty() {
                println!("\nNo Bhumi devices found.");
                println!("Make sure your device is powered on and in pairing mode.");
            }
        }
        BleCommands::Provision { device, ssid, password } => {
            let target = device.as_deref();
            println!("Provisioning device with WiFi credentials...");
            println!("  SSID: {}", ssid);

            ble::provision(target, &ssid, &password).await?;
            println!("WiFi credentials sent successfully!");
            println!("The device should now connect to your WiFi network.");
        }
        BleCommands::Reset { device } => {
            let target = device.as_deref();
            println!("Resetting device...");

            ble::reset(target).await?;
            println!("Device reset command sent.");
            println!("The device will clear all settings and restart.");
        }
    }

    Ok(())
}

// ============================================================================
// Relay Commands
// ============================================================================

async fn cmd_pair(token: &str, alias: &str) -> Result<(), Box<dyn std::error::Error>> {
    let home = get_home();
    let config = NodeConfig {
        kind: "cli-controller".to_string(),
        location: String::new(),
    };
    let mut node = Node::new(home, config);

    println!("Pairing with switch as \"{}\"...", alias);
    node.pair(RELAY_ADDR, token, alias).await?;
    println!("Paired successfully!");
    Ok(())
}

fn cmd_list() {
    let home = get_home();
    let config = NodeConfig {
        kind: "cli-controller".to_string(),
        location: String::new(),
    };
    let node = Node::new(home, config);

    let peers: Vec<_> = node.list_peers().collect();
    if peers.is_empty() {
        println!("No paired switches.");
        println!();
        println!("To pair with a switch, run:");
        println!("  switch-controller pair <invite-token> [alias]");
    } else {
        println!("Paired switches:");
        for (id52, peer) in peers {
            let id_short = data_encoding::BASE32_DNSSEC.encode(&id52[..10]);
            println!("  {} ({}...)", peer.alias, &id_short[..16]);
        }
    }
}

async fn run_switch(switch: &str, cmd: SwitchCommands) -> Result<(), Box<dyn std::error::Error>> {
    let home = get_home();
    let config = NodeConfig {
        kind: "cli-controller".to_string(),
        location: String::new(),
    };
    let mut node = Node::new(home, config);

    match cmd {
        SwitchCommands::Status => {
            let result = node.send(RELAY_ADDR, switch, "status", json!({})).await?;
            let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
            println!("Switch \"{}\" is {}", switch, if is_on { "ON" } else { "OFF" });
        }
        SwitchCommands::On => {
            let result = node.send(RELAY_ADDR, switch, "on", json!({})).await?;
            let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
            println!("Switch \"{}\" is now {}", switch, if is_on { "ON" } else { "OFF" });
        }
        SwitchCommands::Off => {
            let result = node.send(RELAY_ADDR, switch, "off", json!({})).await?;
            let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
            println!("Switch \"{}\" is now {}", switch, if is_on { "ON" } else { "OFF" });
        }
        SwitchCommands::Toggle => {
            let result = node.send(RELAY_ADDR, switch, "toggle", json!({})).await?;
            let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
            println!("Switch \"{}\" is now {}", switch, if is_on { "ON" } else { "OFF" });
        }
        SwitchCommands::Invite { action } => {
            match action {
                InviteCommands::Create { alias, role } => {
                    let result = node.send(RELAY_ADDR, switch, "invite/create", json!({
                        "alias": alias,
                        "role": role
                    })).await?;
                    let token = result.get("token").and_then(|v| v.as_str()).unwrap_or("?");
                    println!("Created invite for \"{}\" with role \"{}\":", alias, role);
                    println!();
                    println!("  {}", token);
                    println!();
                    println!("Share this token with them to pair.");
                }
                InviteCommands::List => {
                    let result = node.send(RELAY_ADDR, switch, "invite/list", json!({})).await?;
                    let invites = result.get("invites").and_then(|v| v.as_array());
                    match invites {
                        Some(list) if !list.is_empty() => {
                            println!("Open invites on \"{}\":", switch);
                            for invite in list {
                                let id = invite.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                                let alias = invite.get("alias").and_then(|v| v.as_str()).unwrap_or("?");
                                let role = invite.get("role").and_then(|v| v.as_str()).unwrap_or("?");
                                println!("  {} - \"{}\" ({})", id, alias, role);
                            }
                        }
                        _ => {
                            println!("No open invites on \"{}\".", switch);
                        }
                    }
                }
                InviteCommands::Delete { id } => {
                    node.send(RELAY_ADDR, switch, "invite/delete", json!({ "id": id })).await?;
                    println!("Deleted invite {}.", id);
                }
            }
        }
    }

    Ok(())
}
