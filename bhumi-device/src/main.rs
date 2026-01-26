mod connection;
mod identity;
mod state;

use std::env;
use std::path::PathBuf;

use connection::{connect_insecure, handshake, send_message, receive_message, send_ack, update_commits};
use identity::{load_or_create_identity, bhumi_home};
use state::{DeviceState, create_invite_token, parse_invite_token};
use bhumi_proto::{
    SEND_OK, HandshakeInit, HandshakeComplete, DeviceMessage, DeviceMessageResponse,
    DEV_HANDSHAKE_INIT, DEV_MESSAGE, parse_device_msg_type, HANDSHAKE_ACCEPTED,
};

const RELAY_ADDR: &str = "127.0.0.1:8443";

fn state_path() -> PathBuf {
    bhumi_home().join("state.json")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("invite") => {
            match args.get(2).map(|s| s.as_str()) {
                Some("create") => {
                    let alias = args.get(3).map(|s| s.as_str()).unwrap_or("peer");
                    cmd_invite_create(alias)?;
                }
                Some("accept") => {
                    if args.len() < 4 {
                        eprintln!("Usage: bhumi-device invite accept <token> [alias]");
                        std::process::exit(1);
                    }
                    let token = &args[3];
                    let alias = args.get(4).map(|s| s.as_str()).unwrap_or("peer");
                    cmd_invite_accept(token, alias).await?;
                }
                Some("list") => {
                    cmd_invite_list()?;
                }
                _ => {
                    eprintln!("Usage: bhumi-device invite <create|accept|list>");
                    std::process::exit(1);
                }
            }
        }
        Some("peer") => {
            match args.get(2).map(|s| s.as_str()) {
                Some("list") => {
                    cmd_peer_list()?;
                }
                _ => {
                    eprintln!("Usage: bhumi-device peer list");
                    std::process::exit(1);
                }
            }
        }
        Some("listen") => {
            cmd_listen().await?;
        }
        Some("send") => {
            if args.len() < 4 {
                eprintln!("Usage: bhumi-device send <peer_alias> <message>");
                std::process::exit(1);
            }
            let peer_alias = &args[2];
            let message = &args[3];
            cmd_send(peer_alias, message).await?;
        }
        Some("status") => {
            cmd_status()?;
        }
        Some(cmd) => {
            eprintln!("Unknown command: {}", cmd);
            print_usage();
            std::process::exit(1);
        }
        None => {
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    eprintln!("Bhumi Device - P2P messaging");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  invite create <alias>       Create invite token for a new peer");
    eprintln!("  invite accept <token> [alias]  Accept invite and initiate handshake");
    eprintln!("  invite list                 List pending invites");
    eprintln!("  peer list                   List established peers");
    eprintln!("  listen                      Listen for incoming messages");
    eprintln!("  send <peer_alias> <message> Send message to peer");
    eprintln!("  status                      Show device status");
}

/// Create an invite token
fn cmd_invite_create(alias: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (secret_key, public_key) = load_or_create_identity();
    let mut state = DeviceState::load(&state_path());

    let (invite, _commit) = state.create_invite(alias);
    state.save(&state_path());

    let token = create_invite_token(&public_key.to_bytes(), &invite.preimage);

    println!("Created invite for \"{}\"", alias);
    println!();
    println!("Share this token with {}:", alias);
    println!("  {}", token);
    println!();
    println!("They can accept with:");
    println!("  bhumi-device invite accept {} \"{}\"", token, public_key);

    Ok(())
}

/// Accept an invite token and initiate handshake
async fn cmd_invite_accept(token: &str, alias: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (their_id52, their_preimage) = parse_invite_token(token)
        .map_err(|e| format!("Invalid token: {}", e))?;

    let their_id52_str = data_encoding::BASE32_DNSSEC.encode(&their_id52);
    println!("Accepting invite from: {}", their_id52_str);

    let (secret_key, public_key) = load_or_create_identity();
    let mut state = DeviceState::load(&state_path());

    // Store in pending_peers
    let (my_preimage, my_commit) = state.accept_invite(their_id52, their_preimage, alias);
    state.save(&state_path());

    // Connect to relay
    let mut stream = connect_insecure(RELAY_ADDR).await?;
    let commits = state.get_all_commits();
    handshake(&mut stream, &secret_key, commits).await?;

    // Send HANDSHAKE_INIT
    let init = HandshakeInit {
        sender_id52: public_key.to_bytes(),
        preimage_for_peer: my_preimage,
        relay_url: RELAY_ADDR.to_string(),
    };

    println!("Sending HANDSHAKE_INIT...");
    let result = send_message(&mut stream, their_id52, their_preimage, init.to_bytes()).await?;

    if result.status != SEND_OK {
        println!("Handshake failed: status={}", result.status);
        // Remove from pending_peers
        state.pending_peers.remove(&their_id52);
        state.save(&state_path());
        return Ok(());
    }

    // Parse HANDSHAKE_COMPLETE
    let complete = HandshakeComplete::from_bytes(&result.payload)?;

    if complete.status != HANDSHAKE_ACCEPTED {
        println!("Handshake rejected by peer");
        state.pending_peers.remove(&their_id52);
        state.save(&state_path());
        return Ok(());
    }

    // Complete handshake
    state.complete_handshake_as_acceptor(&their_id52, complete.preimage_for_peer, Some(complete.relay_url));
    state.save(&state_path());

    println!("Handshake complete! \"{}\" is now a peer.", alias);

    Ok(())
}

/// List pending invites
fn cmd_invite_list() -> Result<(), Box<dyn std::error::Error>> {
    let state = DeviceState::load(&state_path());

    if state.invites.is_empty() {
        println!("No pending invites.");
    } else {
        println!("Pending invites:");
        for (preimage, invite) in &state.invites {
            let preimage_hex = data_encoding::HEXLOWER.encode(preimage);
            println!("  {} (preimage: {}...)", invite.alias, &preimage_hex[..16]);
        }
    }

    if !state.pending_peers.is_empty() {
        println!();
        println!("Pending peer connections (awaiting handshake completion):");
        for (id52, pending) in &state.pending_peers {
            let id52_str = data_encoding::BASE32_DNSSEC.encode(id52);
            println!("  {} ({})", pending.alias, &id52_str[..20]);
        }
    }

    Ok(())
}

/// List established peers
fn cmd_peer_list() -> Result<(), Box<dyn std::error::Error>> {
    let state = DeviceState::load(&state_path());

    if state.peers.is_empty() {
        println!("No established peers.");
        println!();
        println!("To add a peer:");
        println!("  1. Run 'bhumi-device invite create <alias>' to create a token");
        println!("  2. Share the token with your peer");
        println!("  3. Have them run 'bhumi-device invite accept <token>'");
    } else {
        println!("Established peers:");
        for (id52, peer) in &state.peers {
            let id52_str = data_encoding::BASE32_DNSSEC.encode(id52);
            let can_send = if peer.their_preimage.is_some() { "✓" } else { "✗" };
            let can_recv = if !peer.issued_preimages.is_empty() { "✓" } else { "✗" };
            println!("  {} ({}) send:{} recv:{}", peer.alias, &id52_str[..20], can_send, can_recv);
        }
    }

    Ok(())
}

/// Listen for incoming messages
async fn cmd_listen() -> Result<(), Box<dyn std::error::Error>> {
    let (secret_key, public_key) = load_or_create_identity();
    let mut state = DeviceState::load(&state_path());

    println!("Device: {}", public_key);
    println!();

    // Connect to relay
    let mut stream = connect_insecure(RELAY_ADDR).await?;
    let commits = state.get_all_commits();

    if commits.is_empty() {
        println!("Warning: No commits to register. Create an invite first.");
    }

    handshake(&mut stream, &secret_key, commits).await?;

    println!("Listening for messages...\n");

    loop {
        match receive_message(&mut stream).await {
            Ok(deliver) => {
                // Parse device protocol message
                let msg_type = parse_device_msg_type(&deliver.payload);

                match msg_type {
                    Some(DEV_HANDSHAKE_INIT) => {
                        handle_handshake_init(&mut stream, &mut state, &public_key, deliver.msg_id, &deliver.preimage, &deliver.payload).await?;
                        state.save(&state_path());
                    }
                    Some(DEV_MESSAGE) => {
                        handle_message(&mut stream, &mut state, deliver.msg_id, &deliver.preimage, &deliver.payload).await?;
                        state.save(&state_path());
                    }
                    _ => {
                        println!("Unknown device message type: {:?}", msg_type);
                        // Send empty ACK
                        send_ack(&mut stream, deliver.msg_id, vec![]).await?;
                    }
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    println!("Connection closed by relay");
                    break;
                }
                return Err(e.into());
            }
        }
    }

    Ok(())
}

async fn handle_handshake_init<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    state: &mut DeviceState,
    my_public_key: &fastn_id52::PublicKey,
    msg_id: u32,
    preimage: &[u8; 32],  // The preimage from DELIVER - identifies which invite
    payload: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let init = HandshakeInit::from_bytes(payload)?;
    let sender_id52_str = data_encoding::BASE32_DNSSEC.encode(&init.sender_id52);

    println!("=== HANDSHAKE_INIT from {} ===", &sender_id52_str[..20]);

    // Use the preimage from DELIVER to look up the matching invite
    if let Some((new_preimage, new_commit)) = state.complete_handshake_as_inviter(
        preimage,
        init.sender_id52,
        init.preimage_for_peer,
        Some(init.relay_url),
    ) {
        println!("Handshake accepted!");

        // Send HANDSHAKE_COMPLETE
        let complete = HandshakeComplete {
            status: HANDSHAKE_ACCEPTED,
            preimage_for_peer: new_preimage,
            relay_url: RELAY_ADDR.to_string(),
        };

        send_ack(stream, msg_id, complete.to_bytes()).await?;

        // Register the new commit with the relay
        update_commits(stream, vec![new_commit]).await?;

        return Ok(());
    }

    println!("No matching invite found for preimage, rejecting");
    let complete = HandshakeComplete {
        status: 1, // rejected
        preimage_for_peer: [0u8; 32],
        relay_url: String::new(),
    };
    send_ack(stream, msg_id, complete.to_bytes()).await?;

    Ok(())
}

async fn handle_message<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    state: &mut DeviceState,
    msg_id: u32,
    preimage: &[u8; 32],  // The preimage from DELIVER - identifies which peer sent this
    payload: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let msg = DeviceMessage::from_bytes(payload)?;

    // Look up which peer sent this message based on the preimage they used
    let sender_info = state.lookup_preimage(preimage);

    let (peer_id52, peer_alias) = match &sender_info {
        Some(state::PreimageLookup::Peer(id52, peer)) => {
            (Some(*id52), Some(peer.alias.clone()))
        }
        _ => (None, None),
    };

    // Display message with sender info
    println!("=== MESSAGE ===");
    if let Some(alias) = &peer_alias {
        println!("From: {}", alias);
    }
    match msg.content_type {
        0 => {
            let text = String::from_utf8_lossy(&msg.content);
            println!("{}", text);
        }
        _ => {
            println!("(binary: {} bytes)", msg.content.len());
        }
    }
    println!("===============\n");

    // Generate new preimage for this peer if we know who they are
    let (next_preimage, new_commit) = if let Some(id52) = peer_id52 {
        match state.consume_and_renew_preimage(&id52, preimage) {
            Some((new_preimage, new_commit)) => (new_preimage, Some(new_commit)),
            None => ([0u8; 32], None),
        }
    } else {
        println!("Warning: Unknown sender (preimage not found)");
        ([0u8; 32], None)
    };

    let response = DeviceMessageResponse {
        status: 0,
        next_preimage,
        relay_url: RELAY_ADDR.to_string(),
        content: b"Message received".to_vec(),
    };

    send_ack(stream, msg_id, response.to_bytes()).await?;

    // Register the new commit with the relay if we generated one
    if let Some(commit) = new_commit {
        update_commits(stream, vec![commit]).await?;
    }

    Ok(())
}

/// Send a message to a peer
async fn cmd_send(peer_alias: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (secret_key, _public_key) = load_or_create_identity();
    let mut state = DeviceState::load(&state_path());

    // Find peer
    let (peer_id52, peer) = state.find_peer_by_alias(peer_alias)
        .ok_or_else(|| format!("Peer '{}' not found", peer_alias))?;

    let their_preimage = peer.their_preimage
        .ok_or("No preimage available to send to this peer")?;

    println!("Sending to \"{}\"...", peer_alias);

    // Connect to relay
    let mut stream = connect_insecure(RELAY_ADDR).await?;
    let commits = state.get_all_commits();
    handshake(&mut stream, &secret_key, commits).await?;

    // Send message
    let msg = DeviceMessage::text(RELAY_ADDR.to_string(), message);
    let result = send_message(&mut stream, peer_id52, their_preimage, msg.to_bytes()).await?;

    if result.status != SEND_OK {
        println!("Send failed: status={}", result.status);
        return Ok(());
    }

    // Parse response
    let response = DeviceMessageResponse::from_bytes(&result.payload)?;

    println!("Delivered!");
    if !response.content.is_empty() {
        let text = String::from_utf8_lossy(&response.content);
        println!("Response: {}", text);
    }

    // Update peer's preimage for next message
    if response.next_preimage != [0u8; 32] {
        state.update_peer_preimage(&peer_id52, response.next_preimage);
        state.save(&state_path());
    }

    Ok(())
}

/// Show device status
fn cmd_status() -> Result<(), Box<dyn std::error::Error>> {
    let (_secret_key, public_key) = load_or_create_identity();
    let state = DeviceState::load(&state_path());

    println!("Device ID: {}", public_key);
    println!("Data dir:  {}", bhumi_home().display());
    println!();
    println!("Pending invites: {}", state.invites.len());
    println!("Pending peers:   {}", state.pending_peers.len());
    println!("Established:     {}", state.peers.len());

    Ok(())
}
