use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use log::{info, warn};
use quinn::{Connecting, Connection as QuinnConnection, Endpoint as QuinnEndpoint};

use crate::quic::connection::Connection;

use super::{AcceptError, ClientEndpoint, ConnectError, ServerEndpoint};

#[async_trait]
impl ServerEndpoint for QuinnEndpoint {
    async fn accept_connection(
        &mut self,
        zero_rtt: Option<bool>,
    ) -> Result<Box<dyn Connection>, AcceptError> {
        let conn = self
            .accept()
            .await
            .ok_or_else(|| AcceptError("no next incoming".to_owned()))
            .map(|conn| async {
                if zero_rtt.unwrap_or(false) {
                    info!("using 0-rtt handshake");
                    Ok(conn.into_0rtt().unwrap().0)
                } else {
                    let timer = Instant::now();
                    match conn.await {
                        Ok(conn) => {
                            info!(
                                "handshake complete: {}ms, connection: {}",
                                timer.elapsed().as_millis(),
                                conn.stable_id()
                            );
                            Ok(conn)
                        }
                        Err(e) => {
                            warn!("connection failed: {}", e);
                            Err(AcceptError(e.to_string()))
                        }
                    }
                }
            })?
            .await?;

        Ok(Box::new(conn))
    }
}

#[async_trait]
impl ClientEndpoint for QuinnEndpoint {
    async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
        zero_rtt: Option<bool>,
    ) -> Result<Box<dyn Connection>, ConnectError> {
        loop {
            let connecting = self
                .connect(addr, server_name)
                .map_err(|e| ConnectError(e.to_string()))?;
            match build_connection(connecting, zero_rtt).await {
                Ok(conn) => break Ok(Box::new(conn)),
                Err(e) => {
                    warn!("connection failed: {}, sleep for 4 secs", e);
                }
            }
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
    }
}

async fn build_connection(
    connecting: Connecting,
    enable_0rtt: Option<bool>,
) -> Result<QuinnConnection, Box<dyn std::error::Error>> {
    let connection = if enable_0rtt.unwrap_or(false) {
        match connecting.into_0rtt() {
            Ok((conn, _)) => {
                info!("using 0-rtt handshake");
                conn
            }
            Err(conn) => {
                let timer = Instant::now();
                let conn = conn.await?;
                info!("handshake complete: {}ms", timer.elapsed().as_millis());
                conn
            }
        }
    } else {
        connecting.await?
    };
    Ok(connection)
}
