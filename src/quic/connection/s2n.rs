use std::task::Poll;

use async_trait::async_trait;
use bytes::Bytes;
use s2n_quic::{
    provider::datagram::default::{Receiver, Sender},
    Connection as S2nConnection,
};

use super::{BiStream, Connection, ConnectionError, RecvStream, SendStream};

impl SendStream for s2n_quic::stream::SendStream {}
impl RecvStream for s2n_quic::stream::ReceiveStream {}

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
                .map_err(|e| ConnectionError::SendDatagram(e.to_string()))
        })
        .map_err(|e| ConnectionError::SendDatagram(e.to_string()))?
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

    async fn open_bi_stream(&mut self) -> Result<BiStream, ConnectionError> {
        let (rx, tx) = self
            .open_bidirectional_stream()
            .await
            .map_err(|e| ConnectionError::Stream(e.to_string()))?
            .split();

        Ok((Box::new(tx), Box::new(rx)))
    }

    async fn accept_bi_stream(&mut self) -> Result<BiStream, ConnectionError> {
        let (rx, tx) = self
            .accept_bidirectional_stream()
            .await
            .map_err(|e| ConnectionError::Stream(e.to_string()))?
            .ok_or(ConnectionError::Stream("stream finished".to_owned()))?
            .split();

        Ok((Box::new(tx), Box::new(rx)))
    }
}
