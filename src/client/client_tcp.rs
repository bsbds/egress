use super::QuicConnMananger;
use crate::{common::tcp_util::tcp_transport, config::NetworkType};
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub async fn spawn_tcp_sockets(
    listen_addr: SocketAddr,
    peer_addr: SocketAddr,
    conn_man: QuicConnMananger,
) {
    let listener = TcpListener::bind(listen_addr)
        .await
        .expect("bind tcp socket failed");
    loop {
        let (tcp_stream, _) = match listener.accept().await {
            Err(_) => {
                continue;
            }
            Ok(p) => p,
        };
        let handle = conn_man.handle().await;
        let quic_stream = handle
            .open_datagram_stream(NetworkType::Tcp, peer_addr)
            .await
            .expect("open stream failed");
        tcp_transport(tcp_stream, quic_stream);
    }
}
