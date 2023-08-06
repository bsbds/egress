Egress
====

Egress is a lightweight and UDP/TCP tunnel over QUIC that leverages the QUIC Datagram extension.

Features
--------

* Support for both UDP and TCP tunneling
* Utilizes the QUIC Datagram extension for minimized latency
* Support for concurrent socket peers by multiplexing QUIC datagrams

Table of Contents
-----------------

1. [Getting Started](#getting-started)
   * [Prerequisites](#prerequisites)
   * [Installation](#installation)
2. [Usage](#usage)
   * [Configuration](#configuration)
   * [Example](#example)

Getting Started
---------------

### Prerequisites

Before you begin, ensure you have the following software installed on your system:

* [Rust stable](https://www.rust-lang.org/tools/install)
* [Git](https://git-scm.com/downloads)

### Installation

Clone the Egress repository:

```bash
git clone https://github.com/bsbds/egress.git
```

Navigate to the project directory:

```bash
cd egress
```

Build the project:

```bash
cargo build --release
```

Usage
-----

### Configuration

Generate a preshared key

```bash
openssl rand 32 | base64
```

Create a configuration file, `config.toml`, with the following structure:

```toml
mode = "server" # or "client"
psk = "PSK" # preshared key you generated in base64 encoding
congestion = "bbr" # congestion of quic connection, "cubic", "new_reno" or "bbr"
initial_mtu = 1452 # 1500 bytes ethernet frame - 40 bytes ipv6 header - 8 bytes UDP header, modify this for your usecase
enable_0rtt = false # whether to enable 0rtt handshake
loglevel = "warning" # rust's log level
stream_idle_timeout = 30 # timeout for multiplexed stream
connection_idle_timeout = 60 # timeout for one quic connection

# server specific
[server]
quic_listen = "[::]:443" # server listen address
self_sign = false # whether use self signed certificates
certificate = "path/to/certificate"
private_key = "path/to/private_key"

# client specific
[client]
server_name = "example.com" # server name
server_addr = "192.168.1.1:443" # server address, can be a domain
listen = "127.0.0.1:1234" # local listen address
peer = "127.0.0.1:1234" # remote address to connect to
cert_ver = true # whether to verify server certificates
network = "udp" # network mode, or "tcp"
```

### Example

To run Egress:

```bash
./egress --config config.toml
```
