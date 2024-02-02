use std::{
    fs::File,
    io::{self, BufReader},
    sync::Arc,
    time::Duration,
};

use log::info;
use quinn::TransportConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};

use crate::config::{self, Congestion};

pub fn init_certs_and_key(
    config: &config::Common,
) -> io::Result<(
    rustls::RootCertStore,
    Vec<rustls::Certificate>,
    rustls::PrivateKey,
)> {
    let mut roots = rustls::RootCertStore::empty();
    for ca_cert in load_cert(&config.ca_cert_path)? {
        roots
            .add(&ca_cert)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("rustls error: {e}")))?;
    }
    let cert_chain = load_cert(&config.cert_path)?;
    let private_key = load_private_key(&config.private_key_path)?;
    Ok((roots, cert_chain, private_key))
}

pub fn load_cert(cert_path: &str) -> io::Result<Vec<rustls::Certificate>> {
    let cert_file = File::open(cert_path)?;
    let mut cert_reader = BufReader::new(cert_file);
    let cert_chain = certs(&mut cert_reader)?;
    Ok(cert_chain.into_iter().map(rustls::Certificate).collect())
}

pub fn load_private_key(key_path: &str) -> io::Result<rustls::PrivateKey> {
    let key_file = File::open(key_path)?;
    let mut key_reader = BufReader::new(key_file);
    let keys = pkcs8_private_keys(&mut key_reader)?;
    Ok(rustls::PrivateKey(
        keys.into_iter().next().expect("no private keys in file"),
    ))
}

pub fn new_transport_config(config: &config::Common) -> TransportConfig {
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(Duration::from_secs(config.connection_idle_timeout))
            .expect("Invalid idle timeout"),
    ));
    match config.congestion {
        Congestion::Cubic => {
            info!("Using congestion: cubic");
            transport
                .congestion_controller_factory(Arc::new(quinn::congestion::CubicConfig::default()));
        }
        Congestion::Bbr => {
            info!("Using congestion: bbr");
            transport
                .congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));
        }
    };
    transport
}
