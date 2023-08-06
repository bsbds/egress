mod server_tcp;
mod server_udp;

use crate::{
    common::util,
    config::{self, Congestion, NetworkType},
    quic::*,
};
use log::{info, warn};
use quinn::{self, Endpoint};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::{error::Error, fs::File, io::BufReader, net::SocketAddr, sync::Arc};
use tokio::time::Instant;
use {server_tcp::*, server_udp::*};

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

fn build_endpoint(
    quic_listen: SocketAddr,
    certificate: Option<String>,
    private_key: Option<String>,
    congestion: Congestion,
    initial_mtu: u16,
    self_sign: bool,
    conn_idle_timeout: u64,
) -> Result<Endpoint, Box<dyn Error>> {
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

    let transport = util::new_transport(initial_mtu, congestion, conn_idle_timeout);
    let mut server_config = quinn::ServerConfig::with_single_cert(cert_chain, key)?;
    server_config.transport_config(Arc::new(transport));

    let endpoint = Endpoint::server(server_config, quic_listen)?;

    Ok(endpoint)
}

pub async fn server(config: config::Config) {
    let server_config = config.server.expect("server field missing");
    let psk = Arc::new(util::decode_psk(&config.psk).expect("decode psk failed"));
    let endpoint = build_endpoint(
        server_config.quic_listen,
        server_config.certificate,
        server_config.private_key,
        config.congestion,
        config.initial_mtu,
        server_config.self_sign,
        config.connection_idle_timeout,
    )
    .expect("build endpoint failed");

    info!("server listening on: {}", server_config.quic_listen);

    while let Some(conn) = endpoint.accept().await {
        let psk = psk.clone();
        tokio::spawn(async move {
            let connection = if config.enable_0rtt {
                info!("using 0-rtt handshake");
                conn.into_0rtt().unwrap().0
            } else {
                let timer = Instant::now();
                match conn.await {
                    Ok(conn) => {
                        info!(
                            "handshake complete: {}ms, connection: {}",
                            timer.elapsed().as_millis(),
                            conn.stable_id()
                        );
                        conn
                    }
                    Err(e) => {
                        warn!("connection failed: {}", e);
                        return;
                    }
                }
            };
            let conn_id = connection.stable_id();
            info!("server: new connection, connection id: {}", conn_id);

            if let Ok(handle) = QuicConnection::new(connection, psk, config.stream_idle_timeout)
                .await
                .into_server()
                .await
            {
                loop {
                    let stream = handle.accept_stream().await;
                    tokio::spawn(async move {
                        info!("new stream, id: {}", stream.id());

                        let stream_type = stream.stream_type();
                        let peer_addr = stream.peer_addr();

                        match stream_type {
                            NetworkType::Udp => {
                                info!("network type: udp");
                                spawn_udp_sockets(stream, peer_addr).await;
                            }
                            NetworkType::Tcp => {
                                info!("network type: tcp");
                                spawn_tcp_sockets(stream, peer_addr).await;
                            }
                        }
                    });
                }
            }
        });
    }
}
