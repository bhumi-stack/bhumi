//! Switch Owner - CLI to control smart switches
//!
//! Usage:
//!   OWNER_HOME=/tmp/switch-owner cargo run --example switch-owner -p bhumi-person -- <command>
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
use bhumi_person::{Person, json};

const RELAY_ADDR: &str = "127.0.0.1:8443";

fn get_home() -> PathBuf {
    std::env::var("OWNER_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/switch-owner"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let home = get_home();
    let mut person = Person::new(home);

    match args[1].as_str() {
        "pair" => {
            if args.len() < 3 {
                eprintln!("Usage: switch-owner pair <invite-token> [alias]");
                std::process::exit(1);
            }
            let token = &args[2];
            let alias = args.get(3).map(|s| s.as_str()).unwrap_or("switch");
            cmd_pair(&mut person, token, alias).await?;
        }
        "list" => {
            cmd_list(&person);
        }
        switch_alias => {
            if args.len() < 3 {
                eprintln!("Usage: switch-owner <switch> <command> [args...]");
                std::process::exit(1);
            }
            let cmd = &args[2];
            match cmd.as_str() {
                "status" => cmd_status(&mut person, switch_alias).await?,
                "on" => cmd_on(&mut person, switch_alias).await?,
                "off" => cmd_off(&mut person, switch_alias).await?,
                "toggle" => cmd_toggle(&mut person, switch_alias).await?,
                "invite" => {
                    if args.len() < 4 {
                        eprintln!("Usage: switch-owner <switch> invite <create|list|delete>");
                        std::process::exit(1);
                    }
                    match args[3].as_str() {
                        "create" => {
                            let alias = args.get(4).map(|s| s.as_str()).unwrap_or("user");
                            let role = args.get(5).map(|s| s.as_str()).unwrap_or("reader");
                            cmd_invite_create(&mut person, switch_alias, alias, role).await?;
                        }
                        "list" => cmd_invite_list(&mut person, switch_alias).await?,
                        "delete" => {
                            if args.len() < 5 {
                                eprintln!("Usage: switch-owner <switch> invite delete <id>");
                                std::process::exit(1);
                            }
                            cmd_invite_delete(&mut person, switch_alias, &args[4]).await?;
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
    eprintln!("Switch Owner - Control your smart switches");
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

async fn cmd_pair(person: &mut Person, token: &str, alias: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Pairing with switch as \"{}\"...", alias);
    person.pair(RELAY_ADDR, token, alias).await?;
    println!("Paired successfully!");
    Ok(())
}

fn cmd_list(person: &Person) {
    let peers: Vec<_> = person.list_peers().collect();
    if peers.is_empty() {
        println!("No paired switches.");
        println!();
        println!("To pair with a switch, run:");
        println!("  switch-owner pair <invite-token> [alias]");
    } else {
        println!("Paired switches:");
        for (id52, peer) in peers {
            let id_short = data_encoding::BASE32_DNSSEC.encode(&id52[..10]);
            println!("  {} ({}...)", peer.alias, &id_short[..16]);
        }
    }
}

async fn cmd_status(person: &mut Person, switch: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = person.send_command(RELAY_ADDR, switch, "status", json!({})).await?;
    let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
    println!("Switch \"{}\" is {}", switch, if is_on { "ON" } else { "OFF" });
    Ok(())
}

async fn cmd_on(person: &mut Person, switch: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = person.send_command(RELAY_ADDR, switch, "on", json!({})).await?;
    let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
    println!("Switch \"{}\" is now {}", switch, if is_on { "ON" } else { "OFF" });
    Ok(())
}

async fn cmd_off(person: &mut Person, switch: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = person.send_command(RELAY_ADDR, switch, "off", json!({})).await?;
    let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
    println!("Switch \"{}\" is now {}", switch, if is_on { "ON" } else { "OFF" });
    Ok(())
}

async fn cmd_toggle(person: &mut Person, switch: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = person.send_command(RELAY_ADDR, switch, "toggle", json!({})).await?;
    let is_on = result.get("is_on").and_then(|v| v.as_bool()).unwrap_or(false);
    println!("Switch \"{}\" is now {}", switch, if is_on { "ON" } else { "OFF" });
    Ok(())
}

async fn cmd_invite_create(person: &mut Person, switch: &str, alias: &str, role: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = person.send_command(RELAY_ADDR, switch, "invite/create", json!({
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

async fn cmd_invite_list(person: &mut Person, switch: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = person.send_command(RELAY_ADDR, switch, "invite/list", json!({})).await?;
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

async fn cmd_invite_delete(person: &mut Person, switch: &str, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    person.send_command(RELAY_ADDR, switch, "invite/delete", json!({ "id": id })).await?;
    println!("Deleted invite {}.", id);
    Ok(())
}
