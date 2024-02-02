use std::sync::Arc;

use bytes::Buf;
use log::debug;

use crate::{
    config,
    quic::{
        connection::{Connection, QuicConn},
        error::ConnectionError,
        stream::Dispatcher,
    },
};

use super::udp::UdpPeerSocket;

pub(super) struct Client {
    config: config::Client,
}

impl<C> Dispatcher<Client, C>
where
    C: QuicConn,
{
    pub fn new_client(conn: Arc<Connection<C>>, config: config::Client) -> Self {
        Dispatcher::new(conn, Client { config })
    }

    pub async fn dispatch_task(&self) -> Result<(), ConnectionError> {
        loop {
            let mut data = self.conn.recv().await?;
            if data.len() < 2 {
                debug!("received data is invalid");
                continue;
            }
            let id = data.get_u16();
            let Some(tx) = self.streams.get(&id) else {
                debug!("stream id: {id} not found");
                continue;
            };
            if let Err(e) = tx.send(data) {
                debug!("sending to a closed stream: {e}, removing");
                self.streams.remove(&id);
            }
        }
    }

    pub async fn connection_task(&self) -> Result<(), ConnectionError> {
        match self.inner.config.common.network {
            config::Network::Udp => {
                let bind_addr = if self.inner.config.listen_addr.is_ipv4() {
                    "0.0.0.0:0"
                } else {
                    "::0:0"
                };
                let socket = UdpPeerSocket::bind(bind_addr).await?;
                loop {
                    let peer = socket.accept().await?;
                    let stream = self.open_stream();
                    tokio::spawn(async move {
                        loop {
                            tokio::select! {
                                data = peer.recv() => {
                                    stream.send(data).await.unwrap();
                                }
                                res = stream.recv() => {
                                    let data = res.unwrap();
                                    peer.send(data).await.unwrap();
                                }
                            }
                        }
                    });
                }
            }
            config::Network::Tcp => {
                todo!()
            }
        }
    }
}
