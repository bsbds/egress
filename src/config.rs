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
    pub listen_addr: SocketAddr,
}

/// Server specific configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct Server {
    #[serde(flatten)]
    pub common: Common,
    pub listen: SocketAddr,
    pub peer_addr: SocketAddr,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Common {
    pub network: Network,
    pub ca_cert_path: String,
    pub cert_path: String,
    pub private_key_path: String,
    #[serde(default)]
    pub congestion: Congestion,
    #[serde(default = "default_conn_idle_timeout")]
    pub connection_idle_timeout: u64,
    #[serde(default = "default_max_segment_size")]
    pub max_segment_size: usize,
    #[serde(default = "default_loglevel")]
    pub loglevel: String,
}

#[derive(Copy, Clone, Serialize, Deserialize)]
#[serde(rename = "lowercase")]
pub enum Congestion {
    #[serde(alias = "cubic")]
    Cubic,
    #[serde(alias = "bbr")]
    Bbr,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename = "lowercase")]
pub enum Network {
    #[serde(alias = "udp")]
    Udp,
    #[serde(alias = "tcp")]
    Tcp,
}

impl Default for Congestion {
    fn default() -> Self {
        Self::Cubic
    }
}

fn default_conn_idle_timeout() -> u64 {
    600
}

fn default_max_segment_size() -> usize {
    1280
}

fn default_loglevel() -> String {
    "warn".to_string()
}
