use crate::{common::constant::UDP_RECV_BUF_SIZE, quic::*};
use bytes::Bytes;
use log::{error, info};
use std::net::SocketAddr;
use tokio::{self, net::UdpSocket};

fn accept_connections(datagram_stream: DatagramStream<ServerState>, socket: UdpSocket) {
    tokio::spawn(async move {
        let mut buf = [0; UDP_RECV_BUF_SIZE];
        loop {
            tokio::select! {
                res = socket.recv(&mut buf) => {
                    match res {
                        Ok(len) => {
                            datagram_stream.send(Bytes::copy_from_slice(&buf[..len]));
                        }
                        Err(e) => {
                            error!("socket recv error: {}", e);
                            break;
                        }
                    }
                }
                res = datagram_stream.recv() => {
                    match res {
                        Err(e) => {
                            info!("channel closed: {}", e);
                            break;
                        }
                        Ok(bytes) => {
                            if let Err(e) = socket.send(&bytes).await {
                                error!("socket send error: {}", e);
                                break;
                            }
                        }
                    }
                }
                err = datagram_stream.closed() => {
                    info!("quic stream closed: {}", err);
                    break;
                }
            }
        }
    });
}

async fn new_udp_socket(peer_addr: SocketAddr) -> Result<UdpSocket, Box<dyn std::error::Error>> {
    let listen_addr: SocketAddr = if peer_addr.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    }
    .parse()?;
    let socket = UdpSocket::bind(listen_addr).await?;
    socket.connect(peer_addr).await?;
    Ok(socket)
}

pub async fn spawn_udp_sockets(quic_stream: DatagramStream<ServerState>, peer_addr: SocketAddr) {
    let socket = new_udp_socket(peer_addr)
        .await
        .expect("create socket failed");
    accept_connections(quic_stream, socket);
}
