use super::QuicConnManager;
use crate::{
    common::constant::{FLUME_CHANNEL_SIZE, UDP_RECV_BUF_SIZE},
    config::NetworkType,
};
use bytes::Bytes;
use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use flume::{Receiver, Sender};
use log::{error, info};
use std::{net::SocketAddr, sync::Arc};
use thiserror::Error;
use tokio::{net::UdpSocket, sync::Notify};

#[derive(Clone)]
pub struct UdpState(Arc<UdpStateInner>);

pub struct UdpStateInner {
    socket: UdpSocket,
    sender_map: Arc<DashMap<SocketAddr, Sender<Bytes>>>,
    peer_queue: SegQueue<UdpPeer>,
    new_peer: Arc<Notify>,
}

impl std::ops::Deref for UdpState {
    type Target = Arc<UdpStateInner>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl UdpState {
    pub fn new(socket: UdpSocket) -> Self {
        Self(Arc::new(UdpStateInner {
            socket,
            sender_map: Arc::new(DashMap::new()),
            peer_queue: SegQueue::new(),
            new_peer: Arc::new(Notify::new()),
        }))
    }

    pub async fn accept(&self) -> UdpPeer {
        loop {
            if let Some(peer) = self.peer_queue.pop() {
                return peer;
            }
            self.new_peer.notified().await;
        }
    }

    pub fn listen(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut buf = [0; UDP_RECV_BUF_SIZE];
            let UdpStateInner {
                socket,
                peer_queue,
                sender_map,
                new_peer,
            } = self.as_ref();

            loop {
                tokio::select! {
                    res = socket.recv_from(&mut buf) => {
                        match res {
                            Ok((len, addr)) => {
                                let sender = match sender_map.get(&addr) {
                                    None => {
                                        peer_queue.push(UdpPeer::new(self.clone(), addr));
                                        let sender = sender_map.get(&addr).unwrap().clone();
                                        new_peer.notify_waiters();
                                        sender
                                    }
                                    Some(sender) => {
                                        sender.clone()
                                    }
                                };
                                if let Err(e) = sender.send(Bytes::copy_from_slice(&buf[..len])) {
                                    error!("send error: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("socket recv error: {}", e);
                                return;
                            }
                        }
                    }
                }
            }
        })
    }

    async fn peer_send(&self, bytes: Bytes, peer_addr: SocketAddr) -> std::io::Result<usize> {
        self.socket.send_to(&bytes, peer_addr).await
    }

    fn register(&self, peer_addr: SocketAddr, sender: Sender<Bytes>) {
        self.sender_map.insert(peer_addr, sender);
    }

    fn dergister(&self, peer_addr: SocketAddr) {
        self.sender_map.remove(&peer_addr);
    }
}

pub struct UdpPeer {
    parent_state: UdpState,
    peer_addr: SocketAddr,
    receiver: Receiver<Bytes>,
}

impl UdpPeer {
    pub fn new(parent_state: UdpState, peer_addr: SocketAddr) -> Self {
        let (sender, receiver) = flume::bounded(FLUME_CHANNEL_SIZE);
        parent_state.register(peer_addr, sender);
        Self {
            parent_state,
            peer_addr,
            receiver,
        }
    }

    pub async fn send(&self, bytes: Bytes) -> std::io::Result<usize> {
        self.parent_state.peer_send(bytes, self.peer_addr).await
    }

    pub async fn recv(&self) -> Result<Bytes, UdpPeerError> {
        match self.receiver.recv_async().await {
            Err(e) => Err(UdpPeerError::FlumeRecvError(e)),
            Ok(res) => Ok(res),
        }
    }
}

impl Drop for UdpPeer {
    fn drop(&mut self) {
        self.parent_state.dergister(self.peer_addr);
    }
}

#[derive(Copy, Clone, Debug, Error, PartialEq, Eq)]
pub enum UdpPeerError {
    #[error("flume channel receive error")]
    FlumeRecvError(flume::RecvError),
}

async fn accept_connections(listener: UdpState, peer_addr: SocketAddr, conn_man: QuicConnManager) {
    loop {
        let udp_peer = listener.accept().await;
        let handle = conn_man.handle().await;
        let quic_stream = match handle
            .open_datagram_stream(NetworkType::Udp, peer_addr)
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                info!("open datagram stream failed: {e}, reconnecting");
                continue;
            }
        };
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    res = udp_peer.recv() => {
                        match res {
                            Err(e) => {
                                info!("udp recv failed: {}", e);
                                break;
                            }
                            Ok(bytes) => {
                                quic_stream.send(bytes);
                            }
                        }
                    }
                    res = quic_stream.recv() => {
                        match res {
                            Err(e) => {
                                info!("channel closed: {}", e);
                                break;
                            }
                            Ok(bytes) => {
                                if let Err(e) = udp_peer.send(bytes).await {
                                    info!("udp send failed: {}", e);
                                }
                            }
                        }
                    }
                    err = quic_stream.closed() => {
                        info!("quic stream closed: {}", err);
                        break;
                    }
                }
            }
        });
    }
}

pub async fn spawn_udp_sockets(
    udp_listen: SocketAddr,
    peer_addr: SocketAddr,
    conn_man: QuicConnManager,
) {
    let socket = UdpSocket::bind(udp_listen)
        .await
        .expect("create socket failed");
    let state = UdpState::new(socket);
    state.clone().listen();
    accept_connections(state, peer_addr, conn_man).await;
}
