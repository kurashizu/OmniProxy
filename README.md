# socks5-proxy

SOCKS5 proxy with WebSocket transport and stream multiplexing. Supports TCP and UDP.

## Features

- **WebSocket transport** — Tunnel traffic through Cloudflare Workers or any WebSocket endpoint
- **Stream multiplexing** — Multiple TCP/UDP streams share a single WebSocket connection
- **Auto-reconnect** — Client automatically reconnects on connection failure
- **TUN transparent proxy** — Route all system traffic through the proxy via tun2socks

## Quick Start

### 1. Run the server

```bash
cargo run --manifest-path server/Cargo.toml -- --config server/config.yml
```

### 2. Run the client

```bash
cargo run --manifest-path client/Cargo.toml -- --config client/config.yml
```

### 3. Configure your browser or app

Point your SOCKS5 proxy to `127.0.0.1:1080`.

## Configuration

### Server (`server/config.yml`)

```yaml
addr: 0.0.0.0
port: 9880
token: "your-secret-token"  # clients must send X-Proxy-Token header
```

### Client (`client/config.yml`)

```yaml
addr: 127.0.0.1
port: 1080
token: "your-secret-token"
server: "your-worker.your-subdomain.workers.dev"  # auto-prepends wss://
```

## Architecture

```
Browser/App → SOCKS5 client (127.0.0.1:1080) → WebSocket → server → target
```

**client**: SOCKS5 server that multiplexes all streams over a persistent WebSocket connection to the server. Reconnects automatically.

**server**: WebSocket relay that demultiplexes streams and bridges to target TCP/UDP endpoints.

## Protocol

Custom framing on top of WebSocket binary messages:

| Type | Name | Direction | Description |
|------|------|----------|-------------|
| 0x01 | TCP_CONNECT | C→S | New TCP stream |
| 0x02 | TCP_CONNECTED | S→C | Connection success/failure |
| 0x03 | TCP_DATA | both | Payload data |
| 0x04 | TCP_FIN | C→S | Stream closed |
| 0x05 | UDP_DATA | C→S | UDP packet |

UDP payload: `[2B host_len][host][2B port][data]`

## TUN Mode

For full-system transparent proxy, use `proxy.sh` with [tun2socks](https://github.com/xjasonlyu/tun2socks):

```bash
sudo ./scripts/proxy.sh -c ./client -t ./tun2socks -- --server your-worker.workers.dev --token secret
```

Options:
- `-c` — path to client binary (default: `./client`)
- `-t` — path to tun2socks binary (default: `./tun2socks`)
- `--` — arguments passed to client

## Requirements

- Rust 1.75+
- For TUN mode: Linux with TUN/TAP support, iproute2, sudo access

## License

MIT
