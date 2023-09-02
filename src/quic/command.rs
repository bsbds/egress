use super::{
    connection::{RecvStream, SendStream},
    CommandError, StreamId,
};
use bytes::BytesMut;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone)]
pub enum ConnectionCommand {
    Auth { psk: Arc<Vec<u8>> },
    Close { stream_id: StreamId },
    OpenUdp { id: StreamId, peer_addr: SocketAddr },
    OpenTcp { id: StreamId, peer_addr: SocketAddr },
}

#[derive(Copy, Clone, Debug, Error, PartialEq, Eq)]
#[error("command decode failed")]
pub struct DecodeCommandError;

impl TryFrom<Vec<u8>> for ConnectionCommand {
    type Error = DecodeCommandError;
    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(DecodeCommandError);
        }
        let c = value[0];
        let data = value[1..].to_vec();
        match c {
            0x00 => {
                if data.len() != 32 {
                    return Err(DecodeCommandError);
                }
                Ok(Self::Auth {
                    psk: Arc::new(data),
                })
            }
            0x01 => {
                if data.len() != 2 {
                    return Err(DecodeCommandError);
                }
                Ok(Self::Close {
                    stream_id: StreamId::from_be_bytes(data.try_into().unwrap()),
                })
            }
            0x02 | 0x03 => {
                if data.len() < 5 {
                    return Err(DecodeCommandError);
                }
                let stream_id = StreamId::from_be_bytes(data[..2].try_into().unwrap());
                let port = u16::from_be_bytes(data[2..4].try_into().unwrap());
                let ip_ver = data[4];
                let peer_addr = match ip_ver {
                    0x00 => {
                        // ipv4
                        let ip: [u8; 4] = match data[5..].try_into() {
                            Err(_) => {
                                return Err(DecodeCommandError);
                            }
                            Ok(ip) => ip,
                        };
                        SocketAddr::new(IpAddr::from(ip), port)
                    }
                    0x01 => {
                        // ipv6
                        let ip: [u8; 16] = match data[5..].try_into() {
                            Err(_) => {
                                return Err(DecodeCommandError);
                            }
                            Ok(ip) => ip,
                        };
                        SocketAddr::new(IpAddr::from(ip), port)
                    }
                    _ => {
                        return Err(DecodeCommandError);
                    }
                };
                match c {
                    0x02 => Ok(Self::OpenUdp {
                        id: stream_id,
                        peer_addr,
                    }),
                    0x03 => Ok(Self::OpenTcp {
                        id: stream_id,
                        peer_addr,
                    }),
                    _ => unreachable!(),
                }
            }
            _ => Err(DecodeCommandError),
        }
    }
}

impl ConnectionCommand {
    pub fn to_be_bytes(&self) -> Vec<u8> {
        match self {
            Self::Auth { psk } => {
                let mut bytes = vec![0x00];
                bytes.extend_from_slice(psk);
                bytes
            }
            Self::Close { stream_id } => {
                let mut bytes = vec![0x01];
                bytes.extend_from_slice(&stream_id.to_be_bytes());
                bytes
            }
            Self::OpenUdp { id, peer_addr } | Self::OpenTcp { id, peer_addr } => {
                let mut bytes = match self {
                    Self::OpenUdp { .. } => vec![0x02],
                    Self::OpenTcp { .. } => vec![0x03],
                    _ => unreachable!(),
                };
                let port = peer_addr.port();
                let ip = peer_addr.ip();
                bytes.extend_from_slice(&id.to_be_bytes());
                bytes.extend_from_slice(&port.to_be_bytes());
                match ip {
                    IpAddr::V4(ip) => {
                        bytes.extend_from_slice(&[0x00]);
                        bytes.extend_from_slice(&ip.octets())
                    }
                    IpAddr::V6(ip) => {
                        bytes.extend_from_slice(&[0x01]);
                        bytes.extend_from_slice(&ip.octets())
                    }
                };
                bytes
            }
        }
    }

    pub async fn write_to_stream(&self, stream: &mut dyn SendStream) -> Result<(), CommandError> {
        let mut bytes = BytesMut::new();
        let encoded = self.to_be_bytes();
        let length = encoded.len() as u8;
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(&encoded);
        stream.write_all(&bytes.freeze()).await?;
        Ok(())
    }
}

pub async fn read_command(stream: &mut dyn RecvStream) -> Result<ConnectionCommand, CommandError> {
    let mut length_buf = [0; 1];
    stream.read_exact(&mut length_buf).await?;
    let length = u8::from_be_bytes(length_buf) as usize;
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes.try_into()?)
}
