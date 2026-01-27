//! Switch Controller - CLI to control smart switches
//!
//! Usage:
//!   CONTROLLER_HOME=/tmp/switch-controller cargo run --example switch-controller -p bhumi-node -- <command>
//!
//! Commands:
//!   pair <invite-token> [alias]  - Pair with a switch
//!   list                         - List paired switches
//!   <switch> status              - Get switch status
//!   <switch> on                  - Turn switch on
//!   <switch> off                 - Turn switch off
//!   <switch> toggle              - Toggle switch
//!   <switch> invite create [alias] [role]  - Create invite (role: owner/writer/reader)
//!   <switch> invite list         - List open invites
//!   <switch> invite delete <id>  - Delete an invite

use std::path::PathBuf;
use bhumi_node::{Node, NodeConfig, json};

const RELAY_ADDR: &str = "64.227.143.197:8443";

fn get_home() -> PathBuf {
    std::env::var("CONTROLLER_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/switch-controller"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let home = get_home();
    let config = NodeConfig {
        kind: "cli-controller".to_string(),
        location: String::new(),
    };
    let mut node = Node::new(home, config);

    match args[1].as_str() {
        "pair" => {
            if args.len() < 3 {
                eprintln!("Usage: switch-controller pair <invite-token> [alias]");
                std::process::exit(1);
            }
            let token = &args[2];
            let alias = args.get(3).map(|s| s.as_str()).unwrap_or("switch");
            cmd_pair(&mut node, token, alias).await?;
        }
        "list" => {
            cmd_list(&node);
        }
        switch_alias => {
            if args.len() < 3 {
                eprintln!("Usage: switch-controller <switch> <command> [args...]");
                std::process::exit(1);
            }
            let cmd = &args[2];
            match cmd.as_str() {
                "status" => cmd_status(&mut node, switch_alias).await?,
                "on" => cmd_on(&mut node, switch_alias).await?,
                "off" => cmd_off(&mut node, switch_alias).await?,
                "toggle" => cmd_toggle(&mut node, switch_alias).await?,
                "invite" => {
                    if args.len() < 4 {
                        eprintln!("Usage: switch-controller <switch> invite <create|list|delete>");
                        std::process::exit(1);
                    }
                    match args[3].as_str() {
                        "create" => {
                            let alias = args.get(4).map(|s| s.as_str()).unwrap_or("user");
                            let role = args.get(5).map(|s| s.as_str()).unwrap_or("reader");
                            cmd_invite_create(&mut node, switch_alias, alias, role).await?;
                        }
                        "list" => cmd_invite_list(&mut node, switch_alias).await?,
                        "delete" => {
                            if args.len() < 5 {
                                eprintln!("Usage: switch-controller <switch> invite delete <id>");
                                std::process::exit(1);
                            }
                            cmd_invite_delete(&mut node, switch_alias, &args[4]).await?;
                        }
                        _ => {
                            eprintln!("Unknown invite command: {}", args[3]);
                            std::process::exit(1);
                        }
                    }
                }
                _ => {
                    eprintln!("Unknown command: {}", cmd);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

fn print_usage() {
    eprintln!("Switch Controller - Control your smart switches");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  pair <token> [alias]     Pair with a switch");
    eprintln!("  list                     List paired switches");
    eprintln!("  <switch> status          Get switch status");
    eprintln!("  <switch> on              Turn switch on");
    eprintln!("  <switch> off             Turn switch off");
    eprintln!("  <switch> toggle          Toggle switch");
    eprintln!("  <switch> invite create [alias] [role]");
    eprintln!("  <switch> invite list");
    eprintln!("  <switch> invite delete <id>");
}

async fn cmd_pair(node: &mut Node, token: &str, alias: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Pairing with switch as \"{}\"...", alias);
    node.pair(RELAY_ADDR, token, alias).await?;
    println!("Paired successfully!");
    Ok(())
}

fn cmd_list(node: &Node) {
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

async fn cmd_status(node: &mut Node, switch: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = node.send(RELAY_ADDR, switch, "status", json!({})).await?;
    let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
    println!("Switch \"{}\" is {}", switch, if is_on { "ON" } else { "OFF" });
    Ok(())
}

async fn cmd_on(node: &mut Node, switch: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = node.send(RELAY_ADDR, switch, "on", json!({})).await?;
    let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
    println!("Switch \"{}\" is now {}", switch, if is_on { "ON" } else { "OFF" });
    Ok(())
}

async fn cmd_off(node: &mut Node, switch: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = node.send(RELAY_ADDR, switch, "off", json!({})).await?;
    let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
    println!("Switch \"{}\" is now {}", switch, if is_on { "ON" } else { "OFF" });
    Ok(())
}

async fn cmd_toggle(node: &mut Node, switch: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = node.send(RELAY_ADDR, switch, "toggle", json!({})).await?;
    let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
    println!("Switch \"{}\" is now {}", switch, if is_on { "ON" } else { "OFF" });
    Ok(())
}

async fn cmd_invite_create(node: &mut Node, switch: &str, alias: &str, role: &str) -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}

async fn cmd_invite_list(node: &mut Node, switch: &str) -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}

async fn cmd_invite_delete(node: &mut Node, switch: &str, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    node.send(RELAY_ADDR, switch, "invite/delete", json!({ "id": id })).await?;
    println!("Deleted invite {}.", id);
    Ok(())
}
