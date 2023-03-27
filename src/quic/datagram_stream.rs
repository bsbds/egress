use super::{ConnectedState, DatagramStreamError, QuicConnection, StreamId};
use crate::{common::constant::FLUME_CHANNEL_SIZE, config::NetworkType};
use bytes::Bytes;
use flume::Receiver;
use log::info;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::sync::{Notify, RwLock};

#[derive(Clone)]
pub struct DatagramStream<C: Sync + Send + 'static + Clone + ConnectedState>(
    pub Arc<DatagramStreamInner<C>>,
);

impl<C> DatagramStream<C>
where
    C: Sync + Send + 'static + Clone + ConnectedState,
{
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    pub fn send(&self, bytes: Bytes) {
        self.parent_connection.stream_send(self.stream_id, bytes);
        self.packet_received.notify_waiters();
    }

    pub async fn recv(&self) -> Result<Bytes, DatagramStreamError> {
        match self.receiver.recv_async().await {
            Err(e) => Err(DatagramStreamError::FlumeRecvError(e)),
            Ok(res) => {
                self.packet_received.notify_waiters();
                Ok(res)
            }
        }
    }

    pub fn stream_type(&self) -> NetworkType {
        self.stream_type
    }

    pub fn id(&self) -> StreamId {
        self.stream_id
    }

    pub async fn closed(&self) -> DatagramStreamError {
        self.close_notify.notified().await;
        self.close_reason.read().await.unwrap()
    }

    pub(super) fn wait_close(&self, conn_closed: Arc<Notify>) {
        let this = self.clone();
        tokio::spawn(async move {
            let DatagramStreamInner {
                stream_id,
                packet_received,
                close_reason,
                close_notify,
                idle_timeout,
                parent_close,
                ..
            } = this.0.as_ref();
            loop {
                let idle_timeout = tokio::time::sleep(Duration::from_secs(*idle_timeout));
                tokio::select! {
                    _ = idle_timeout => {
                        let mut close_reason = close_reason.write().await;
                        close_reason.replace(DatagramStreamError::IdleTimeout);
                        break;
                    }
                    _ = conn_closed.notified() => {
                        let mut close_reason = close_reason.write().await;
                        close_reason.replace(DatagramStreamError::ConnectionError);
                        break;
                    }
                    _ = parent_close.notified() => {
                        let mut close_reason = close_reason.write().await;
                        close_reason.replace(DatagramStreamError::ManualClosed);
                        break;
                    }
                    _ = packet_received.notified() => {}
                }
            }
            close_notify.notify_waiters();
            info!("stream closed, id: {}", stream_id);
        });
    }
}

impl<C> std::ops::Deref for DatagramStream<C>
where
    C: Sync + Send + 'static + Clone + ConnectedState,
{
    type Target = DatagramStreamInner<C>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<C> Drop for DatagramStream<C>
where
    C: Sync + Send + 'static + Clone + ConnectedState,
{
    fn drop(&mut self) {
        self.parent_connection.deregister(self.stream_id);
        self.parent_connection.close_stream(self.id());
    }
}

pub struct DatagramStreamInner<C>
where
    C: Sync + Send + 'static + Clone + ConnectedState,
{
    parent_connection: QuicConnection<C>,
    stream_id: StreamId,
    receiver: Receiver<Bytes>,
    stream_type: NetworkType,
    peer_addr: SocketAddr,
    packet_received: Notify,
    close_reason: RwLock<Option<DatagramStreamError>>,
    close_notify: Notify,
    idle_timeout: u64,
    parent_close: Arc<Notify>,
}

impl<C> DatagramStreamInner<C>
where
    C: Sync + Send + 'static + Clone + ConnectedState,
{
    pub(super) fn new(
        parent_connection: QuicConnection<C>,
        stream_id: StreamId,
        stream_type: NetworkType,
        peer_addr: SocketAddr,
        idle_timeout: u64,
        parent_close: Arc<Notify>,
    ) -> Self {
        let (sender, receiver) = flume::bounded(FLUME_CHANNEL_SIZE);
        parent_connection.register(stream_id, sender);
        Self {
            parent_connection,
            stream_id,
            receiver,
            stream_type,
            peer_addr,
            packet_received: Notify::new(),
            close_reason: RwLock::new(None),
            close_notify: Notify::new(),
            idle_timeout,
            parent_close,
        }
    }
}
