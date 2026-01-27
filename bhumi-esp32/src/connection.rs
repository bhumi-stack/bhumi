//! Blocking TCP connection for ESP32 (TLS disabled for dev)
//!
//! Uses std::net for blocking TCP I/O.

use fastn_id52::SecretKey;
use bhumi_proto::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use log::*;

/// Received message from relay
pub struct ReceivedMessage {
    pub msg_id: u32,
    pub preimage: [u8; 32],
    pub payload: Vec<u8>,
    msg_type: Option<u8>,
}

impl ReceivedMessage {
    /// Check if this is a handshake init message
    pub fn is_handshake(&self) -> bool {
        self.msg_type == Some(DEV_HANDSHAKE_INIT)
    }
}

/// Blocking TCP connection to bhumi relay
pub struct Connection {
    stream: TcpStream,
}

impl Connection {
    /// Connect to relay and authenticate
    pub fn connect(
        addr: &str,
        secret_key: &SecretKey,
        commits: Vec<[u8; 32]>,
    ) -> anyhow::Result<Self> {
        info!("Connecting to {}", addr);

        // Connect TCP
        let mut stream = TcpStream::connect(addr)?;
        info!("TCP connected");

        // Read HELLO
        let frame = Frame::read_from(&mut stream)?;
        if frame.msg_type != MSG_HELLO {
            anyhow::bail!("expected HELLO, got msg_type {}", frame.msg_type);
        }
        let hello = Hello::from_bytes(&frame.payload)?;
        info!("Received HELLO (nonce=0x{:08x})", hello.nonce);

        // Create I_AM response
        let mut to_sign = Vec::with_capacity(36);
        to_sign.extend_from_slice(&hello.nonce.to_be_bytes());
        to_sign.extend_from_slice(&secret_key.public_key().to_bytes());

        let signature = secret_key.sign(&to_sign);
        let i_am = IAm::new(
            secret_key.public_key().to_bytes(),
            signature.to_bytes(),
            commits,
        );

        // Send I_AM
        let frame = Frame::i_am(&i_am);
        frame.write_to(&mut stream)?;
        stream.flush()?;
        info!("Sent I_AM, registered {} commits", i_am.commits.len());

        Ok(Self { stream })
    }

    /// Receive a message (blocking)
    pub fn receive(&mut self) -> anyhow::Result<ReceivedMessage> {
        let frame = Frame::read_from(&mut self.stream)?;

        if frame.msg_type != MSG_DELIVER {
            anyhow::bail!("expected DELIVER, got msg_type {}", frame.msg_type);
        }

        let deliver = Deliver::from_bytes(&frame.payload)?;
        info!("Received DELIVER msg_id={}", deliver.msg_id);

        // Check for handshake message type
        let msg_type = if !deliver.payload.is_empty() {
            Some(deliver.payload[0])
        } else {
            None
        };

        Ok(ReceivedMessage {
            msg_id: deliver.msg_id,
            preimage: deliver.preimage,
            payload: deliver.payload,
            msg_type,
        })
    }

    /// Send ACK response
    pub fn send_ack(&mut self, msg_id: u32, payload: Vec<u8>) -> anyhow::Result<()> {
        let ack = Ack { msg_id, payload };
        let frame = Frame::ack(&ack);
        frame.write_to(&mut self.stream)?;
        self.stream.flush()?;
        info!("Sent ACK msg_id={}", msg_id);
        Ok(())
    }

    /// Update commits with relay
    pub fn update_commits(&mut self, commits: Vec<[u8; 32]>) -> anyhow::Result<()> {
        let update = UpdateCommits { commits };
        let frame = Frame::update_commits(&update);
        frame.write_to(&mut self.stream)?;
        self.stream.flush()?;
        info!("Sent UPDATE_COMMITS ({} commits)", update.commits.len());
        Ok(())
    }
}
