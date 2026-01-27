mod router;
mod server;
mod session;

use server::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::bind("0.0.0.0:8443").await?;
    server.run().await
}
