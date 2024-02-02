use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use async_trait::async_trait;
use bytes::{Buf, Bytes, BytesMut};
use parking_lot::Mutex;
use tokio::io::{AsyncRead, AsyncWrite};

use super::error::ConnectionError;

pub trait SendStream: AsyncWrite + Send + Sync + Unpin {}
pub trait RecvStream: AsyncRead + Send + Sync + Unpin {}

#[async_trait]
pub trait QuicConn: Send + Sync + 'static {
    /// Connection id
    async fn id(&self) -> u64;

    /// Close the connection
    async fn close(&self);

    /// Check if the connection has closed
    async fn closed(&self) -> bool;

    /// Send datagram
    async fn send_datagram(&self, data: Bytes) -> Result<(), ConnectionError>;

    /// Read datagram
    async fn read_datagram(&self) -> Result<Bytes, ConnectionError>;
}

pub struct Connection<C> {
    inner: C,
    config: ConnectionConfig,
    state: ConnectionState,
}

#[derive(Default)]
struct ConnectionState {
    id_gen: IdGenerator,
    cache: Mutex<BTreeMap<u64, SegmentCache>>,
}

pub struct ConnectionConfig {
    max_segment_size: usize,
}

impl ConnectionConfig {
    pub fn new(max_segment_size: usize) -> Self {
        Self { max_segment_size }
    }
}

pub struct Datagram {
    id: u64,
    data: Bytes,
}

pub enum Segment {
    First(SegmentFirst),
    Rest(SegmentRest),
}

pub struct SegmentFirst {
    id: u64,
    seq: u16,
    total: u16,
    data: Bytes,
}

pub struct SegmentRest {
    id: u64,
    seq: u16,
    data: Bytes,
}

#[derive(Default)]
struct SegmentCache {
    segments: Vec<Segment>,
    total: u16,
}

#[derive(Default)]
struct IdGenerator {
    id: AtomicU64,
}

impl<C> Connection<C> {
    pub fn new(conn: C, config: ConnectionConfig) -> Self {
        Self {
            inner: conn,
            config,
            state: ConnectionState::default(),
        }
    }
}

impl<C> Connection<C>
where
    C: QuicConn,
{
    pub async fn send(&self, bytes: Bytes) -> Result<(), ConnectionError> {
        let id = self.state.id_gen.next();
        let data = Datagram::new(id, bytes);
        let segments = data.split(self.config.max_segment_size);
        for segment in segments {
            self.send_datagram(segment.encode()).await?;
        }

        Ok(())
    }

    pub async fn recv(&self) -> Result<Bytes, ConnectionError> {
        loop {
            let bytes = self.read_datagram().await?;
            let segment = Segment::decode(bytes)?;
            let mut cache_l = self.state.cache.lock();
            let entry = cache_l.entry(segment.id()).or_default();
            let id = segment.id();
            if let Some(data) = entry.push_segment(segment) {
                cache_l.remove(&id);
                return Ok(data.into_bytes());
            }
        }
    }
}

#[async_trait]
impl<C> QuicConn for Connection<C>
where
    C: QuicConn,
{
    /// Connection id
    async fn id(&self) -> u64 {
        self.inner.id().await
    }

    /// Close the connection
    async fn close(&self) {
        self.inner.close().await
    }

    /// Check if the connection has closed
    async fn closed(&self) -> bool {
        self.inner.closed().await
    }

    /// Send datagram
    async fn send_datagram(&self, data: Bytes) -> Result<(), ConnectionError> {
        self.inner.send_datagram(data).await
    }

    /// Read datagram
    async fn read_datagram(&self) -> Result<Bytes, ConnectionError> {
        self.inner.read_datagram().await
    }
}

impl Datagram {
    pub fn new(id: u64, data: Bytes) -> Self {
        Self { id, data }
    }

    fn split(mut self, segment_size: usize) -> Vec<Segment> {
        let mut segments = vec![];

        if self.data.is_empty() {
            return segments;
        }

        let num_segments = (self.data.len() + segment_size - 1) / segment_size;
        assert!(num_segments < u16::MAX as usize);

        let bytes_first = self.data.split_to(segment_size.min(self.data.len()));
        segments.push(Segment::new_first(
            self.id,
            num_segments as u16,
            bytes_first,
        ));

        let mut seq = 1;
        while !self.data.is_empty() {
            let segment = Segment::new_rest(
                self.id,
                seq,
                self.data.split_to(segment_size.min(self.data.len())),
            );
            segments.push(segment);
            seq += 1;
        }
        segments
    }

    fn join(id: u64, mut segments: Vec<Segment>) -> Datagram {
        segments.sort_unstable_by_key(|a| a.seq());
        let len = segments.iter().fold(0, |len, x| len + x.data().len());
        let bytes = segments
            .into_iter()
            .map(Segment::into_bytes)
            .fold(BytesMut::with_capacity(len), |mut data, b| {
                data.extend(b);
                data
            })
            .freeze();
        Datagram::new(id, bytes)
    }

    fn into_bytes(self) -> Bytes {
        self.data
    }
}

impl SegmentFirst {
    pub fn new(id: u64, seq: u16, total: u16, data_seg: Bytes) -> Self {
        Self {
            id,
            seq,
            total,
            data: data_seg,
        }
    }

    fn encode(self) -> Bytes {
        let mut bytes = BytesMut::with_capacity(8 + self.data.len());
        bytes.extend_from_slice(&self.id.to_be_bytes());
        bytes.extend_from_slice(&self.seq.to_be_bytes());
        bytes.extend_from_slice(&self.total.to_be_bytes());
        bytes.extend_from_slice(&self.data);
        bytes.freeze()
    }
}

impl SegmentRest {
    pub fn new(id: u64, seq: u16, data_seg: Bytes) -> Self {
        Self {
            id,
            seq,
            data: data_seg,
        }
    }

    fn encode(self) -> Bytes {
        let mut bytes = BytesMut::with_capacity(8 + self.data.len());
        bytes.extend_from_slice(&self.id.to_be_bytes());
        bytes.extend_from_slice(&self.seq.to_be_bytes());
        bytes.extend_from_slice(&self.data);
        bytes.freeze()
    }
}

impl Segment {
    fn new_first(id: u64, total: u16, data: Bytes) -> Self {
        Self::First(SegmentFirst {
            id,
            seq: 0,
            total,
            data,
        })
    }

    fn new_rest(id: u64, seq: u16, data: Bytes) -> Self {
        Self::Rest(SegmentRest { id, seq, data })
    }

    fn id(&self) -> u64 {
        match *self {
            Segment::First(ref s) => s.id,
            Segment::Rest(ref s) => s.id,
        }
    }

    fn seq(&self) -> u16 {
        match *self {
            Segment::First(ref s) => s.seq,
            Segment::Rest(ref s) => s.seq,
        }
    }

    fn data(&self) -> &Bytes {
        match *self {
            Segment::First(ref s) => &s.data,
            Segment::Rest(ref s) => &s.data,
        }
    }

    fn into_bytes(self) -> Bytes {
        match self {
            Segment::First(s) => s.data,
            Segment::Rest(s) => s.data,
        }
    }

    fn encode(self) -> Bytes {
        match self {
            Segment::First(s) => s.encode(),
            Segment::Rest(s) => s.encode(),
        }
    }

    fn decode(mut bytes: Bytes) -> Result<Self, ConnectionError> {
        if bytes.len() < 8 + 2 {
            return Err(ConnectionError::Encoding(
                "failed to decode a segment from bytes".to_owned(),
            ));
        }
        let id = bytes.get_u64_le();
        let seq = bytes.get_u16_le();
        if seq == 0 {
            if bytes.len() < 2 {
                return Err(ConnectionError::Encoding(
                    "failed to decode a segment from bytes".to_owned(),
                ));
            }
            let total = bytes.get_u16_le();
            Ok(Segment::First(SegmentFirst::new(id, seq, total, bytes)))
        } else {
            Ok(Segment::Rest(SegmentRest::new(id, seq, bytes)))
        }
    }
}

impl SegmentCache {
    fn push_segment(&mut self, segment: Segment) -> Option<Datagram> {
        if let Segment::First(ref fst) = segment {
            self.total = fst.total;
        }
        let id = segment.id();
        self.segments.push(segment);
        if self.total as usize == self.segments.len() {
            return Some(Datagram::join(id, self.segments.drain(..).collect()));
        }
        None
    }
}

impl IdGenerator {
    fn next(&self) -> u64 {
        self.id.fetch_add(1, Ordering::Relaxed)
    }
}
