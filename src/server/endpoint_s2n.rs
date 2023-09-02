use crate::{
    common::constant::{DATAGRAM_RECV_CAPACITY, DATAGRAM_SEND_CAPACITY},
    config::Congestion,
    quic::endpoint::ServerEndpoint,
};
use log::warn;
use s2n_quic::{provider::congestion_controller, Server as S2nServer};
use std::{error::Error, net::SocketAddr};

pub(super) fn build_endpoint(
    quic_listen: SocketAddr,
    certificate: Option<String>,
    private_key: Option<String>,
    congestion: Congestion,
    _initial_mtu: u16,
    _self_sign: bool,
    conn_idle_timeout: u64,
) -> Result<Box<dyn ServerEndpoint>, Box<dyn Error>> {
    use std::path::PathBuf;
    let (cert, key) = (
        PathBuf::from(certificate.expect("certificate field missing")),
        PathBuf::from(private_key.expect("private key field missing")),
    );
    let limits = s2n_quic::provider::limits::Limits::default()
        .with_max_idle_timeout(std::time::Duration::from_secs(conn_idle_timeout))?;

    let datagram = s2n_quic::provider::datagram::default::Endpoint::builder()
        .with_recv_capacity(DATAGRAM_RECV_CAPACITY)?
        .with_send_capacity(DATAGRAM_SEND_CAPACITY)?
        .build()
        .unwrap();

    let server = S2nServer::builder()
        .with_tls((cert.as_path(), key.as_path()))?
        .with_io(quic_listen)?
        .with_datagram(datagram)?
        .with_limits(limits)?;

    let server = match congestion {
        Congestion::Cubic => server
            .with_congestion_controller(congestion_controller::Cubic::default())?
            .start()?,
        Congestion::NewReno => {
            warn!("new reno congestion controller is not supported by s2n-quic, using cubic");
            server
                .with_congestion_controller(congestion_controller::Cubic::default())?
                .start()?
        }
        Congestion::Bbr => server
            .with_congestion_controller(congestion_controller::Bbr::default())?
            .start()?,
    };

    Ok(Box::new(server))
}
