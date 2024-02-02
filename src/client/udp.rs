use std::{collections::HashMap, io, net::SocketAddr, sync::Arc};

use bytes::{Bytes, BytesMut};
use flume::{Receiver, Sender};
use log::error;
use tokio::net::{ToSocketAddrs, UdpSocket};

pub struct UdpPeerSocket {
    peer_rx: Receiver<Result<UdpPeer, io::Error>>,
}

impl UdpPeerSocket {
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let socket = Arc::new(UdpSocket::bind(addr).await?);
        let mut buf = BytesMut::new();
        let (peer_tx, peer_rx) = flume::bounded(1024);
        tokio::spawn(async move {
            // TODO: GC the map
            let mut peer_map = HashMap::<SocketAddr, Sender<Bytes>>::new();
            loop {
                let (len, addr) = match socket.recv_buf_from(&mut buf).await {
                    Ok(r) => r,
                    Err(e) => {
                        error!("Udp socket recv failed: {e}");
                        let _ignore = peer_tx.send(Err(e));
                        return;
                    }
                };
                match peer_map.get(&addr) {
                    Some(data_tx) => {
                        data_tx.send(buf.split_to(len).freeze());
                    }
                    None => {
                        let (data_tx, data_rx) = flume::bounded(1024);
                        data_tx.send(buf.split_to(len).freeze());
                        peer_map.insert(addr, data_tx);
                        let peer = UdpPeer {
                            addr,
                            socket_ref: Arc::clone(&socket),
                            rx: data_rx,
                        };
                        peer_tx.send(Ok(peer));
                    }
                }
            }
        });

        Ok(Self { peer_rx })
    }

    pub async fn accept(&self) -> io::Result<UdpPeer> {
        match self.peer_rx.recv_async().await {
            Ok(res) => res,
            Err(_) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Closed udp socket",
            )),
        }
    }
}

pub struct UdpPeer {
    addr: SocketAddr,
    socket_ref: Arc<UdpSocket>,
    rx: Receiver<Bytes>,
}

impl UdpPeer {
    pub async fn send(&self, bytes: Bytes) -> io::Result<usize> {
        self.socket_ref.send_to(&bytes, self.addr).await
    }

    pub async fn recv(&self) -> Bytes {
        self.rx.recv_async().await.unwrap()
    }
}
