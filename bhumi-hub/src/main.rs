#[tokio::main]
async fn main() {
    bhumi_hub::http::run_server().await
}
