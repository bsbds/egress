#[cfg(feature = "quinn")]
mod endpoint_quinn;
#[cfg(feature = "s2n-quic")]
mod endpoint_s2n;
mod server_tcp;
mod server_udp;

use crate::{
    common::util,
    config::{self, NetworkType},
    quic::*,
};
#[cfg(feature = "quinn")]
use endpoint_quinn::build_endpoint;
#[cfg(feature = "s2n-quic")]
use endpoint_s2n::build_endpoint;
use log::info;

use std::sync::Arc;
use {server_tcp::*, server_udp::*};

pub async fn server(config: config::Config) {
    let server_config = config.server.expect("server field missing");
    let psk = Arc::new(util::decode_psk(&config.psk).expect("decode psk failed"));
    let mut endpoint = build_endpoint(
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

    while let Ok(connection) = endpoint.accept_connection(config.enable_0rtt).await {
        let psk = psk.clone();
        tokio::spawn(async move {
            let conn_id = connection.id();
            info!("server: new connection, connection id: {}", conn_id);

            if let Ok(handle) = ConnectionBuilder::new(connection, psk, config.stream_idle_timeout)
                .build_server()
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
