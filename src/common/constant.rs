pub const PSK_SIZE: usize = 32;

pub const UDP_RECV_BUF_SIZE: usize = 65535;

pub const TCP_READ_BUF_SIZE: usize = 1200;

pub const FLUME_CHANNEL_SIZE: usize = 65535;

pub const STREAM_QUEUE_SIZE: usize = 16384;

pub const MAX_PENDING_DATA_SIZE: usize = 65535;

pub const MAX_PENDING_STREAM: usize = 1024;

#[cfg(feature = "s2n-quic")]
pub(crate) const DATAGRAM_SEND_CAPACITY: usize = 65535;
#[cfg(feature = "s2n-quic")]
pub(crate) const DATAGRAM_RECV_CAPACITY: usize = 65535;
