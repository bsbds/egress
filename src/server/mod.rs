pub mod dispatcher;
mod stream_handler;

use std::{io, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use quinn::{Endpoint, ServerConfig};
use rustls::server::AllowAnyAuthenticatedClient;

use crate::{
    config,
    quic::{
        connection::{Connection, ConnectionConfig, QuicConn},
        error::ConnectionError,
        stream::Dispatcher,
    },
    utils::{init_certs_and_key, new_transport_config},
};

#[async_trait]
impl QuicConn for quinn::Connection {
    async fn id(&self) -> u64 {
        self.stable_id() as u64
    }

    async fn close(&self) {
        self.close(0u32.into(), &[]);
    }

    async fn closed(&self) -> bool {
        self.close_reason().is_some()
    }

    async fn send_datagram(&self, data: Bytes) -> Result<(), ConnectionError> {
        self.send_datagram(data)
            .map_err(|e| ConnectionError::SendDatagram(e.to_string()))
    }

    async fn read_datagram(&self) -> Result<Bytes, ConnectionError> {
        self.read_datagram()
            .await
            .map_err(|e| ConnectionError::ReadDatagram(e.to_string()))
    }
}

pub async fn run(config: config::Server) -> io::Result<()> {
    let endpoint = build_endpoint(&config)?;

    while let Some(connecting) = endpoint.accept().await {
        let connection = connecting.await?;
        let conn = Connection::new(
            connection,
            ConnectionConfig::new(config.common.max_segment_size),
        );
        let dispatcher = Dispatcher::new_server(Arc::new(conn), config.clone());
        let _ignore = tokio::spawn(dispatcher.dispatch_task());
    }

    Ok(())
}

fn build_endpoint(config: &config::Server) -> io::Result<Endpoint> {
    let (roots, cert_chain, private_key) = init_certs_and_key(&config.common)?;
    let mut quinn_server_config = ServerConfig::with_crypto(Arc::new(
        server_config(roots, cert_chain, private_key)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("rustls error: {e}")))?,
    ));
    let transport_config = new_transport_config(&config.common);
    quinn_server_config.transport_config(Arc::new(transport_config));

    Endpoint::server(quinn_server_config, config.listen)
}

pub(crate) fn server_config(
    roots: rustls::RootCertStore,
    cert_chain: Vec<rustls::Certificate>,
    key: rustls::PrivateKey,
) -> Result<rustls::ServerConfig, rustls::Error> {
    let mut cfg = rustls::ServerConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_client_cert_verifier(Arc::new(AllowAnyAuthenticatedClient::new(roots)))
        .with_single_cert(cert_chain, key)?;
    cfg.max_early_data_size = u32::MAX;
    Ok(cfg)
}
