//! TCP server setup and connection handling (TLS disabled for dev)

use std::sync::Arc;
use tokio::net::TcpListener;

use crate::router::Router;
use crate::session::Session;

pub struct Server {
    listener: TcpListener,
    router: Arc<Router>,
}

impl Server {
    pub async fn bind(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(addr).await?;
        let router = Router::new();

        println!("Relay listening on {} (TLS disabled)", addr);

        Ok(Self {
            listener,
            router,
        })
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let (stream, addr) = self.listener.accept().await?;
            let router = self.router.clone();

            tokio::spawn(async move {
                println!("Connection from {}", addr);

                // Generate random nonce
                let nonce: u32 = rand::random();

                let session = Session::new(stream, router, nonce);
                if let Err(e) = session.run().await {
                    if e.kind() != std::io::ErrorKind::UnexpectedEof {
                        eprintln!("Session error with {}: {}", addr, e);
                    }
                }
                println!("Connection closed: {}", addr);
            });
        }
    }
}
