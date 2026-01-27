//! Connection to relay (TLS disabled for dev)

use tokio::net::TcpStream;

use bhumi_proto::{Frame, Hello, IAm, Send as SendMsg, Deliver, Ack, SendResult, UpdateCommits, MSG_HELLO, MSG_DELIVER, MSG_SEND_RESULT};
use bhumi_proto::async_io::{read_frame, write_frame};
use fastn_id52::SecretKey;

/// A connection to a Bhumi relay
pub struct Connection {
    stream: TcpStream,
}

impl Connection {
    /// Connect anonymously for send-only mode (no I_AM, sender stays anonymous)
    pub async fn connect_anonymous(addr: &str) -> std::io::Result<Self> {
        let mut stream = TcpStream::connect(addr).await?;

        // Read HELLO (required to establish connection)
        let frame = read_frame(&mut stream).await?;
        if frame.msg_type != MSG_HELLO {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("expected HELLO, got 0x{:04x}", frame.msg_type),
            ));
        }
        // We don't send I_AM - sender remains anonymous to relay

        Ok(Self { stream })
    }

    /// Connect to a relay with identity (for devices that need to receive messages)
    pub async fn connect(
        addr: &str,
        secret_key: &SecretKey,
        commits: Vec<[u8; 32]>,
    ) -> std::io::Result<Self> {
        let mut stream = TcpStream::connect(addr).await?;

        // Perform full handshake with I_AM
        Self::handshake(&mut stream, secret_key, commits).await?;

        Ok(Self { stream })
    }

    async fn handshake(
        stream: &mut TcpStream,
        secret_key: &SecretKey,
        commits: Vec<[u8; 32]>,
    ) -> std::io::Result<()> {
        // Read HELLO
        let frame = read_frame(stream).await?;
        if frame.msg_type != MSG_HELLO {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("expected HELLO, got 0x{:04x}", frame.msg_type),
            ));
        }

        let hello = Hello::from_bytes(&frame.payload)?;

        // Create I_AM
        let public_key = secret_key.public_key();
        let id52 = public_key.to_bytes();

        // Sign(nonce || id52)
        let mut msg = Vec::with_capacity(4 + 32);
        msg.extend_from_slice(&hello.nonce.to_be_bytes());
        msg.extend_from_slice(&id52);
        let signature = secret_key.sign(&msg);

        let i_am = IAm::new(id52, signature.to_bytes(), commits);

        write_frame(stream, &Frame::i_am(&i_am)).await?;

        Ok(())
    }

    /// Send a message to another device and wait for response
    pub async fn send(
        &mut self,
        to_id52: [u8; 32],
        preimage: [u8; 32],
        payload: Vec<u8>,
    ) -> std::io::Result<SendResult> {
        let send = SendMsg {
            to_id52,
            preimage,
            payload,
        };

        write_frame(&mut self.stream, &Frame::send(&send)).await?;

        // Wait for SEND_RESULT
        let frame = read_frame(&mut self.stream).await?;
        if frame.msg_type != MSG_SEND_RESULT {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("expected SEND_RESULT, got 0x{:04x}", frame.msg_type),
            ));
        }

        SendResult::from_bytes(&frame.payload)
    }

    /// Send an ACK response to a delivered message
    pub async fn send_ack(&mut self, msg_id: u32, payload: Vec<u8>) -> std::io::Result<()> {
        let ack = Ack { msg_id, payload };
        write_frame(&mut self.stream, &Frame::ack(&ack)).await
    }

    /// Send UPDATE_COMMITS to add new commits while connected
    pub async fn update_commits(&mut self, commits: Vec<[u8; 32]>) -> std::io::Result<()> {
        let update = UpdateCommits { commits };
        write_frame(&mut self.stream, &Frame::update_commits(&update)).await
    }

    /// Wait for and receive a delivered message
    pub async fn receive_deliver(&mut self) -> std::io::Result<Deliver> {
        let frame = read_frame(&mut self.stream).await?;

        if frame.msg_type != MSG_DELIVER {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("expected DELIVER, got 0x{:04x}", frame.msg_type),
            ));
        }

        Deliver::from_bytes(&frame.payload)
    }
}
