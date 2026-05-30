# OmniProxy

A self-hosted transparent proxy suite that tunnels all system traffic through encrypted WebSocket connections with stream multiplexing. Supports TCP/UDP/ICMP, with full-system TUN mode for seamless traffic routing.

## Features

- **WebSocket transport** — Tunnel traffic through Cloudflare Workers or any WebSocket endpoint
- **Stream multiplexing** — Multiple TCP/UDP/ICMP streams share a single WebSocket connection
- **Connection handling** — Client retries with exponential backoff and unlimited retries; outbound IP can be bound explicitly when needed
- **TUN transparent proxy** — Route all system traffic through the proxy via built-in TUN forwarder
- **ICMP passthrough** — Ping (IPv4 and IPv6) works through the proxy
- **Cross-platform** — Linux, macOS, and Windows support

## Quick Start

### 1. Deploy the server

Deploy a Cloudflare Worker using the `server/` source, or run it on any server with a public WebSocket endpoint.

Example Worker binding: the server listens on `0.0.0.0:9880` and expects a `token` header from clients.

### 2. Run the client

```bash
cargo run --manifest-path client/Cargo.toml -- --config client/config.yml
```

Or with a downloaded release:
```bash
./client --config config.yml
```

### 3. Configure your system

Set your system or browser SOCKS5 proxy to `127.0.0.1:1080`.

**Browser:** Firefox → Settings → Network Settings → Manual proxy → SOCKS5 `127.0.0.1:1080`

**macOS:** System Settings → Network → Wi-Fi → Proxies → SOCKS Proxy → `127.0.0.1:1080`

**Linux:** System network settings or environment variables:
```bash
export ALL_PROXY=socks5://127.0.0.1:1080
```

## Download Pre-built Binaries

Download the latest release from GitHub:

- **Linux**: `omni-proxy-linux-x86_64-musl.zip`
- **Windows**: `omni-proxy-windows-x86_64-msvc.zip`
- **macOS**: `omni-proxy-macos-x86_64.zip` or `omni-proxy-macos-aarch64.zip`

Each zip contains: `client`, `server`, `proxy`, `config.yml`, `README.md`, and `setup_macos.sh` (macOS only).

**One-time setup:** Extract the zip to any directory. Edit `config.yml` once — the `client` binary is resolved relative to the `proxy` binary location, so `./client` works out of the box.

**macOS:** Run `./setup_macos.sh` once to bypass Gatekeeper security restrictions (removes quarantine attributes and ad-hoc signs binaries).

## Configuration

### Client (`client/config.yml`)

```yaml
addr: 127.0.0.1
port: 1080
token: "your-secret-token"
server: "your-worker.your-subdomain.workers.dev/your-path"
```

`server` should be a WebSocket URL. The client prepends `wss://` if the scheme is omitted.
If you want the client to bind a source IP, pass `--outbound-ip`; the `proxy` binary can inject it automatically on platforms that need it.

### Server (`server/config.yml`)

```yaml
addr: 0.0.0.0
port: 9880
token: "your-secret-token"  # clients must send this token
```

### Proxy (`config.yml`)

For TUN transparent proxy mode. Place `config.yml` in the same directory as the `proxy` binary — the `client` path is resolved relative to the binary location, so `./client` works out of the box.

```yaml
# Core executables (relative to proxy binary location)
client: "./client"

# Server connection
server: "your-worker.your-subdomain.workers.dev"
token: "your-secret-token"

# Local SOCKS5 port (client listens on)
socks_port: 1080

# TUN interface settings
tun_name: "tun0"          # Linux: tun0 | macOS: utun100 | Windows: tun0
tun_ip: "198.18.0.1"      # TUN virtual IP
tun_prefix: 16            # CIDR prefix (198.18.0.0/16)

# IPv6 TUN settings
tun_ip6: "fd00::1"
tun_prefix6: 64

# Physical interface (optional — leave empty for auto-detect)
phys_iface: ""
```

## Architecture

```
Browser/App → SOCKS5 client (127.0.0.1:1080) → WebSocket → server → target
```

**client**: SOCKS5 proxy server that multiplexes all streams over a persistent WebSocket connection to the server. Supports TCP, UDP, and ICMP (ping). Handles reconnection with exponential backoff.

**server**: WebSocket relay that demultiplexes streams and bridges to target TCP/UDP endpoints. Enforces a 4096-stream concurrency limit.

**proxy**: Transparent proxy manager. Sets up TUN interface, routing rules, and launches the client with a built-in TUN forwarder.

## Protocol

Custom framing on top of WebSocket binary messages:

| Type | Name | Direction | Description |
|------|------|----------|-------------|
| 0x01 | TCP_CONNECT | C→S | New TCP stream |
| 0x02 | TCP_CONNECTED | S→C | Connection success/failure |
| 0x03 | TCP_DATA | both | Payload data |
| 0x04 | TCP_FIN | C→S | Stream closed |
| 0x05 | UDP_DATA | C→S | UDP packet |
| 0x06 | ICMP_DATA | C→S | ICMP echo request/response |

UDP payload: `[2B host_len][host bytes][2B port][data]`

ICMP payload: `[2B ip_len][ip string][icmp_data]`

## TUN Mode

Route all system traffic through the proxy. The proxy binary sets up TUN, configures routes, and launches client with a built-in TUN forwarder.

### Linux/macOS

```bash
sudo ./proxy --config ./config.yml
```

### Windows (Driver by [Wintun](https://www.wintun.net/))

Run with administrator privileges:

```powershell
.\proxy.exe --config .\config.yml
```

### How it works

1. Proxy creates a TUN interface with fake-IP ranges `198.18.0.0/16` (IPv4) and `fd00::/64` (IPv6)
2. Routes traffic destined to fake-IPs through the TUN interface
3. Built-in forwarder reads from TUN and sends to the local SOCKS5 client
4. Client multiplexes traffic over WebSocket to the server

**Important:** The fake-IP ranges `198.18.0.0/16` and `fd00::/64` must not overlap with your real network.

## Requirements

- Rust 1.85+ (edition 2024)
- For TUN mode:
  - Linux: TUN/TAP support, iproute2, sudo access
  - macOS: sudo access, utun interface support (macOS 10.13+)
  - Windows: Administrator privileges, wintun.dll (included in release)
- For ICMP passthrough on the server: `CAP_NET_RAW` or root (Linux)

## License

MIT
