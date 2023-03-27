use crate::{
    common::tcp_util::tcp_transport,
    quic::{DatagramStream, ServerState},
};
use log::error;
use std::net::SocketAddr;
use tokio::net::TcpStream;

pub async fn spawn_tcp_sockets(
    datagram_stream: DatagramStream<ServerState>,
    peer_addr: SocketAddr,
) {
    let tcp_stream = match TcpStream::connect(peer_addr).await {
        Err(e) => {
            error!("tcp stream connect failed: {}", e);
            return;
        }
        Ok(stream) => stream,
    };
    tcp_transport(tcp_stream, datagram_stream);
}
