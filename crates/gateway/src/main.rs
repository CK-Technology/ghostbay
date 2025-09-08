use anyhow::Result;
use clap::Parser;
use ghostbay_gateway::{GhostBayServer, ServerConfig};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    bind_address: String,

    #[arg(short, long, default_value_t = 3000)]
    port: u16,

    #[arg(long, default_value = "sqlite:./ghostbay.db")]
    database_url: String,

    #[arg(long, default_value = "./data")]
    data_dir: PathBuf,

    #[arg(long, default_value = "./tmp")]
    temp_dir: PathBuf,

    #[arg(long, default_value = "info")]
    log_level: String,

    #[arg(short, long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // color_eyre::install()?;

    let args = Args::parse();

    let config = if let Some(config_path) = args.config {
        let config_content = tokio::fs::read_to_string(&config_path).await?;
        toml::from_str(&config_content)?
    } else {
        ServerConfig {
            bind_address: args.bind_address,
            port: args.port,
            database_url: args.database_url,
            data_dir: args.data_dir,
            temp_dir: args.temp_dir,
            log_level: args.log_level,
        }
    };

    let server = GhostBayServer::new(config);
    server.run().await
}