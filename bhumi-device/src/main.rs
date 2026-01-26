mod commits;
mod connection;
mod identity;

use std::env;

use commits::CommitStore;
use connection::{connect_insecure, handshake, send_message, receive_message, send_ack};
use identity::load_or_create_identity;
use bhumi_proto::SEND_OK;

const RELAY_ADDR: &str = "127.0.0.1:8443";
const NUM_COMMITS: usize = 10;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("receive") | None => {
            run_receiver().await?;
        }
        Some("send") => {
            if args.len() < 4 {
                eprintln!("Usage: bhumi-device send <to_id52> <preimage_hex> [message]");
                eprintln!("  to_id52: recipient's 52-character identity");
                eprintln!("  preimage_hex: 64-character hex preimage from recipient");
                eprintln!("  message: optional message text (default: 'Hello from bhumi!')");
                std::process::exit(1);
            }
            let to_id52 = &args[2];
            let preimage_hex = &args[3];
            let message = args.get(4).map(|s| s.as_str()).unwrap_or("Hello from bhumi!");

            run_sender(to_id52, preimage_hex, message).await?;
        }
        Some(cmd) => {
            eprintln!("Unknown command: {}", cmd);
            eprintln!("Usage: bhumi-device [receive|send]");
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn run_receiver() -> Result<(), Box<dyn std::error::Error>> {
    // Load or create identity
    let (secret_key, public_key) = load_or_create_identity();

    // Load or create commits
    let commit_store = CommitStore::load_or_create(NUM_COMMITS);
    let commits = commit_store.get_commits();

    // Print preimages for sharing
    commit_store.print_preimages(&public_key.to_string());

    // Connect to relay
    let mut stream = connect_insecure(RELAY_ADDR).await?;

    // Handshake
    handshake(&mut stream, &secret_key, commits).await?;

    println!("\nWaiting for messages...\n");

    // Receive loop
    loop {
        match receive_message(&mut stream).await {
            Ok(deliver) => {
                println!("=== Received message (id={}) ===", deliver.msg_id);
                match String::from_utf8(deliver.payload.clone()) {
                    Ok(text) => println!("{}", text),
                    Err(_) => println!("(binary: {} bytes)", deliver.payload.len()),
                }
                println!("================================\n");

                // Send ACK with a simple response
                let response = b"Message received!".to_vec();
                send_ack(&mut stream, deliver.msg_id, response).await?;
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

async fn run_sender(
    to_id52_str: &str,
    preimage_hex: &str,
    message: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse recipient id52
    let to_id52_bytes = data_encoding::BASE32_DNSSEC
        .decode(to_id52_str.as_bytes())
        .map_err(|e| format!("invalid id52: {}", e))?;

    if to_id52_bytes.len() != 32 {
        return Err(format!("id52 must be 32 bytes, got {}", to_id52_bytes.len()).into());
    }
    let to_id52: [u8; 32] = to_id52_bytes.try_into().unwrap();

    // Parse preimage
    let preimage_bytes = data_encoding::HEXLOWER
        .decode(preimage_hex.as_bytes())
        .map_err(|e| format!("invalid preimage hex: {}", e))?;

    if preimage_bytes.len() != 32 {
        return Err(format!("preimage must be 32 bytes, got {}", preimage_bytes.len()).into());
    }
    let preimage: [u8; 32] = preimage_bytes.try_into().unwrap();

    // Load our identity (needed for handshake, though we're just sending)
    let (secret_key, _public_key) = load_or_create_identity();

    // We need commits for handshake even if we're just sending
    let commit_store = CommitStore::load_or_create(NUM_COMMITS);
    let commits = commit_store.get_commits();

    // Connect to relay
    let mut stream = connect_insecure(RELAY_ADDR).await?;

    // Handshake
    handshake(&mut stream, &secret_key, commits).await?;

    // Send message
    println!("\nSending message to {}...", to_id52_str);
    let result = send_message(&mut stream, to_id52, preimage, message.as_bytes().to_vec()).await?;

    if result.status == SEND_OK {
        println!("Message delivered!");
        if !result.payload.is_empty() {
            match String::from_utf8(result.payload) {
                Ok(text) => println!("Response: {}", text),
                Err(e) => println!("Response: (binary, {} bytes)", e.into_bytes().len()),
            }
        }
    } else {
        println!("Failed to deliver message (status={})", result.status);
    }

    Ok(())
}
