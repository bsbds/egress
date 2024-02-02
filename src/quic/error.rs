use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("encoding error: {0}")]
    Encoding(String),
    #[error("send datagram error: {0}")]
    SendDatagram(String),
    #[error("read datagram error: {0}")]
    ReadDatagram(String),
    #[error("stream error: {0}")]
    Stream(String),
    #[error("io error: {0}")]
    IO(#[from] io::Error),
}
