use std::sync::{
    atomic::{AtomicU16, Ordering},
    Arc,
};

use bytes::{Bytes, BytesMut};
use dashmap::DashMap;

use super::{
    connection::{Connection, QuicConn},
    error::ConnectionError,
};

const STREAM_CHANNEL_SIZE: usize = 1024;

pub struct Dispatcher<T, C> {
    pub conn: Arc<Connection<C>>,
    pub streams: Arc<DashMap<u16, flume::Sender<Bytes>>>,
    pub current_id: AtomicU16,
    pub inner: T,
}

pub struct Stream<C> {
    id: u16,
    conn: Arc<Connection<C>>,
    rx: flume::Receiver<Bytes>,
    streams_ref: Arc<DashMap<u16, flume::Sender<Bytes>>>,
}

impl<T, C> Dispatcher<T, C>
where
    C: QuicConn,
{
    pub fn new(conn: Arc<Connection<C>>, inner: T) -> Self {
        Self {
            conn,
            streams: Arc::new(DashMap::new()),
            current_id: AtomicU16::new(0),
            inner,
        }
    }

    pub fn open_stream(&self) -> Stream<C> {
        let id = self.next_id();
        let (tx, rx) = flume::bounded(STREAM_CHANNEL_SIZE);
        self.streams.insert(id, tx);
        Stream {
            id,
            rx,
            conn: Arc::clone(&self.conn),
            streams_ref: Arc::clone(&self.streams),
        }
    }

    pub fn next_id(&self) -> u16 {
        self.current_id.fetch_add(1, Ordering::Relaxed)
    }
}

impl<C> Stream<C>
where
    C: QuicConn,
{
    pub async fn send(&self, bytes: Bytes) -> Result<(), ConnectionError> {
        let mut data = BytesMut::with_capacity(2 + bytes.len());
        data.extend(self.id.to_be_bytes());
        data.extend(bytes);
        self.conn.send(data.freeze()).await
    }

    pub async fn recv(&self) -> Result<Bytes, ConnectionError> {
        self.rx
            .recv_async()
            .await
            .map_err(|e| ConnectionError::Stream(e.to_string()))
    }
}

impl<C> Drop for Stream<C> {
    fn drop(&mut self) {
        let _ignore = self.streams_ref.remove(&self.id);
    }
}
