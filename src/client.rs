mod client_tcp;
mod client_udp;
#[cfg(feature = "quinn")]
mod endpoint_quinn;
#[cfg(feature = "s2n-quic")]
mod endpoint_s2n;

use crate::{
    common::util,
    config::{self, NetworkType},
    quic::{connection::Connection, endpoint::ClientEndpoint, *},
};
#[cfg(feature = "quinn")]
use endpoint_quinn::build_endpoint;
#[cfg(feature = "s2n-quic")]
use endpoint_s2n::build_endpoint;
use log::{info, warn};
use std::{net::SocketAddr, sync::Arc};
use tokio::{sync::Mutex, time::Duration};
use {client_tcp::*, client_udp::*};

pub struct QuicConnManager {
    conn_handle: Mutex<Option<QuicConnection<ClientState>>>,
    endpoint: Box<dyn ClientEndpoint>,
    server_addr: SocketAddr,
    server_name: String,
    psk: Arc<Vec<u8>>,
    enable_0rtt: Option<bool>,
    stream_idle_timeout: u64,
}

impl QuicConnManager {
    fn new(
        endpoint: Box<dyn ClientEndpoint>,
        server_addr: SocketAddr,
        server_name: String,
        psk: Vec<u8>,
        enable_0rtt: Option<bool>,
        stream_idle_timeout: u64,
    ) -> Self {
        Self {
            conn_handle: Mutex::new(None),
            endpoint,
            server_addr,
            server_name,
            psk: Arc::new(psk),
            enable_0rtt,
            stream_idle_timeout,
        }
    }

    pub(super) async fn handle(&self) -> QuicConnection<ClientState> {
        let mut guard = self.conn_handle.lock().await;
        if let Some(handle) = guard.as_ref() {
            if !handle.connection_closed() {
                return handle.clone();
            }
        }
        let connection = self.endpoint_connect().await;
        let new_handle =
            ConnectionBuilder::new(connection, self.psk.clone(), self.stream_idle_timeout)
                .build_client()
                .await
                .expect("open connection failed");
        guard.replace(new_handle.clone());
        new_handle
    }

    async fn endpoint_connect(&self) -> Box<dyn Connection> {
        loop {
            info!("connecting to {}", self.server_addr);
            match self
                .endpoint
                .connect(self.server_addr, &self.server_name, self.enable_0rtt)
                .await
            {
                Ok(conn) => {
                    return conn;
                }
                Err(e) => {
                    warn!("connect to {} failed: {e}", self.server_addr);
                }
            }
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
    }
}

pub async fn client(config: config::Config) {
    let client_conf = config.client.expect("client field missing");
    let psk = util::decode_psk(&config.psk).expect("decode psk failed");
    let server_addr = tokio::net::lookup_host(&client_conf.server_addr)
        .await
        .expect("failed to lookup host");
    let server_addr = server_addr
        .into_iter()
        .next()
        .expect("server ip address not found");
    let endpoint = build_endpoint(
        client_conf.cert_ver,
        client_conf.cert_path,
        config.congestion,
        server_addr,
        config.connection_idle_timeout,
    )
    .expect("build endpoint failed");

    info!("client listening on: {}", client_conf.listen);

    let conn_man = QuicConnManager::new(
        endpoint,
        server_addr,
        client_conf.server_name,
        psk,
        config.enable_0rtt,
        config.stream_idle_timeout,
    );

    match client_conf.network {
        NetworkType::Tcp => {
            spawn_tcp_sockets(client_conf.listen, client_conf.peer, conn_man).await;
        }
        NetworkType::Udp => {
            spawn_udp_sockets(client_conf.listen, client_conf.peer, conn_man).await;
        }
    }
}
