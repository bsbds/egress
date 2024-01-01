use std::fs;

use clap::Parser;

mod config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = config::Args::parse();
    let _config: config::Config = toml::from_str(
        &fs::read_to_string(args.config_path).expect("failed to read configuration file"),
    )
    .expect("failed to parse configuration file");
    Ok(())
}
