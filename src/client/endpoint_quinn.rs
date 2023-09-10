use crate::{common::util, config::Congestion, quic::endpoint::ClientEndpoint};
use log::warn;
use quinn::{self, Endpoint};
use std::{net::SocketAddr, sync::Arc};

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

pub(super) fn build_endpoint(
    cert_ver: bool,
    _cert_path: Option<String>,
    congestion: Congestion,
    server_addr: SocketAddr,
    conn_idle_timeout: u64,
) -> Result<Box<dyn ClientEndpoint>, Box<dyn std::error::Error>> {
    let mut client_config = if cert_ver {
        quinn::ClientConfig::with_native_roots()
    } else {
        warn!("skipping certificate verification");
        configure_client()
    };

    let transport = util::quinn::new_transport(congestion, conn_idle_timeout);
    client_config.transport_config(Arc::new(transport));

    let addr: SocketAddr = match server_addr {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    }
    .parse()
    .unwrap();

    let mut endpoint = Endpoint::client(addr)?;
    endpoint.set_default_client_config(client_config);

    Ok(Box::new(endpoint))
}
