# socks5-proxy

Transparent SOCKS5 proxy tunneling traffic via WebSocket. Supports TCP and UDP with a multiplexing protocol on top of WebSocket.

## Architecture

```
Client (SOCKS5) → client → WebSocket → server → target TCP/UDP
```

**client**: Listens for SOCKS5 connections, multiplexes all streams over a single WebSocket connection to the server. Auto-reconnects on disconnect.

**server**: Accepts WebSocket connections, demultiplexes streams, connects to target TCP/UDP endpoints, and relays traffic back.

## Quick Start

**Server**:
```bash
cargo run --manifest-path server/Cargo.toml -- --config server/config.yml
```

**Client**:
```bash
cargo run --manifest-path client/Cargo.toml -- --config client/config.yml
```

## WebSocket Multiplexing Protocol

Frame format: `[4B stream_id][1B type][payload]`

| Type | Name | Direction | Description |
|------|------|----------|-------------|
| 0x01 | TCP_CONNECT | C→S | New TCP stream to target |
| 0x02 | TCP_CONNECTED | S→C | TCP connection established |
| 0x03 | TCP_DATA | both | TCP payload data |
| 0x04 | TCP_FIN | C→S | TCP stream closed |
| 0x05 | UDP_DATA | C→S | UDP packet (stream_id=0) |

UDP payload: `[2B host_len][host][2B port][data]`

## Configuration

### Server (`server/config.yml`)
```yaml
addr: 0.0.0.0
port: 9880
token: "your-secret-token"  # X-Proxy-Token header
```

### Client (`client/config.yml`)
```yaml
addr: 127.0.0.1
port: 1080
token: "your-secret-token"
server: "tunnel-oracle.022025.xyz"  # auto-prepends wss://
```

## TUN Mode (Transparent Proxy)

For full-system proxy, use `proxy.sh` with tun2socks:

```bash
sudo ./scripts/proxy.sh -c ./client -t ./tun2socks -- --server example.com --token secret
```

This sets up:
- Routing isolation (client user bypasses TUN to avoid routing loops)
- TUN interface with fake-ip range (198.18.0.0/16)
- tun2socks forwarding all traffic through the local SOCKS5 client

## Build Release

```bash
cargo build --release --manifest-path client/Cargo.toml
cargo build --release --manifest-path server/Cargo.toml
```

## GitHub Release

```bash
gh release delete beta0.1 --repo kurashizu/socks5-proxy --yes
gh release create beta0.1 --title "beta0.1" --notes "..." --repo kurashizu/socks5-proxy
gh release upload beta0.1 client/target/release/client server/target/release/server --repo kurashizu/socks5-proxy
```