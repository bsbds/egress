#[cfg(feature = "quinn")]
pub(crate) mod quinn;
#[cfg(feature = "s2n-quic")]
pub(crate) mod s2n;

use std::net::SocketAddr;

use async_trait::async_trait;
use thiserror::Error;

use super::connection::Connection;

#[derive(Debug, Error)]
#[error("connect error: {0}")]
pub(crate) struct ConnectError(String);

#[derive(Debug, Error)]
#[error("accept error: {0}")]
pub(crate) struct AcceptError(String);

#[async_trait]
pub(crate) trait ServerEndpoint {
    async fn accept_connection(
        &mut self,
        zero_rtt: bool,
    ) -> Result<Box<dyn Connection>, AcceptError>;
}

#[async_trait]
pub(crate) trait ClientEndpoint {
    async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
        zero_rtt: bool,
    ) -> Result<Box<dyn Connection>, ConnectError>;
}
