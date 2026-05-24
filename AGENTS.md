# SOCKS5 Proxy — Agent Notes

## Project Structure

- `client/` — SOCKS5 proxy client (Rust). Listens on `127.0.0.1:1080`, tunnels via WebSocket to remote.
- `server/` — WebSocket relay server (Rust, axum). Listens on `0.0.0.0:9880`, bridges WebSocket ↔ TCP target.

## Commands

```bash
# Build
cargo build --package client   # or --package server

# Run
cargo run --package client
cargo run --package server
```

## Architecture

**Client flow**: SOCKS5 client → local SOCKS5 (client) → WebSocket (wss://) → Server → target TCP

**First WebSocket frame**: client sends `target` as binary (e.g., `1.2.3.4:80` or `example.com:80`). Server reads it and connects to that address.

## Key Hardcoded Values

- `client/src/main.rs:15` — SOCKS5 bind: `127.0.0.1:1080`
- `client/src/main.rs:16` — WebSocket URL: `wss://your-worker.your-subdomain.workers.dev`
- `client/src/main.rs:17` — Auth token: `your-secret-token`
- `server/src/main.rs:13` — Server bind: `0.0.0.0:9880`

Update these before deployment.

## Testing

No test suite exists. Manual testing:
1. Start server: `cargo run --package server`
2. Start client: `cargo run --package client`
3. Configure browser/system SOCKS5 proxy to `127.0.0.1:1080`
4. Traffic should tunnel through WebSocket to server, then to target.