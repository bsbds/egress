use std::sync::{atomic::Ordering, Arc};

use bytes::{Buf, BytesMut};
use log::debug;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
};

use crate::{
    config,
    quic::{
        connection::{Connection, QuicConn},
        error::ConnectionError,
        stream::{Dispatcher, Stream},
    },
};

pub struct Server {
    config: config::Server,
}

impl<C> Dispatcher<Server, C>
where
    C: QuicConn,
{
    pub fn new_server(conn: Arc<Connection<C>>, config: config::Server) -> Self {
        Dispatcher::new(conn, Server { config })
    }

    pub async fn dispatch_task(self) -> Result<(), ConnectionError> {
        loop {
            let mut data = self.conn.recv().await?;
            if data.len() < 2 {
                debug!("received data is invalid");
                continue;
            }
            let id = data.get_u16();
            let tx = match self.streams.get(&id) {
                Some(tx) => tx,
                None => {
                    if id < self.current_id.load(Ordering::Relaxed) {
                        debug!("received outdated stream, id: {id}");
                        continue;
                    }
                    let stream = self.open_stream();
                    let _ignore =
                        tokio::spawn(handle_stream_task(stream, self.inner.config.clone()));
                    self.streams.get(&id).unwrap()
                }
            };
            if let Err(e) = tx.send(data) {
                debug!("sending to a closed stream: {e}, removing");
                self.streams.remove(&id);
            }
        }
    }
}

async fn handle_stream_task<C>(
    stream: Stream<C>,
    config: config::Server,
) -> Result<(), ConnectionError>
where
    C: QuicConn,
{
    match config.common.network {
        config::Network::Udp => {
            let bind_addr = if config.peer_addr.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "::0:0"
            };
            let socket = UdpSocket::bind(bind_addr).await?;
            socket.connect(config.peer_addr).await?;
            let mut buf = BytesMut::new();
            loop {
                tokio::select! {
                    res = socket.recv_buf(&mut buf) => {
                        let len = res?;
                        stream.send(buf.split_to(len).freeze()).await?;
                    }
                    res = stream.recv() => {
                        let data = res?;
                        socket.send(&data).await?;
                    }
                }
            }
        }
        config::Network::Tcp => {
            let mut tcp_stream = TcpStream::connect(config.peer_addr).await?;
            let mut buf = BytesMut::new();
            loop {
                tokio::select! {
                    res = tcp_stream.read_buf(&mut buf) => {
                        let len = res?;
                        stream.send(buf.split_to(len).freeze()).await?;
                    }
                    res = stream.recv() => {
                        let data = res?;
                        tcp_stream.write_all(&data).await?;
                    }
                }
            }
        }
    }
}
