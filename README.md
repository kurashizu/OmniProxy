# SOCKS5 Proxy over WebSocket

SOCKS5 proxy client-server pair tunneling traffic via WebSocket. Supports TCP and UDP.

## Quick Start

**Server** (accepts WebSocket connections):
```bash
cargo run --package server -- --config server/config.yml
```

**Client** (runs local SOCKS5 proxy):
```bash
cargo run --package client -- --config client/config.yml
```

## Configuration

### Server (`server/config.yml`)
```yaml
addr: 0.0.0.0
port: 9880
token: "your-secret-token"  # clients must send X-Proxy-Token
```

### Client (`client/config.yml`)
```yaml
addr: 127.0.0.1
port: 1080
token: "your-secret-token"
server: "tunnel-oracle.022025.xyz"  # auto-prepends wss://
```

## Usage

Configure your browser or application to use SOCKS5 proxy at `127.0.0.1:1080`.

## Protocol

1. Client connects to server via WebSocket
2. Client sends first frame: `T` + target for TCP, `U` for UDP
3. Server connects to target and relays traffic
4. UDP data is fragmented into ≤60KB chunks with custom framing