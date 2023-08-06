use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use log::{info, warn};
use quinn::{Connecting, Connection as QuinnConnection, Endpoint as QuinnEndpoint};
use s2n_quic::{
    client::Connect as ConnectConfig, Client as S2nClient, Connection as S2nConnection,
    Server as S2nServer,
};
use thiserror::Error;

use super::connection::Connection;

#[derive(Debug, Error)]
#[error("connect error: {0}")]
pub(crate) struct ConnectError(String);

#[derive(Debug, Error)]
#[error("accept error: {0}")]
pub(crate) struct AcceptError(String);

#[async_trait]
pub(crate) trait ServerEndpoint<C: Connection> {
    async fn accept_connection(&mut self, zero_rtt: bool) -> Result<C, AcceptError>;
}

#[async_trait]
pub(crate) trait ClientEndpoint<C: Connection> {
    async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
        zero_rtt: bool,
    ) -> Result<C, ConnectError>;
}

#[async_trait]
impl ServerEndpoint<QuinnConnection> for QuinnEndpoint {
    async fn accept_connection(&mut self, zero_rtt: bool) -> Result<QuinnConnection, AcceptError> {
        self.accept()
            .await
            .ok_or_else(|| AcceptError("no next incoming".to_owned()))
            .map(|conn| async {
                if zero_rtt {
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
            .await
    }
}

#[async_trait]
impl ClientEndpoint<QuinnConnection> for QuinnEndpoint {
    async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
        zero_rtt: bool,
    ) -> Result<QuinnConnection, ConnectError> {
        loop {
            let connecting = self
                .connect(addr, server_name)
                .map_err(|e| ConnectError(e.to_string()))?;
            match build_connection(connecting, zero_rtt).await {
                Ok(conn) => break Ok(conn),
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
    enable_0rtt: bool,
) -> Result<QuinnConnection, Box<dyn std::error::Error>> {
    let connection = if enable_0rtt {
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

#[async_trait]
impl ServerEndpoint<S2nConnection> for S2nServer {
    async fn accept_connection(&mut self, _zero_rtt: bool) -> Result<S2nConnection, AcceptError> {
        self.accept()
            .await
            .ok_or_else(|| AcceptError("no next incoming".to_owned()))
    }
}

#[async_trait]
impl ClientEndpoint<S2nConnection> for S2nClient {
    async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
        _zero_rtt: bool,
    ) -> Result<S2nConnection, ConnectError> {
        let config = ConnectConfig::new(addr).with_server_name(server_name);
        self.connect(config)
            .await
            .map_err(|e| ConnectError(e.to_string()))
    }
}
