use super::constant::PSK_SIZE;
use crate::config::Congestion;
use base64::{engine::general_purpose, Engine as _};
use log::info;
use quinn::TransportConfig;
use std::{sync::Arc, time::Duration};

pub fn decode_psk(psk_encoded: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let psk = general_purpose::STANDARD.decode(psk_encoded)?;
    if psk.len() != PSK_SIZE {
        return Err("wrong length of psk string")?;
    }
    Ok(psk)
}

pub fn new_transport(
    initial_mtu: u16,
    congestion: Congestion,
    idle_timeout: u64,
) -> TransportConfig {
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(Duration::from_secs(idle_timeout)).expect("invalid timeout"),
    ));
    transport.initial_mtu(initial_mtu);
    match congestion {
        Congestion::Cubic => {
            info!("using congestion: cubic");
            transport
                .congestion_controller_factory(Arc::new(quinn::congestion::CubicConfig::default()));
        }
        Congestion::NewReno => {
            info!("using congestion: new_reno");
            transport.congestion_controller_factory(Arc::new(
                quinn::congestion::NewRenoConfig::default(),
            ));
        }
        Congestion::Bbr => {
            info!("using congestion: bbr");
            transport
                .congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));
        }
    };
    transport
}
