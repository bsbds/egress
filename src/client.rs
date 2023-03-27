mod client_tcp;
mod client_udp;

use crate::{
    common::util,
    config::{self, Congestion, NetworkType},
    quic::*,
};
use log::{info, warn};
use quinn::{self, Connecting, Connection, Endpoint};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    sync::Mutex,
    time::{Duration, Instant},
};
use {client_tcp::*, client_udp::*};

pub struct QuicConnMananger {
    conn_handle: Mutex<Option<QuicConnection<ClientState>>>,
    endpoint: Endpoint,
    server_addr: SocketAddr,
    server_name: String,
    psk: Arc<Vec<u8>>,
    enable_0rtt: bool,
    stream_idle_timeout: u64,
}

impl QuicConnMananger {
    fn new(
        endpoint: Endpoint,
        server_addr: SocketAddr,
        server_name: String,
        psk: Vec<u8>,
        enable_0rtt: bool,
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
            if !handle.connetion_closed() {
                return handle.clone();
            }
        }
        let connection = self.endpoint_connect().await;
        let new_handle =
            QuicConnection::new(connection, self.psk.clone(), self.stream_idle_timeout)
                .await
                .into_client()
                .await
                .expect("open connection failed");
        guard.replace(new_handle.clone());
        new_handle
    }

    async fn endpoint_connect(&self) -> Connection {
        let conn = loop {
            let connecting = self
                .endpoint
                .connect(self.server_addr, &self.server_name)
                .expect("endpoint connect failed");
            match self.build_connection(connecting, self.enable_0rtt).await {
                Ok(conn) => break conn,
                Err(e) => {
                    warn!("connection failed: {}, sleep for 4 secs", e);
                }
            }
            tokio::time::sleep(Duration::from_secs(4)).await;
        };
        conn
    }

    async fn build_connection(
        &self,
        connecting: Connecting,
        enable_0rtt: bool,
    ) -> Result<Connection, Box<dyn std::error::Error>> {
        let connection = if enable_0rtt {
            match connecting.into_0rtt() {
                Ok((conn, _)) => {
                    info!("using 0-rtt handshake");
                    conn
                }
                Err(conn) => {
                    let timer = Instant::now();
                    let conn = conn.await?;
                    info!("handshake complete: {}ms", timer.elapsed().as_millis());
                    conn
                }
            }
        } else {
            connecting.await?
        };
        Ok(connection)
    }
}

struct SkipServerVerification;
impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}
impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

fn configure_client() -> quinn::ClientConfig {
    let mut crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();
    crypto.enable_early_data = true;
    quinn::ClientConfig::new(Arc::new(crypto))
}

fn build_endpoint(
    cert_ver: bool,
    congestion: Congestion,
    max_udp_payload: u16,
    server_addr: SocketAddr,
    conn_idle_timout: u64,
) -> Result<Endpoint, Box<dyn std::error::Error>> {
    let mut client_config = if cert_ver {
        quinn::ClientConfig::with_native_roots()
    } else {
        warn!("skipping certificate varification");
        configure_client()
    };

    let transport = util::new_transport(max_udp_payload, congestion, conn_idle_timout);
    client_config.transport_config(Arc::new(transport));

    let addr: SocketAddr = match server_addr {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    }
    .parse()
    .unwrap();

    let mut endpoint = Endpoint::client(addr)?;
    endpoint.set_default_client_config(client_config);

    Ok(endpoint)
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
        config.congestion,
        config.max_udp_payload,
        server_addr,
        config.connection_idle_timeout,
    )
    .expect("build endpoint failed");

    info!("client listening on: {}", client_conf.listen);

    let conn_man = QuicConnMananger::new(
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
