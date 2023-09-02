use async_trait::async_trait;
use bytes::Bytes;
use quinn::Connection as QuinnConnection;

use super::{BiStream, Connection, ConnectionError, RecvStream, SendStream};

impl SendStream for quinn::SendStream {}
impl RecvStream for quinn::RecvStream {}

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

    async fn open_bi_stream(&mut self) -> Result<BiStream, ConnectionError> {
        let (tx, rx) = self
            .open_bi()
            .await
            .map_err(|e| ConnectionError::Stream(e.to_string()))?;

        Ok((Box::new(tx), Box::new(rx)))
    }

    async fn accept_bi_stream(&mut self) -> Result<BiStream, ConnectionError> {
        let (tx, rx) = self
            .accept_bi()
            .await
            .map_err(|e| ConnectionError::Stream(e.to_string()))?;

        Ok((Box::new(tx), Box::new(rx)))
    }
}
