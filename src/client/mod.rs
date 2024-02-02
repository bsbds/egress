mod dispatcher;

mod udp;

use std::{io, net::SocketAddr, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::FutureExt;
use log::warn;
use quinn::{ClientConfig, Connection as QuinnConnection, Endpoint};
use rustls::{Certificate, PrivateKey};

use crate::{
    config,
    quic::{
        connection::{Connection, ConnectionConfig, QuicConn},
        error::ConnectionError,
        stream::Dispatcher,
    },
    utils::{init_certs_and_key, new_transport_config},
};

struct AutoReconnect {
    endpoint: Endpoint,
    connection: tokio::sync::RwLock<Option<QuinnConnection>>,
    server_addr: SocketAddr,
    server_name: String,
}

impl AutoReconnect {
    async fn connect(
        endpoint: Endpoint,
        server_addr: SocketAddr,
        server_name: String,
    ) -> io::Result<Self> {
        let connection = endpoint
            .connect(server_addr, &server_name)
            .expect("endpoint connect failed")
            .await?;

        Ok(Self {
            endpoint,
            connection: tokio::sync::RwLock::new(Some(connection)),
            server_addr,
            server_name,
        })
    }

    async fn reconnect(&self) {
        loop {
            let conn_result = self
                .endpoint
                .connect(self.server_addr, &self.server_name)
                .expect("endpoint connect failed")
                .await;
            match conn_result {
                Ok(new_conn) => {
                    self.connection.write().await.replace(new_conn);
                    return;
                }
                Err(e) => {
                    warn!("connection error: {e}, reconnect in 5s");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn reconnect_on_failure<F, Fut, R>(&self, op: F) -> R
    where
        Fut: futures::Future<Output = Option<R>>,
        F: Fn() -> Fut,
    {
        loop {
            let fut = op();
            match fut.await {
                Some(res) => return res,
                None => self.reconnect().await,
            }
        }
    }

    async fn send_datagram(&self, data: Bytes) -> Result<(), quinn::SendDatagramError> {
        let conn_l = self.connection.read().await;
        conn_l.as_ref().unwrap().send_datagram(data)
    }

    async fn recv_datagram(&self) -> Result<Bytes, quinn::ConnectionError> {
        let conn_l = self.connection.read().await;
        conn_l.as_ref().unwrap().read_datagram().await
    }
}

#[async_trait]
impl QuicConn for AutoReconnect {
    async fn id(&self) -> u64 {
        todo!()
    }

    async fn close(&self) {
        todo!()
    }

    async fn closed(&self) -> bool {
        let conn_l = self.connection.read().await;
        conn_l.as_ref().unwrap().close_reason().is_some()
    }

    async fn send_datagram(&self, data: Bytes) -> Result<(), ConnectionError> {
        self.reconnect_on_failure(|| self.send_datagram(data.clone()).map(Result::ok))
            .await;

        Ok(())
    }

    async fn read_datagram(&self) -> Result<Bytes, ConnectionError> {
        Ok(self
            .reconnect_on_failure(|| self.recv_datagram().map(Result::ok))
            .await)
    }
}

pub async fn run(config: config::Client) -> io::Result<()> {
    let endpoint = build_endpoint(&config)?;
    let reconnect = AutoReconnect::connect(
        endpoint,
        config.server_addr.parse().unwrap(),
        config.server_name.clone(),
    )
    .await?;
    let conn = Connection::new(
        reconnect,
        ConnectionConfig::new(config.common.max_segment_size),
    );
    let dispatcher = Arc::new(Dispatcher::new_client(Arc::new(conn), config));
    let dispatcher_c = Arc::clone(&dispatcher);
    tokio::spawn(async move {
        dispatcher.dispatch_task().await.unwrap();
    });
    tokio::spawn(async move {
        dispatcher_c.connection_task().await.unwrap();
    });

    Ok(())
}

fn build_endpoint(config: &config::Client) -> io::Result<Endpoint> {
    let (roots, cert_chain, private_key) = init_certs_and_key(&config.common)?;
    let mut quinn_client_config =
        ClientConfig::new(Arc::new(client_config(roots, cert_chain, private_key)));
    let transport_config = new_transport_config(&config.common);
    quinn_client_config.transport_config(Arc::new(transport_config));
    let addr: SocketAddr = match config.server_addr.parse().unwrap() {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    }
    .parse()
    .unwrap();
    let mut endpoint = Endpoint::client(addr)?;
    endpoint.set_default_client_config(quinn_client_config);

    Ok(endpoint)
}

fn client_config(
    roots: rustls::RootCertStore,
    cert_chain: Vec<Certificate>,
    key_der: PrivateKey,
) -> rustls::ClientConfig {
    let mut cfg = rustls::ClientConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_client_auth_cert(cert_chain, key_der)
        .unwrap();
    cfg.enable_early_data = true;
    cfg
}
