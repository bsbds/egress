use clap::Parser;
use serde::de::{self, Deserialize, Deserializer, Unexpected, Visitor};
use serde_derive::Deserialize;
use std::{fmt, net::SocketAddr};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    pub config: std::path::PathBuf,
}

#[derive(Deserialize)]
pub struct Config {
    pub mode: Mode,
    pub psk: String,
    pub client: Option<ClientConfig>,
    pub server: Option<ServerConfig>,
    #[serde(default = "default_congestion")]
    pub congestion: Congestion,
    #[serde(default = "default_max_udp_payload")]
    pub max_udp_payload: u16,
    #[serde(default = "default_conn_idle_timeout")]
    pub connection_idle_timeout: u64,
    #[serde(default = "default_stream_idle_timeout")]
    pub stream_idle_timeout: u64,
    #[serde(default = "default_enable_0rtt")]
    pub enable_0rtt: bool,
    #[serde(default = "default_loglevel")]
    pub loglevel: String,
}
fn default_congestion() -> Congestion {
    Congestion::Bbr
}
fn default_conn_idle_timeout() -> u64 {
    60
}
fn default_stream_idle_timeout() -> u64 {
    30
}
fn default_enable_0rtt() -> bool {
    false
}
fn default_loglevel() -> String {
    "warn".to_string()
}
fn default_max_udp_payload() -> u16 {
    1452
}

/// Client specific configuration
#[derive(Deserialize)]
pub struct ClientConfig {
    pub server_name: String,
    pub server_addr: String,
    pub listen: SocketAddr,
    pub peer: SocketAddr,
    pub network: NetworkType,
    #[serde(default = "default_cert_ver")]
    pub cert_ver: bool,
}
fn default_cert_ver() -> bool {
    true
}

/// Server specific configuration
#[derive(Deserialize)]
pub struct ServerConfig {
    pub quic_listen: SocketAddr,
    pub certificate: Option<String>,
    pub private_key: Option<String>,
    #[serde(default = "default_self_sign")]
    pub self_sign: bool,
}
fn default_self_sign() -> bool {
    false
}

macro_rules! deserialize_string_enum {
    ($name:ident, $($variant:ident => $str:expr),+) => {
        #[derive(Copy, Clone, Debug)]
        pub enum $name {
            $($variant),+
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<$name, D::Error> {
                struct StringVisitor;

                impl<'de> Visitor<'de> for StringVisitor {
                    type Value = $name;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        let valid_values = vec![$($str),+].join(", ");
                        write!(formatter, "only the following strings are valid: {}", valid_values)
                    }

                    fn visit_str<E: de::Error>(self, value: &str) -> Result<$name, E> {
                        match value {
                            $($str => Ok($name::$variant),)+
                            _ => Err(de::Error::invalid_value(Unexpected::Str(value), &self)),
                        }
                    }
                }

                deserializer.deserialize_str(StringVisitor)
            }
        }
    }
}

deserialize_string_enum! {
    Mode,
    Client => "client",
    Server => "server"
}

deserialize_string_enum! {
    Congestion,
    Cubic => "cubic",
    NewReno => "new_reno",
    Bbr => "bbr"
}

deserialize_string_enum! {
    NetworkType,
    Udp => "udp",
    Tcp => "tcp"
}
