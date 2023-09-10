use crate::{
    common::constant::{DATAGRAM_RECV_CAPACITY, DATAGRAM_SEND_CAPACITY},
    config::Congestion,
    quic::{connection::s2n::ConnectionSubscriber, endpoint::ClientEndpoint},
};
use log::warn;
use s2n_quic::{
    client::{Builder, ClientProviders},
    provider::congestion_controller,
    Client as S2nClient,
};
use std::{net::SocketAddr, path::Path};
use tokio::time::Duration;

fn build_congestion(
    client: Builder<impl ClientProviders>,
    congestion: Congestion,
) -> Result<S2nClient, Box<dyn std::error::Error>> {
    Ok(match congestion {
        Congestion::Cubic => client
            .with_congestion_controller(congestion_controller::Cubic::default())?
            .start()?,
        Congestion::NewReno => {
            warn!("new reno congestion controller is not supported by s2n-quic, using cubic");
            client
                .with_congestion_controller(congestion_controller::Cubic::default())?
                .start()?
        }
        Congestion::Bbr => client
            .with_congestion_controller(congestion_controller::Bbr::default())?
            .start()?,
    })
}

pub(super) fn build_endpoint(
    _cert_ver: bool,
    cert_path: Option<String>,
    congestion: Congestion,
    server_addr: SocketAddr,
    conn_idle_timeout: u64,
) -> Result<Box<dyn ClientEndpoint>, Box<dyn std::error::Error>> {
    let addr: SocketAddr = match server_addr {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    }
    .parse()
    .unwrap();

    let limits = s2n_quic::provider::limits::Limits::default()
        .with_max_idle_timeout(Duration::from_secs(conn_idle_timeout))?;

    let datagram = s2n_quic::provider::datagram::default::Endpoint::builder()
        .with_recv_capacity(DATAGRAM_RECV_CAPACITY)?
        .with_send_capacity(DATAGRAM_SEND_CAPACITY)?
        .build()
        .unwrap();

    let client = S2nClient::builder()
        .with_io(addr)?
        .with_limits(limits)?
        .with_datagram(datagram)?
        .with_event(ConnectionSubscriber {})?;

    let client = if let Some(path) = cert_path {
        build_congestion(client.with_tls(Path::new(&path))?, congestion)?
    } else {
        build_congestion(client, congestion)?
    };

    Ok(Box::new(client))
}
