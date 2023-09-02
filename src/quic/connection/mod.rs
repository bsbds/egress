#[cfg(feature = "quinn")]
pub(crate) mod quinn;
#[cfg(feature = "s2n-quic")]
pub(crate) mod s2n;

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};

type BiStream = (Box<dyn SendStream>, Box<dyn RecvStream>);

#[async_trait]
pub trait Connection: Send + Sync {
    /// Connection id
    fn id(&self) -> u64;

    /// Close the connection
    fn close(&self);

    /// Check if the connection has closed
    fn closed(&self) -> bool;

    /// Send datagram
    fn send_datagram(&self, data: Bytes) -> Result<(), ConnectionError>;

    /// Read datagram
    async fn read_datagram(&self) -> Result<Bytes, ConnectionError>;

    /// Open a bi-directional stream
    async fn open_bi_stream(&mut self) -> Result<BiStream, ConnectionError>;

    /// Accept a bi-directional stream
    async fn accept_bi_stream(&mut self) -> Result<BiStream, ConnectionError>;
}

pub trait SendStream: AsyncWrite + Send + Sync + Unpin {}
pub trait RecvStream: AsyncRead + Send + Sync + Unpin {}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("send datagram error: {0}")]
    SendDatagram(String),
    #[error("read datagram error: {0}")]
    ReadDatagram(String),
    #[error("stream error: {0}")]
    Stream(String),
}
