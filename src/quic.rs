mod command;
mod datagram_stream;

use crate::{common::constant::FLUME_CHANNEL_SIZE, config::NetworkType};
use bytes::{Bytes, BytesMut};
use crossbeam_queue::SegQueue;
use dashmap::DashMap;
pub use datagram_stream::DatagramStream;
use flume::{Receiver, Sender};
use log::{error, info};
use quinn::{Connection, RecvStream, SendStream};
use std::{
    collections::HashMap,
    fmt::Display,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
};
use thiserror::Error;
use tokio::sync::{Mutex, Notify};
use {command::*, datagram_stream::*};

pub trait ConnectedState {}

#[derive(Clone)]
pub struct ClientState {
    id_gen: Arc<StreamIdGenerator>,
}

#[derive(Clone)]
pub struct ServerState {
    stream_queue: Arc<SegQueue<DatagramStream<ServerState>>>,
    new_stream: Arc<Notify>,
}

impl ConnectedState for ClientState {}
impl ConnectedState for ServerState {}

#[derive(Clone)]
pub struct QuicConnection<S>
where
    S: Clone,
{
    inner: Arc<QuicConnectionInner>,
    state: S,
}

pub struct QuicConnectionInner {
    connection: Connection,
    command_send_stream: Mutex<Option<SendStream>>,
    command_recv_stream: Mutex<Option<RecvStream>>,
    psk: Arc<Vec<u8>>,
    chan_mpsc: (Sender<StreamData>, Receiver<StreamData>),
    sender_map: Arc<DashMap<StreamId, Sender<Bytes>>>,
    close_signal_map: DashMap<StreamId, Arc<Notify>>,
    closed: Arc<Notify>,
    stream_idle_timeout: u64,
}

impl<S> std::ops::Deref for QuicConnection<S>
where
    S: Clone,
{
    type Target = Arc<QuicConnectionInner>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<S> QuicConnection<S>
where
    S: Clone,
{
    pub fn connetion_closed(&self) -> bool {
        self.connection.close_reason().is_some()
    }
}

impl QuicConnection<()> {
    pub async fn new(connection: Connection, psk: Arc<Vec<u8>>, stream_idle_timeout: u64) -> Self {
        QuicConnection {
            inner: Arc::new(QuicConnectionInner {
                connection,
                command_send_stream: Mutex::new(None),
                command_recv_stream: Mutex::new(None),
                psk,
                chan_mpsc: flume::bounded(FLUME_CHANNEL_SIZE),
                sender_map: Arc::new(DashMap::new()),
                close_signal_map: DashMap::new(),
                closed: Arc::new(Notify::new()),
                stream_idle_timeout,
            }),
            state: (),
        }
    }

    pub async fn into_client(self) -> Result<QuicConnection<ClientState>, CommandError> {
        let (mut send_stream, recv_stream) = self.connection.open_bi().await?;

        let command = ConnectionCommand::Auth {
            psk: self.psk.clone(),
        };
        command.write_to_stream(&mut send_stream).await?;
        self.command_send_stream.lock().await.replace(send_stream);
        self.command_recv_stream.lock().await.replace(recv_stream);
        let connection = QuicConnection {
            inner: self.inner.clone(),
            state: ClientState {
                id_gen: Arc::new(StreamIdGenerator::default()),
            },
        };
        connection.start_transport();
        Ok(connection)
    }

    pub async fn into_server(self) -> Result<QuicConnection<ServerState>, CommandError> {
        let (send_stream, mut recv_stream) = self.connection.accept_bi().await?;

        let command = read_command(&mut recv_stream).await?;

        self.command_send_stream.lock().await.replace(send_stream);
        self.command_recv_stream.lock().await.replace(recv_stream);

        if let ConnectionCommand::Auth { psk } = command {
            if psk.iter().zip(self.psk.iter()).all(|(x, y)| x == y) {
            } else {
                self.connection.close(0u32.into(), &[]);
                return Err(CommandError::ConnectionError(QuinnError::Connection(
                    quinn::ConnectionError::LocallyClosed,
                )));
            }
        }
        let connection = QuicConnection {
            inner: self.inner.clone(),
            state: ServerState {
                stream_queue: Arc::new(SegQueue::new()),
                new_stream: Arc::new(Notify::new()),
            },
        };
        connection.start_transport();
        Ok(connection)
    }
}

impl<C> QuicConnection<C>
where
    C: Clone + ConnectedState + Send + 'static,
{
    pub(super) fn close_stream(&self, stream_id: StreamId) {
        let this = self.clone();
        tokio::spawn(async move {
            let command = ConnectionCommand::Close { stream_id };
            let mut guard = this.command_send_stream.lock().await;
            let send_stream = guard.as_mut().unwrap();
            if let Err(e) = command.write_to_stream(send_stream).await {
                info!("manually close stream failed: {}", e);
            }
        });
    }

    pub(super) fn stream_send(&self, stream_id: StreamId, bytes: Bytes) {
        let (sender, _) = &self.chan_mpsc;
        let _ = sender.send(StreamData { stream_id, bytes });
    }

    pub(super) fn register(&self, stream_id: StreamId, sender: Sender<Bytes>) {
        self.sender_map.insert(stream_id, sender);
    }

    pub(super) fn deregister(&self, stream_id: StreamId) {
        self.sender_map.remove(&stream_id);
    }
}

impl QuicConnection<ClientState> {
    pub async fn open_datagram_stream(
        &self,
        stream_type: NetworkType,
        peer_addr: SocketAddr,
    ) -> Result<DatagramStream<ClientState>, CommandError> {
        let id = self.state.id_gen.new_id();
        info!("new stream, id: {}", id);
        let command = match stream_type {
            NetworkType::Udp => ConnectionCommand::OpenUdp { id, peer_addr },
            NetworkType::Tcp => ConnectionCommand::OpenTcp { id, peer_addr },
        };
        let stream = self.build_stream(stream_type, id, peer_addr);
        let mut send_stream = self.command_send_stream.lock().await;
        command
            .write_to_stream(send_stream.as_mut().unwrap())
            .await?;
        Ok(stream)
    }
}

impl QuicConnection<ServerState> {
    pub async fn accecpt_stream(&self) -> DatagramStream<ServerState> {
        loop {
            if let Some(stream) = self.state.stream_queue.pop() {
                return stream;
            }
            self.state.new_stream.notified().await;
        }
    }
}

pub trait HandleCommand<C>
where
    C: ConnectedState + Clone,
{
    fn handle_command(
        command: ConnectionCommand,
        connection: &QuicConnection<C>,
        pending_queue: &mut HashMap<StreamId, Vec<Bytes>>,
    );
}

impl HandleCommand<ClientState> for ClientState {
    fn handle_command(
        command: ConnectionCommand,
        connection: &QuicConnection<ClientState>,
        _: &mut HashMap<StreamId, Vec<Bytes>>,
    ) {
        if let ConnectionCommand::Close { stream_id } = command {
            if let Some(notifier) = connection.close_signal_map.get(&stream_id) {
                notifier.notify_waiters();
            }
        }
    }
}

impl HandleCommand<ServerState> for ServerState {
    fn handle_command(
        command: ConnectionCommand,
        connection: &QuicConnection<ServerState>,
        pending_queue: &mut HashMap<StreamId, Vec<Bytes>>,
    ) {
        match command {
            ConnectionCommand::Auth { .. } => {}
            ConnectionCommand::Close { stream_id } => {
                if let Some(notifier) = connection.close_signal_map.get(&stream_id) {
                    notifier.notify_waiters();
                }
            }
            _ => {
                let (id, stream) = match command {
                    ConnectionCommand::OpenUdp { id, peer_addr } => {
                        (id, connection.build_stream(NetworkType::Udp, id, peer_addr))
                    }
                    ConnectionCommand::OpenTcp { id, peer_addr } => {
                        (id, connection.build_stream(NetworkType::Tcp, id, peer_addr))
                    }
                    _ => unreachable!(),
                };

                // resend pending datagrams
                if let Some(pending) = pending_queue.remove(&id) {
                    if let Some(sender) = connection.sender_map.get(&id) {
                        for bytes in pending {
                            let _ = sender.send(bytes);
                        }
                    }
                }

                connection.state.stream_queue.push(stream);
                connection.state.new_stream.notify_waiters();
            }
        }
    }
}

impl<C> QuicConnection<C>
where
    C: ConnectedState + HandleCommand<C> + Clone + Send + Sync + 'static,
{
    fn start_transport(&self) {
        let conn_id = self.connection.stable_id();

        let this = self.clone();
        tokio::spawn(async move {
            let QuicConnectionInner {
                connection,
                command_recv_stream,
                chan_mpsc: (_, receiver),
                sender_map,
                closed,
                ..
            } = this.as_ref();

            let mut guard = command_recv_stream.lock().await;
            let recv_stream = guard.as_mut().unwrap();

            let mut pending_queue = HashMap::new();

            loop {
                tokio::select! {
                    Ok(data) = receiver.recv_async() => {
                        if let Err(e) = connection.send_datagram(data.to_be_bytes()) {
                            info!("send datagram error: {}, conn id: {}", e, conn_id);
                            break;
                        }
                    }
                    recv = connection.read_datagram() => {
                        let bytes = match recv {
                            Err(e) => {
                                info!("read datagram error: {} ", e);
                                break;
                            },
                            Ok(bytes) => bytes,
                        };
                        match StreamData::try_from(bytes) {
                            Err(e) => info!("bytes decode failed: {}", e),
                            Ok(data) => {
                                let id = data.stream_id;
                                match sender_map.get(&id) {
                                    None => {
                                        let vec = pending_queue.entry(id).or_insert(vec![]);
                                        vec.push(data.bytes);
                                    }
                                    Some(sender) => {
                                        let _ = sender.send(data.bytes);
                                    }
                                }
                            }
                        }
                    }
                    res = read_command(recv_stream) => {
                        match res {
                            Err(e) => {
                                info!("accept uni stream failed: {}", e);
                                break;
                            }
                            Ok(command) => {
                                <C as HandleCommand<C>>::handle_command(
                                    command,
                                    &this,
                                    &mut pending_queue,
                                );
                            }
                        };
                    }
                }
            }
            closed.notify_waiters();
        });
    }

    fn build_stream(
        &self,
        stream_type: NetworkType,
        id: StreamId,
        peer_addr: SocketAddr,
    ) -> DatagramStream<C> {
        let manual_close = Arc::new(Notify::new());
        let stream = DatagramStream(Arc::new(DatagramStreamInner::new(
            self.clone(),
            id,
            stream_type,
            peer_addr,
            self.stream_idle_timeout,
            manual_close.clone(),
        )));
        self.close_signal_map.insert(id, manual_close);
        stream.wait_close(self.closed.clone());
        stream
    }
}

#[derive(Copy, Clone, Debug, Error, PartialEq, Eq)]
pub enum DatagramStreamError {
    #[error("underlying connection error")]
    ConnectionError,
    #[error("timeout")]
    IdleTimeout,
    #[error("manual closed by connection")]
    ManualClosed,
    #[error("flume channel receive error")]
    FlumeRecvError(flume::RecvError),
}

#[derive(Default)]
struct StreamIdGenerator {
    current: AtomicU16,
}

impl StreamIdGenerator {
    // wrapping around when overflowed
    fn new_id(&self) -> StreamId {
        StreamId(self.current.fetch_add(1, Ordering::SeqCst))
    }
}

#[derive(PartialEq, Eq, Hash, Copy, Clone)]
pub struct StreamId(u16);

impl Display for StreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl StreamId {
    pub(super) fn to_be_bytes(self) -> [u8; 2] {
        self.0.to_be_bytes()
    }

    pub(super) fn from_be_bytes(bytes: [u8; 2]) -> Self {
        Self(u16::from_be_bytes(bytes))
    }
}

#[derive(Clone)]
struct StreamData {
    stream_id: StreamId,
    bytes: Bytes,
}

impl StreamData {
    fn len(&self) -> usize {
        2 + self.bytes.len()
    }

    fn to_be_bytes(&self) -> Bytes {
        let mut res = BytesMut::with_capacity(self.len());
        res.extend_from_slice(&self.stream_id.to_be_bytes());
        res.extend_from_slice(&self.bytes);
        res.freeze()
    }
}

impl TryFrom<Bytes> for StreamData {
    type Error = DecodeDataError<Bytes>;

    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        Ok(StreamData {
            stream_id: StreamId::from_be_bytes(match bytes[0..2].try_into() {
                Err(_) => return Err(DecodeDataError(bytes)),
                Ok(bytes) => bytes,
            }),
            bytes: bytes.slice(2..),
        })
    }
}

#[derive(Copy, Clone, Debug, Error, PartialEq, Eq)]
#[error("decode stream data failed")]
pub struct DecodeDataError<T>(pub T);

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CommandError {
    #[error("connection error")]
    ConnectionError(QuinnError),
    #[error("data error")]
    DataError(DecodeCommandError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuinnError {
    Connection(quinn::ConnectionError),
    Write(quinn::WriteError),
    ReadExact(quinn::ReadExactError),
}

impl From<quinn::ReadExactError> for CommandError {
    fn from(value: quinn::ReadExactError) -> Self {
        Self::ConnectionError(QuinnError::ReadExact(value))
    }
}
impl From<quinn::WriteError> for CommandError {
    fn from(value: quinn::WriteError) -> Self {
        Self::ConnectionError(QuinnError::Write(value))
    }
}
impl From<quinn::ConnectionError> for CommandError {
    fn from(value: quinn::ConnectionError) -> Self {
        Self::ConnectionError(QuinnError::Connection(value))
    }
}
impl From<DecodeCommandError> for CommandError {
    fn from(value: DecodeCommandError) -> Self {
        Self::DataError(value)
    }
}
