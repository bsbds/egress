use crate::{common::util, config::Congestion, quic::endpoint::ServerEndpoint};
use log::warn;
use quinn::{self, Endpoint};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::{error::Error, fs::File, io::BufReader, net::SocketAddr, sync::Arc};

fn parse_cert_and_key(
    cert_path: &str,
    key_path: &str,
) -> Result<(Vec<rustls::Certificate>, rustls::PrivateKey), Box<dyn Error>> {
    let cert_file = File::open(cert_path)?;
    let key_file = File::open(key_path)?;
    let mut cert_reader = BufReader::new(cert_file);
    let mut key_reader = BufReader::new(key_file);
    let cert_chain = certs(&mut cert_reader)?;
    let keys = pkcs8_private_keys(&mut key_reader)?;
    Ok((
        cert_chain.into_iter().map(rustls::Certificate).collect(),
        rustls::PrivateKey(keys.into_iter().next().expect("no private keys in file")),
    ))
}

fn generate_self_signed_cert() -> Result<(rustls::Certificate, rustls::PrivateKey), Box<dyn Error>>
{
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
    let key = rustls::PrivateKey(cert.serialize_private_key_der());
    Ok((rustls::Certificate(cert.serialize_der()?), key))
}

pub(super) fn build_endpoint(
    quic_listen: SocketAddr,
    certificate: Option<String>,
    private_key: Option<String>,
    congestion: Congestion,
    initial_mtu: u16,
    self_sign: bool,
    conn_idle_timeout: u64,
) -> Result<Box<dyn ServerEndpoint>, Box<dyn Error>> {
    let (cert_chain, key) = if self_sign {
        warn!("using self signed cert");
        let (cert, key) = generate_self_signed_cert()?;
        (vec![cert], key)
    } else {
        parse_cert_and_key(
            &certificate.expect("certificate field missing"),
            &private_key.expect("private key field missing"),
        )?
    };

    let transport = util::quinn::new_transport(initial_mtu, congestion, conn_idle_timeout);
    let mut server_config = quinn::ServerConfig::with_single_cert(cert_chain, key)?;
    server_config.transport_config(Arc::new(transport));

    let endpoint = Endpoint::server(server_config, quic_listen)?;

    Ok(Box::new(endpoint))
}
