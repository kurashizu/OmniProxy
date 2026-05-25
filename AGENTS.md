# SOCKS5 Proxy — Agent Notes

## Project Structure

- `client/` — SOCKS5 proxy client (Rust). Listens on `127.0.0.1:1080`, multiplexes streams over WebSocket.
- `server/` — WebSocket relay server (Rust, axum). Demultiplexes streams, bridges to TCP/UDP.
- `scripts/` — Shell scripts for TUN-based transparent proxy setup.

No workspace `Cargo.toml` — each package is independent.

## Commands

```bash
cargo build --release --manifest-path client/Cargo.toml
cargo build --release --manifest-path server/Cargo.toml

cargo run --manifest-path client/Cargo.toml -- --config client/config.yml
cargo run --manifest-path server/Cargo.toml -- --config server/config.yml
```

## WebSocket Multiplexing Protocol

**Frame format**: `[4B stream_id][1B type][payload]`

Types:
- `0x01` TCP_CONNECT (C→S): new TCP stream
- `0x02` TCP_CONNECTED (S→C): connection success/failure
- `0x03` TCP_DATA: payload data
- `0x04` TCP_FIN (C→S): stream closed
- `0x05` UDP_DATA (C→S): UDP packet (stream_id=0)

UDP payload: `[2B host_len][host bytes][2B port][data]`

## Key Implementation Details

- Client auto-reconnects to server on disconnect (3s retry, then 5s on failure)
- Server does NOT auto-reconnect to client
- Client raises `RLIMIT_NOFILE` to 65535 on startup
- Server uses `DashMap` for stream state, client uses `RwLock<HashMap>`
- UDP relay on server shares a single `UdpSocket` across all streams

## Scripts (`scripts/`)

| Script | Usage | Description |
|--------|-------|-------------|
| `proxy.sh` | `sudo ./proxy.sh [OPTIONS] -- [CLIENT_ARGS]` | One-shot launcher: TUN + routing isolation + client + tun2socks |
| `setup_tun.sh` | `sudo ./setup_tun.sh` | Configures tun0 with fake-ip range (198.18.0.0/16) |
| `run_client.sh` | `sudo ./run_client.sh [-- CLIENT_ARGS]` | Runs client with uid-based route isolation |
| `run_tun2socks.sh` | `./run_tun2socks.sh [TUN2SOCKS_PATH]` | Starts tun2socks connecting to local SOCKS5 |

## Release Workflow

```bash
cargo build --release --manifest-path client/Cargo.toml
cargo build --release --manifest-path server/Cargo.toml

gh release delete beta0.1 --repo kurashizu/socks5-proxy --yes
gh release create beta0.1 --title "beta0.1" --notes "..." --repo kurashizu/socks5-proxy
gh release upload beta0.1 client/target/release/client server/target/release/server --repo kurashizu/socks5-proxy
```

## Key Files

- `client/src/main.rs` — SOCKS5 server, Mux with auto-reconnect, TCP/UDP handling
- `server/src/main.rs` — WebSocket accept, demux, TCP streams, UDP relay
- `scripts/proxy.sh` — Full TUN proxy launcher with routing isolation