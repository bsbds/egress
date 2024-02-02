mod client;
mod config;
mod quic;
mod server;
mod utils;

use std::fs;

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = config::Args::parse();
    let config: config::Config = toml::from_str(&fs::read_to_string(args.config_path)?)?;

    match config {
        config::Config::Client(c) => {
            client::run(c).await;
        }
        config::Config::Server(c) => {
            server::run(c).await;
        }
    }

    Ok(())
}
