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
    let bhumi_home = std::env::var("BHUMI_HOME").unwrap_or_else(|_| {
        dirs::home_dir()
            .expect("Could not determine home directory")
            .join(".bhumi")
            .to_string_lossy()
            .to_string()
    });
    std::fs::create_dir_all(&bhumi_home).unwrap();

    match cli.command {
        Commands::CreateKey => bhumi_hub::create_key(&bhumi_home),
        Commands::Run => {
            let key = match bhumi_hub::read_key(&bhumi_home) {
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
