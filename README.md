# OmniProxy

A self-hosted transparent proxy suite that tunnels all system traffic through encrypted WebSocket connections with stream multiplexing. Supports TCP/UDP/ICMP, with full-system TUN mode for seamless traffic routing.

## Features

- **WebSocket transport** — Tunnel traffic through Cloudflare Workers or any WebSocket endpoint
- **Stream multiplexing** — Multiple TCP/UDP/ICMP streams share a single WebSocket connection with per-stream rate tracking and three-level priority QoS (control > interactive > bulk)
- **Connection handling** — Client retries with exponential backoff and unlimited retries; outbound IP can be bound explicitly when needed
- **TUN transparent proxy** — Route all system traffic through the proxy via built-in TUN forwarder; infrastructure and service tasks are isolated so a single protocol failure doesn't tear down the whole tunnel
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

- **Linux**: `omni-proxy-linux-x86_64.tar.xz`
- **Windows**: `omni-proxy-windows-x86_64-msvc.zip`
- **macOS**: `omni-proxy-macos-x86_64.tar.xz` or `omni-proxy-macos-aarch64.tar.xz`

Each archive contains: `client`, `server`, `proxy`, `config.yml`, and `README.md`. macOS archives include `setup_macos.sh`.

**One-time setup:** Extract the archive to any directory. Edit `config.yml` once — the `client` binary is resolved relative to the `proxy` binary location, so `./client` works out of the box.

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
tun_ip: "198.18.0.1"      # TUN interface IP
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

**client**: SOCKS5 proxy server that multiplexes all streams over a persistent WebSocket connection to the server. Supports TCP, UDP, and ICMP (ping). Handles reconnection with exponential backoff. Outbound frames are prioritized: control frames (CONNECT/FIN) always go first, interactive data (SSH, DNS) is prioritized over bulk transfers (downloads, speed tests). Per-stream send rate is tracked every 16 frames and streams are dynamically reclassified between interactive and bulk queues.

**server**: WebSocket relay that demultiplexes streams and bridges to target TCP/UDP endpoints. Enforces a 4096-stream concurrency limit. Per-stream backpressure with 5s timeout prevents a slow stream from blocking the entire session.

**proxy**: Transparent proxy manager. Sets up TUN interface, routing rules, and launches the client with a built-in TUN forwarder. Infrastructure tasks (TUN read/write, netstack) are separated from service tasks (TCP/UDP/ICMP handlers) — a single protocol failure only loses that protocol, not the entire tunnel.

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

Route all system traffic through the proxy. The proxy binary sets up TUN, configures routes, and launches the client with a built-in TUN forwarder. DNS is resolved server-side — the client sends the original hostname via SOCKS5 CONNECT, not a resolved IP.

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

1. Proxy creates a TUN interface with IP `198.18.0.1/16` (IPv4) and `fd00::1/64` (IPv6)
2. Routes all system traffic through the TUN interface via split default routes (`0.0.0.0/1` + `128.0.0.0/1`)
3. Built-in forwarder reads packets from TUN, extracts the original destination, and sends to the local SOCKS5 client using hostname-based CONNECT (DNS is resolved server-side)
4. Client multiplexes traffic over WebSocket to the server

**Important:** The TUN IP ranges `198.18.0.0/16` and `fd00::/64` must not conflict with your local network.

## Requirements

- Rust 1.85+ (edition 2024)
- For TUN mode:
  - Linux: TUN/TAP support, iproute2, sudo access
  - macOS: sudo access, utun interface support (macOS 10.13+)
  - Windows: Administrator privileges, wintun.dll (included in release)
- For ICMP passthrough on the server: `CAP_NET_RAW` or root (Linux)

## License

MIT
