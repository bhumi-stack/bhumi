#[derive(clap::Parser)]
#[command(name = "bhumi-hub")]
#[command(about = "Bhumi Hub server")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Create a new key
    CreateKey,
    /// Run the server
    Run,
}

#[tokio::main]
async fn main() {
    let cli: Cli = clap::Parser::parse();

    match cli.command {
        Commands::CreateKey => bhumi_hub::crate_key(),
        Commands::Run => {
            let key = match bhumi_hub::read_key() {
                Ok(key) => key,
                Err(e) => {
                    eprintln!("Failed to read key: {e}");
                    std::process::exit(1);
                }
            };
            bhumi_hub::http::run_server(key).await.unwrap();
        }
    }
}
