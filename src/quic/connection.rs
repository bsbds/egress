use std::task::Poll;

use async_trait::async_trait;
use bytes::Bytes;
use quinn::Connection as QuinnConnection;
use s2n_quic::{
    provider::datagram::default::{Receiver, Sender},
    Connection as S2nConnection,
};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};

#[async_trait]
pub(crate) trait Connection: Send + Sync {
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
    async fn open_bi_stream(
        &mut self,
    ) -> Result<(Box<dyn AsyncWrite>, Box<dyn AsyncRead>), ConnectionError>;

    /// Accept a bi-directional stream
    async fn accept_bi_stream(
        &mut self,
    ) -> Result<(Box<dyn AsyncWrite>, Box<dyn AsyncRead>), ConnectionError>;
}

#[async_trait]
impl Connection for QuinnConnection {
    fn id(&self) -> u64 {
        self.stable_id() as u64
    }

    fn close(&self) {
        self.close(0u32.into(), &[]);
    }

    fn closed(&self) -> bool {
        self.close_reason().is_some()
    }

    fn send_datagram(&self, data: Bytes) -> Result<(), ConnectionError> {
        self.send_datagram(data)
            .map_err(|e| ConnectionError::SendDatagram(e.to_string()))
    }

    async fn read_datagram(&self) -> Result<Bytes, ConnectionError> {
        self.read_datagram()
            .await
            .map_err(|e| ConnectionError::ReadDatagram(e.to_string()))
    }

    async fn open_bi_stream(
        &mut self,
    ) -> Result<(Box<dyn AsyncWrite>, Box<dyn AsyncRead>), ConnectionError> {
        let (tx, rx) = self
            .open_bi()
            .await
            .map_err(|e| ConnectionError::Stream(e.to_string()))?;

        Ok((Box::new(tx), Box::new(rx)))
    }

    async fn accept_bi_stream(
        &mut self,
    ) -> Result<(Box<dyn AsyncWrite>, Box<dyn AsyncRead>), ConnectionError> {
        let (tx, rx) = self
            .accept_bi()
            .await
            .map_err(|e| ConnectionError::Stream(e.to_string()))?;

        Ok((Box::new(tx), Box::new(rx)))
    }
}

#[async_trait]
impl Connection for S2nConnection {
    fn id(&self) -> u64 {
        self.id()
    }

    fn close(&self) {
        self.close(0u32.into());
    }

    fn closed(&self) -> bool {
        self.remote_addr().is_err()
    }

    fn send_datagram(&self, data: Bytes) -> Result<(), ConnectionError> {
        self.datagram_mut(|sender: &mut Sender| {
            sender
                .send_datagram(data)
                .map_err(|e| ConnectionError::ReadDatagram(e.to_string()))
        })
        .map_err(|e| ConnectionError::ReadDatagram(e.to_string()))?
    }

    async fn read_datagram(&self) -> Result<Bytes, ConnectionError> {
        futures::future::poll_fn(|cx| {
            match self.datagram_mut(|recv: &mut Receiver| recv.poll_recv_datagram(cx)) {
                Ok(value) => value.map_err(|e| ConnectionError::ReadDatagram(e.to_string())),
                Err(e) => Poll::Ready(Err(ConnectionError::ReadDatagram(e.to_string()))),
            }
        })
        .await
    }

    async fn open_bi_stream(
        &mut self,
    ) -> Result<(Box<dyn AsyncWrite>, Box<dyn AsyncRead>), ConnectionError> {
        let (rx, tx) = self
            .open_bidirectional_stream()
            .await
            .map_err(|e| ConnectionError::Stream(e.to_string()))?
            .split();

        Ok((Box::new(tx), Box::new(rx)))
    }

    async fn accept_bi_stream(
        &mut self,
    ) -> Result<(Box<dyn AsyncWrite>, Box<dyn AsyncRead>), ConnectionError> {
        let (rx, tx) = self
            .accept_bidirectional_stream()
            .await
            .map_err(|e| ConnectionError::Stream(e.to_string()))?
            .ok_or(ConnectionError::Stream("stream finished".to_owned()))?
            .split();

        Ok((Box::new(tx), Box::new(rx)))
    }
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("send datagram error: {0}")]
    SendDatagram(String),
    #[error("read datagram error: {0}")]
    ReadDatagram(String),
    #[error("stream error: {0}")]
    Stream(String),
}
