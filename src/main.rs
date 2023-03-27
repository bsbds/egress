mod client;
mod common;
mod config;
mod quic;
mod server;

use clap::Parser;
use std::fs;

async fn run(config: config::Config) {
    match config.mode {
        config::Mode::Client => {
            client::client(config).await;
        }
        config::Mode::Server => {
            server::server(config).await;
        }
    }
}

#[tokio::main]
async fn main() {
    let args = config::Args::parse();
    let config: config::Config = toml::from_str(
        &fs::read_to_string(args.config).expect("failed to read configuration file"),
    )
    .expect("failed to parse configuration file");
    env_logger::builder()
        .filter_level(config.loglevel.parse().expect("failed to parse loglevel"))
        .format_module_path(false)
        .format_target(false)
        .init();

    run(config).await;
}
