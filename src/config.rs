use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    pub config_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum Config {
    #[serde(alias = "client")]
    Client(Client),
    #[serde(alias = "server")]
    Server(Server),
}

/// Client specific configuration
#[derive(Serialize, Deserialize)]
pub struct Client {
    #[serde(flatten)]
    pub common: Common,
    pub server_name: String,
    pub server_addr: String,
    pub listen: SocketAddr,
    pub peer: SocketAddr,
    #[serde(default = "default_cert_ver")]
    pub cert_ver: bool,
    pub cert_path: Option<String>,
}
/// Server specific configuration
#[derive(Serialize, Deserialize)]
pub struct Server {
    #[serde(flatten)]
    pub common: Common,
    pub listen: Vec<SocketAddr>,
    pub certificate: Option<String>,
    pub private_key: Option<String>,
    #[serde(default = "default_self_sign")]
    pub self_sign: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Common {
    pub psk: String,
    #[serde(default)]
    pub congestion: Congestion,
    #[serde(default = "default_conn_idle_timeout")]
    pub connection_idle_timeout: u64,
    #[serde(default = "default_stream_idle_timeout")]
    pub stream_idle_timeout: u64,
    #[serde(default = "default_enable_0rtt")]
    pub enable_0rtt: Option<bool>,
    #[serde(default = "default_loglevel")]
    pub loglevel: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename = "lowercase")]
pub enum Congestion {
    #[serde(alias = "cubic")]
    Cubic,
    #[serde(alias = "bbr")]
    Bbr,
}

impl Default for Congestion {
    fn default() -> Self {
        Self::Cubic
    }
}

fn default_cert_ver() -> bool {
    true
}

fn default_self_sign() -> bool {
    false
}

fn default_conn_idle_timeout() -> u64 {
    600
}

fn default_stream_idle_timeout() -> u64 {
    30
}

fn default_enable_0rtt() -> Option<bool> {
    Some(false)
}

fn default_loglevel() -> String {
    "warn".to_string()
}
