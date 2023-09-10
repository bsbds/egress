use std::task::Poll;

use async_trait::async_trait;
use bytes::Bytes;
use s2n_quic::{
    provider::{
        datagram::default::{Receiver, Sender},
        event::{events, Subscriber},
    },
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
        let context = self.query_event_context(|event_context: &EventContext| *event_context);
        context.map(|c| c.closed).unwrap_or(true)
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

pub(crate) struct ConnectionSubscriber {}

impl Subscriber for ConnectionSubscriber {
    type ConnectionContext = EventContext;
    fn create_connection_context(
        &mut self,
        _meta: &events::ConnectionMeta,
        _info: &events::ConnectionInfo,
    ) -> Self::ConnectionContext {
        EventContext { closed: false }
    }

    fn on_connection_closed(
        &mut self,
        context: &mut Self::ConnectionContext,
        _meta: &s2n_quic::provider::event::ConnectionMeta,
        _event: &events::ConnectionClosed,
    ) {
        context.closed = true;
    }
}

#[derive(Clone, Copy)]
pub struct EventContext {
    closed: bool,
}
