//! Bhumi wire protocol - message types and framing

use std::io::{self, Read, Write};

// Message types
pub const MSG_HELLO: u16 = 0x0001;
pub const MSG_I_AM: u16 = 0x0002;
pub const MSG_SEND: u16 = 0x0003;
pub const MSG_DELIVER: u16 = 0x0004;
pub const MSG_ACK: u16 = 0x0005;
pub const MSG_KEEPALIVE: u16 = 0x0006;
pub const MSG_SEND_RESULT: u16 = 0x0007;
pub const MSG_UPDATE_COMMITS: u16 = 0x0008;

// SEND_RESULT status codes
pub const SEND_OK: u8 = 0;
pub const SEND_ERR_NOT_CONNECTED: u8 = 1;
pub const SEND_ERR_INVALID_PREIMAGE: u8 = 2;
pub const SEND_ERR_TIMEOUT: u8 = 3;
pub const SEND_ERR_DISCONNECTED: u8 = 4;

// Device protocol message types (inside encrypted payload)
pub const DEV_HANDSHAKE_INIT: u8 = 0x01;
pub const DEV_HANDSHAKE_COMPLETE: u8 = 0x02;
pub const DEV_MESSAGE: u8 = 0x10;
pub const DEV_MESSAGE_RESPONSE: u8 = 0x11;

// Handshake status codes
pub const HANDSHAKE_ACCEPTED: u8 = 0;
pub const HANDSHAKE_REJECTED: u8 = 1;

/// HELLO message sent by relay on connection
#[derive(Debug, Clone)]
pub struct Hello {
    pub version: u8,
    pub nonce: u32,
    pub max_payload_size: u32,
}

impl Hello {
    pub fn new(nonce: u32, max_payload_size: u32) -> Self {
        Self {
            version: 1,
            nonce,
            max_payload_size,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(9);
        buf.push(self.version);
        buf.extend_from_slice(&self.nonce.to_be_bytes());
        buf.extend_from_slice(&self.max_payload_size.to_be_bytes());
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 9 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "HELLO too short"));
        }
        Ok(Self {
            version: data[0],
            nonce: u32::from_be_bytes([data[1], data[2], data[3], data[4]]),
            max_payload_size: u32::from_be_bytes([data[5], data[6], data[7], data[8]]),
        })
    }
}

/// Recent response for relay cache portability
#[derive(Debug, Clone)]
pub struct RecentResponse {
    pub preimage: [u8; 32],
    pub response: Vec<u8>,
}

/// I_AM message sent by device to authenticate and register commits
#[derive(Debug, Clone)]
pub struct IAm {
    pub id52: [u8; 32],       // Ed25519 public key
    pub signature: [u8; 64],  // Sign(nonce || id52)
    pub commits: Vec<[u8; 32]>, // SHA256 hashes of preimages
    pub recent_responses: Vec<RecentResponse>, // For relay cache portability
}

impl IAm {
    pub fn new(id52: [u8; 32], signature: [u8; 64], commits: Vec<[u8; 32]>) -> Self {
        Self {
            id52,
            signature,
            commits,
            recent_responses: Vec::new(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let commit_count = self.commits.len() as u16;
        let response_count = self.recent_responses.len() as u16;

        // Calculate size
        let mut size = 32 + 64 + 2 + self.commits.len() * 32 + 2;
        for resp in &self.recent_responses {
            size += 32 + 4 + resp.response.len();
        }

        let mut buf = Vec::with_capacity(size);
        buf.extend_from_slice(&self.id52);
        buf.extend_from_slice(&self.signature);
        buf.extend_from_slice(&commit_count.to_be_bytes());
        for commit in &self.commits {
            buf.extend_from_slice(commit);
        }
        buf.extend_from_slice(&response_count.to_be_bytes());
        for resp in &self.recent_responses {
            buf.extend_from_slice(&resp.preimage);
            buf.extend_from_slice(&(resp.response.len() as u32).to_be_bytes());
            buf.extend_from_slice(&resp.response);
        }
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 32 + 64 + 2 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "I_AM too short"));
        }

        let id52: [u8; 32] = data[0..32].try_into().unwrap();
        let signature: [u8; 64] = data[32..96].try_into().unwrap();
        let commit_count = u16::from_be_bytes([data[96], data[97]]) as usize;

        let commits_end = 98 + commit_count * 32;
        if data.len() < commits_end + 2 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "I_AM commits truncated"));
        }

        let mut commits = Vec::with_capacity(commit_count);
        for i in 0..commit_count {
            let start = 98 + i * 32;
            let commit: [u8; 32] = data[start..start + 32].try_into().unwrap();
            commits.push(commit);
        }

        let response_count = u16::from_be_bytes([data[commits_end], data[commits_end + 1]]) as usize;
        let mut recent_responses = Vec::with_capacity(response_count);
        let mut pos = commits_end + 2;

        for _ in 0..response_count {
            if data.len() < pos + 36 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "I_AM responses truncated"));
            }
            let preimage: [u8; 32] = data[pos..pos + 32].try_into().unwrap();
            let resp_len = u32::from_be_bytes([data[pos + 32], data[pos + 33], data[pos + 34], data[pos + 35]]) as usize;
            pos += 36;

            if data.len() < pos + resp_len {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "I_AM response data truncated"));
            }
            let response = data[pos..pos + resp_len].to_vec();
            pos += resp_len;

            recent_responses.push(RecentResponse { preimage, response });
        }

        Ok(Self { id52, signature, commits, recent_responses })
    }
}

/// SEND message - deliver a message to a recipient
#[derive(Debug, Clone)]
pub struct Send {
    pub to_id52: [u8; 32],
    pub preimage: [u8; 32],
    pub payload: Vec<u8>,
}

impl Send {
    pub fn to_bytes(&self) -> Vec<u8> {
        let payload_len = self.payload.len() as u32;
        let mut buf = Vec::with_capacity(32 + 32 + 4 + self.payload.len());
        buf.extend_from_slice(&self.to_id52);
        buf.extend_from_slice(&self.preimage);
        buf.extend_from_slice(&payload_len.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 32 + 32 + 4 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "SEND too short"));
        }

        let to_id52: [u8; 32] = data[0..32].try_into().unwrap();
        let preimage: [u8; 32] = data[32..64].try_into().unwrap();
        let payload_len = u32::from_be_bytes([data[64], data[65], data[66], data[67]]) as usize;

        if data.len() < 68 + payload_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "SEND payload truncated"));
        }

        let payload = data[68..68 + payload_len].to_vec();
        Ok(Self { to_id52, preimage, payload })
    }
}

/// UPDATE_COMMITS - add commits to an existing connection
#[derive(Debug, Clone)]
pub struct UpdateCommits {
    pub commits: Vec<[u8; 32]>,
}

impl UpdateCommits {
    pub fn to_bytes(&self) -> Vec<u8> {
        let commit_count = self.commits.len() as u16;
        let mut buf = Vec::with_capacity(2 + self.commits.len() * 32);
        buf.extend_from_slice(&commit_count.to_be_bytes());
        for commit in &self.commits {
            buf.extend_from_slice(commit);
        }
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 2 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "UPDATE_COMMITS too short"));
        }

        let commit_count = u16::from_be_bytes([data[0], data[1]]) as usize;

        if data.len() < 2 + commit_count * 32 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "UPDATE_COMMITS truncated"));
        }

        let mut commits = Vec::with_capacity(commit_count);
        for i in 0..commit_count {
            let start = 2 + i * 32;
            let commit: [u8; 32] = data[start..start + 32].try_into().unwrap();
            commits.push(commit);
        }

        Ok(Self { commits })
    }
}

/// DELIVER message - relay forwards message to recipient
#[derive(Debug, Clone)]
pub struct Deliver {
    pub msg_id: u32,
    pub payload: Vec<u8>,
}

impl Deliver {
    pub fn to_bytes(&self) -> Vec<u8> {
        let payload_len = self.payload.len() as u32;
        let mut buf = Vec::with_capacity(4 + 4 + self.payload.len());
        buf.extend_from_slice(&self.msg_id.to_be_bytes());
        buf.extend_from_slice(&payload_len.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 8 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "DELIVER too short"));
        }

        let msg_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let payload_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;

        if data.len() < 8 + payload_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "DELIVER payload truncated"));
        }

        let payload = data[8..8 + payload_len].to_vec();
        Ok(Self { msg_id, payload })
    }
}

/// ACK message - recipient's response to DELIVER
#[derive(Debug, Clone)]
pub struct Ack {
    pub msg_id: u32,
    pub payload: Vec<u8>,  // encrypted response for sender
}

impl Ack {
    pub fn to_bytes(&self) -> Vec<u8> {
        let payload_len = self.payload.len() as u32;
        let mut buf = Vec::with_capacity(4 + 4 + self.payload.len());
        buf.extend_from_slice(&self.msg_id.to_be_bytes());
        buf.extend_from_slice(&payload_len.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 8 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "ACK too short"));
        }

        let msg_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let payload_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;

        if data.len() < 8 + payload_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "ACK payload truncated"));
        }

        let payload = data[8..8 + payload_len].to_vec();
        Ok(Self { msg_id, payload })
    }
}

/// SEND_RESULT message - relay's response to SEND
#[derive(Debug, Clone)]
pub struct SendResult {
    pub status: u8,
    pub payload: Vec<u8>,  // encrypted response from recipient (empty on error)
}

impl SendResult {
    pub fn success(payload: Vec<u8>) -> Self {
        Self { status: SEND_OK, payload }
    }

    pub fn error(status: u8) -> Self {
        Self { status, payload: Vec::new() }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let payload_len = self.payload.len() as u32;
        let mut buf = Vec::with_capacity(1 + 4 + self.payload.len());
        buf.push(self.status);
        buf.extend_from_slice(&payload_len.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 5 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "SEND_RESULT too short"));
        }

        let status = data[0];
        let payload_len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;

        if data.len() < 5 + payload_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "SEND_RESULT payload truncated"));
        }

        let payload = data[5..5 + payload_len].to_vec();
        Ok(Self { status, payload })
    }
}

// ============================================================================
// Device Protocol Messages (inside encrypted payload)
// ============================================================================

/// HANDSHAKE_INIT: First message from invite acceptor to invite creator
#[derive(Debug, Clone)]
pub struct HandshakeInit {
    pub sender_id52: [u8; 32],
    pub preimage_for_peer: [u8; 32],
    pub relay_url: String,
}

impl HandshakeInit {
    pub fn to_bytes(&self) -> Vec<u8> {
        let url_bytes = self.relay_url.as_bytes();
        let url_len = url_bytes.len() as u16;
        let mut buf = Vec::with_capacity(1 + 32 + 32 + 2 + url_bytes.len());
        buf.push(DEV_HANDSHAKE_INIT);
        buf.extend_from_slice(&self.sender_id52);
        buf.extend_from_slice(&self.preimage_for_peer);
        buf.extend_from_slice(&url_len.to_be_bytes());
        buf.extend_from_slice(url_bytes);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 1 + 32 + 32 + 2 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "HANDSHAKE_INIT too short"));
        }
        if data[0] != DEV_HANDSHAKE_INIT {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not HANDSHAKE_INIT"));
        }

        let sender_id52: [u8; 32] = data[1..33].try_into().unwrap();
        let preimage_for_peer: [u8; 32] = data[33..65].try_into().unwrap();
        let url_len = u16::from_be_bytes([data[65], data[66]]) as usize;

        if data.len() < 67 + url_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "HANDSHAKE_INIT url truncated"));
        }

        let relay_url = String::from_utf8(data[67..67 + url_len].to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid utf8 in relay_url"))?;

        Ok(Self { sender_id52, preimage_for_peer, relay_url })
    }
}

/// HANDSHAKE_COMPLETE: Response from invite creator to acceptor
#[derive(Debug, Clone)]
pub struct HandshakeComplete {
    pub status: u8,
    pub preimage_for_peer: [u8; 32],
    pub relay_url: String,
}

impl HandshakeComplete {
    pub fn to_bytes(&self) -> Vec<u8> {
        let url_bytes = self.relay_url.as_bytes();
        let url_len = url_bytes.len() as u16;
        let mut buf = Vec::with_capacity(1 + 1 + 32 + 2 + url_bytes.len());
        buf.push(DEV_HANDSHAKE_COMPLETE);
        buf.push(self.status);
        buf.extend_from_slice(&self.preimage_for_peer);
        buf.extend_from_slice(&url_len.to_be_bytes());
        buf.extend_from_slice(url_bytes);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 1 + 1 + 32 + 2 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "HANDSHAKE_COMPLETE too short"));
        }
        if data[0] != DEV_HANDSHAKE_COMPLETE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not HANDSHAKE_COMPLETE"));
        }

        let status = data[1];
        let preimage_for_peer: [u8; 32] = data[2..34].try_into().unwrap();
        let url_len = u16::from_be_bytes([data[34], data[35]]) as usize;

        if data.len() < 36 + url_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "HANDSHAKE_COMPLETE url truncated"));
        }

        let relay_url = String::from_utf8(data[36..36 + url_len].to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid utf8 in relay_url"))?;

        Ok(Self { status, preimage_for_peer, relay_url })
    }
}

/// MESSAGE: Application message between paired peers
#[derive(Debug, Clone)]
pub struct DeviceMessage {
    pub content_type: u8,
    pub relay_url: String,
    pub content: Vec<u8>,
}

impl DeviceMessage {
    pub const CONTENT_TEXT: u8 = 0;
    pub const CONTENT_BINARY: u8 = 1;

    pub fn text(relay_url: String, text: &str) -> Self {
        Self {
            content_type: Self::CONTENT_TEXT,
            relay_url,
            content: text.as_bytes().to_vec(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let url_bytes = self.relay_url.as_bytes();
        let url_len = url_bytes.len() as u16;
        let content_len = self.content.len() as u32;
        let mut buf = Vec::with_capacity(1 + 1 + 2 + url_bytes.len() + 4 + self.content.len());
        buf.push(DEV_MESSAGE);
        buf.push(self.content_type);
        buf.extend_from_slice(&url_len.to_be_bytes());
        buf.extend_from_slice(url_bytes);
        buf.extend_from_slice(&content_len.to_be_bytes());
        buf.extend_from_slice(&self.content);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 1 + 1 + 2 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "MESSAGE too short"));
        }
        if data[0] != DEV_MESSAGE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not MESSAGE"));
        }

        let content_type = data[1];
        let url_len = u16::from_be_bytes([data[2], data[3]]) as usize;

        if data.len() < 4 + url_len + 4 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "MESSAGE url truncated"));
        }

        let relay_url = String::from_utf8(data[4..4 + url_len].to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid utf8 in relay_url"))?;

        let content_start = 4 + url_len;
        let content_len = u32::from_be_bytes([
            data[content_start], data[content_start + 1],
            data[content_start + 2], data[content_start + 3]
        ]) as usize;

        if data.len() < content_start + 4 + content_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "MESSAGE content truncated"));
        }

        let content = data[content_start + 4..content_start + 4 + content_len].to_vec();

        Ok(Self { content_type, relay_url, content })
    }
}

/// MESSAGE_RESPONSE: Response to a message, includes next preimage
#[derive(Debug, Clone)]
pub struct DeviceMessageResponse {
    pub status: u8,
    pub next_preimage: [u8; 32],
    pub relay_url: String,
    pub content: Vec<u8>,
}

impl DeviceMessageResponse {
    pub fn to_bytes(&self) -> Vec<u8> {
        let url_bytes = self.relay_url.as_bytes();
        let url_len = url_bytes.len() as u16;
        let content_len = self.content.len() as u32;
        let mut buf = Vec::with_capacity(1 + 1 + 32 + 2 + url_bytes.len() + 4 + self.content.len());
        buf.push(DEV_MESSAGE_RESPONSE);
        buf.push(self.status);
        buf.extend_from_slice(&self.next_preimage);
        buf.extend_from_slice(&url_len.to_be_bytes());
        buf.extend_from_slice(url_bytes);
        buf.extend_from_slice(&content_len.to_be_bytes());
        buf.extend_from_slice(&self.content);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 1 + 1 + 32 + 2 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "MESSAGE_RESPONSE too short"));
        }
        if data[0] != DEV_MESSAGE_RESPONSE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not MESSAGE_RESPONSE"));
        }

        let status = data[1];
        let next_preimage: [u8; 32] = data[2..34].try_into().unwrap();
        let url_len = u16::from_be_bytes([data[34], data[35]]) as usize;

        if data.len() < 36 + url_len + 4 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "MESSAGE_RESPONSE url truncated"));
        }

        let relay_url = String::from_utf8(data[36..36 + url_len].to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid utf8 in relay_url"))?;

        let content_start = 36 + url_len;
        let content_len = u32::from_be_bytes([
            data[content_start], data[content_start + 1],
            data[content_start + 2], data[content_start + 3]
        ]) as usize;

        if data.len() < content_start + 4 + content_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "MESSAGE_RESPONSE content truncated"));
        }

        let content = data[content_start + 4..content_start + 4 + content_len].to_vec();

        Ok(Self { status, next_preimage, relay_url, content })
    }
}

/// Parse device protocol message type from first byte
pub fn parse_device_msg_type(data: &[u8]) -> Option<u8> {
    data.first().copied()
}

// ============================================================================
// Relay Protocol Framing
// ============================================================================

/// Frame: wraps any message with type and length
#[derive(Debug, Clone)]
pub struct Frame {
    pub msg_type: u16,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(msg_type: u16, payload: Vec<u8>) -> Self {
        Self { msg_type, payload }
    }

    pub fn hello(hello: &Hello) -> Self {
        Self::new(MSG_HELLO, hello.to_bytes())
    }

    pub fn i_am(i_am: &IAm) -> Self {
        Self::new(MSG_I_AM, i_am.to_bytes())
    }

    pub fn send(send: &Send) -> Self {
        Self::new(MSG_SEND, send.to_bytes())
    }

    pub fn deliver(deliver: &Deliver) -> Self {
        Self::new(MSG_DELIVER, deliver.to_bytes())
    }

    pub fn ack(ack: &Ack) -> Self {
        Self::new(MSG_ACK, ack.to_bytes())
    }

    pub fn send_result(result: &SendResult) -> Self {
        Self::new(MSG_SEND_RESULT, result.to_bytes())
    }

    pub fn update_commits(update: &UpdateCommits) -> Self {
        Self::new(MSG_UPDATE_COMMITS, update.to_bytes())
    }

    /// Write frame to a writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let len = self.payload.len() as u32;
        writer.write_all(&self.msg_type.to_be_bytes())?;
        writer.write_all(&len.to_be_bytes())?;
        writer.write_all(&self.payload)?;
        Ok(())
    }

    /// Read frame from a reader
    pub fn read_from<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut header = [0u8; 6];
        reader.read_exact(&mut header)?;

        let msg_type = u16::from_be_bytes([header[0], header[1]]);
        let len = u32::from_be_bytes([header[2], header[3], header[4], header[5]]) as usize;

        // Sanity check
        if len > 1024 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
        }

        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload)?;

        Ok(Self { msg_type, payload })
    }
}

/// Async frame operations for tokio
#[cfg(feature = "async")]
pub mod async_io {
    use super::*;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, frame: &Frame) -> io::Result<()> {
        let len = frame.payload.len() as u32;
        writer.write_all(&frame.msg_type.to_be_bytes()).await?;
        writer.write_all(&len.to_be_bytes()).await?;
        writer.write_all(&frame.payload).await?;
        writer.flush().await?;
        Ok(())
    }

    pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Frame> {
        let mut header = [0u8; 6];
        reader.read_exact(&mut header).await?;

        let msg_type = u16::from_be_bytes([header[0], header[1]]);
        let len = u32::from_be_bytes([header[2], header[3], header[4], header[5]]) as usize;

        if len > 1024 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
        }

        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).await?;

        Ok(Frame { msg_type, payload })
    }
}
