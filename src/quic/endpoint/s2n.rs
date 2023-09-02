use std::net::SocketAddr;

use async_trait::async_trait;
use s2n_quic::{client::Connect as ConnectConfig, Client as S2nClient, Server as S2nServer};

use crate::quic::connection::Connection;

use super::{AcceptError, ClientEndpoint, ConnectError, ServerEndpoint};

#[async_trait]
impl ServerEndpoint for S2nServer {
    async fn accept_connection(
        &mut self,
        _zero_rtt: bool,
    ) -> Result<Box<dyn Connection>, AcceptError> {
        Ok(Box::new(self.accept().await.ok_or_else(|| {
            AcceptError("no next incoming".to_owned())
        })?))
    }
}

#[async_trait]
impl ClientEndpoint for S2nClient {
    async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
        _zero_rtt: bool,
    ) -> Result<Box<dyn Connection>, ConnectError> {
        let config = ConnectConfig::new(addr).with_server_name(server_name);
        Ok(Box::new(
            self.connect(config)
                .await
                .map_err(|e| ConnectError(e.to_string()))?,
        ))
    }
}
