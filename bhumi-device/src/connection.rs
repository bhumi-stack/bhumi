//! Connection to relay

use std::sync::Arc;
use rustls::pki_types::ServerName;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio::io::{AsyncRead, AsyncWrite};

use bhumi_proto::{Frame, Hello, IAm, Send as SendMsg, Deliver, Ack, SendResult, UpdateCommits, MSG_HELLO, MSG_DELIVER, MSG_SEND_RESULT};
use bhumi_proto::async_io::{read_frame, write_frame};
use fastn_id52::SecretKey;

/// Connect to a relay (insecure for dev - no cert verification)
pub async fn connect_insecure(
    addr: &str,
) -> std::io::Result<impl AsyncRead + AsyncWrite + Unpin> {
    // Skip certificate verification (DEV ONLY)
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertVerifier))
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));

    println!("Connecting to relay at {}...", addr);
    let stream = TcpStream::connect(addr).await?;
    println!("TCP connected");

    let server_name = ServerName::try_from("localhost")
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let tls_stream = connector.connect(server_name, stream).await?;
    println!("TLS handshake complete");

    Ok(tls_stream)
}

/// Perform handshake: receive HELLO, send I_AM with commits
pub async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
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
    println!(
        "Received HELLO: version={}, nonce=0x{:08x}, max_payload={}",
        hello.version, hello.nonce, hello.max_payload_size
    );

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
    println!("Sent I_AM with {} commits", i_am.commits.len());

    Ok(())
}

/// Send a message to another device and wait for response
pub async fn send_message<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    to_id52: [u8; 32],
    preimage: [u8; 32],
    payload: Vec<u8>,
) -> std::io::Result<SendResult> {
    let send = SendMsg {
        to_id52,
        preimage,
        payload,
    };

    write_frame(stream, &Frame::send(&send)).await?;
    println!("Sent message ({} bytes), waiting for result...", send.payload.len());

    // Wait for SEND_RESULT
    let frame = read_frame(stream).await?;
    if frame.msg_type != MSG_SEND_RESULT {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("expected SEND_RESULT, got 0x{:04x}", frame.msg_type),
        ));
    }

    let result = SendResult::from_bytes(&frame.payload)?;
    let status_str = match result.status {
        0 => "success",
        1 => "not connected",
        2 => "invalid preimage",
        3 => "timeout",
        4 => "disconnected",
        _ => "unknown",
    };
    println!("SEND_RESULT: {} ({} bytes response)", status_str, result.payload.len());

    Ok(result)
}

/// Send an ACK response to a delivered message
pub async fn send_ack<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    msg_id: u32,
    payload: Vec<u8>,
) -> std::io::Result<()> {
    let ack = Ack { msg_id, payload };
    write_frame(stream, &Frame::ack(&ack)).await?;
    println!("Sent ACK for msg_id={}", msg_id);
    Ok(())
}

/// Send UPDATE_COMMITS to add new commits while connected
pub async fn update_commits<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    commits: Vec<[u8; 32]>,
) -> std::io::Result<()> {
    let update = UpdateCommits { commits: commits.clone() };
    write_frame(stream, &Frame::update_commits(&update)).await?;
    println!("Sent UPDATE_COMMITS ({} commits)", commits.len());
    Ok(())
}

/// Wait for and receive a delivered message
pub async fn receive_message<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
) -> std::io::Result<Deliver> {
    let frame = read_frame(stream).await?;

    if frame.msg_type != MSG_DELIVER {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("expected DELIVER, got 0x{:04x}", frame.msg_type),
        ));
    }

    Deliver::from_bytes(&frame.payload)
}

// Skip certificate verification for self-signed certs (DEV ONLY)
#[derive(Debug)]
struct NoCertVerifier;

impl rustls::client::danger::ServerCertVerifier for NoCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}
