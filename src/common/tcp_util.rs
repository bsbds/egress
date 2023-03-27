use super::constant::TCP_READ_BUF_SIZE;
use crate::quic::{ConnectedState, DatagramStream};
use bytes::BytesMut;
use log::info;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub fn tcp_transport<C>(mut tcp_stream: TcpStream, datagram_stream: DatagramStream<C>)
where
    C: ConnectedState + Clone + Send + Sync + 'static,
{
    tokio::spawn(async move {
        let (mut tcp_read, mut tcp_write) = tcp_stream.split();

        loop {
            let mut buf = BytesMut::with_capacity(TCP_READ_BUF_SIZE);
            tokio::select! {
                res = tcp_read.read_buf(&mut buf) => {
                    match res {
                        Err(e) => {
                            info!("stream read failed: {}", e);
                            break;
                        }
                        Ok(n) => {
                            if n == 0 {
                                info!("tcp closed");
                                break;
                            }
                            datagram_stream.send(buf.freeze());
                        }
                    }
                }
                res = datagram_stream.recv() => {
                    match res {
                        Err(e) => {
                            info!("channel closed: {}", e);
                            break;
                        }
                        Ok(bytes) => {
                            if let Err(e) = tcp_write.write_all(&bytes).await {
                                info!("tcp stream write failed: {}", e);
                                break;
                            }
                        }
                    }
                }
                err = datagram_stream.closed() => {
                    let _ = tcp_stream.shutdown().await;
                    info!("quic stream closed: {}", err);
                    break;
                }
            }
        }
    });
}
