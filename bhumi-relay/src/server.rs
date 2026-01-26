//! TLS server setup and connection handling

use std::sync::Arc;
use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::router::Router;
use crate::session::Session;

pub struct Server {
    acceptor: TlsAcceptor,
    listener: TcpListener,
    router: Arc<Router>,
}

impl Server {
    pub async fn bind(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Generate self-signed certificate
        let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names)?;

        let cert_der = CertificateDer::from(cert.der().to_vec());
        let key_der = PrivateKeyDer::try_from(key_pair.serialize_der()).unwrap();

        // Print cert for debugging
        println!("=== Self-signed certificate ===");
        println!("{}", cert.pem());
        println!("===============================\n");

        // Build TLS config
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)?;

        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind(addr).await?;
        let router = Router::new();

        println!("Relay listening on {}", addr);

        Ok(Self {
            acceptor,
            listener,
            router,
        })
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let (stream, addr) = self.listener.accept().await?;
            let acceptor = self.acceptor.clone();
            let router = self.router.clone();

            tokio::spawn(async move {
                println!("Connection from {}", addr);

                match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        println!("TLS handshake complete with {}", addr);

                        // Generate random nonce
                        let nonce: u32 = rand::random();

                        let session = Session::new(tls_stream, router, nonce);
                        if let Err(e) = session.run().await {
                            if e.kind() != std::io::ErrorKind::UnexpectedEof {
                                eprintln!("Session error with {}: {}", addr, e);
                            }
                        }
                        println!("Connection closed: {}", addr);
                    }
                    Err(e) => {
                        eprintln!("TLS handshake failed with {}: {}", addr, e);
                    }
                }
            });
        }
    }
}
