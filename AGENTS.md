# SOCKS5 Proxy — Agent Notes

## Project Structure

- `client/` — SOCKS5 proxy client (Rust). Supports TCP and UDP, tunnels via WebSocket.
- `server/` — WebSocket relay server (Rust, axum). Bridges WebSocket ↔ TCP/UDP target.

Both packages have CLI args and YAML config (`--config`).

## Commands

No workspace `Cargo.toml` — use `--manifest-path` for each package:

```bash
cargo build --manifest-path client/Cargo.toml
cargo build --manifest-path server/Cargo.toml

cargo run --manifest-path client/Cargo.toml -- --server tunnel-oracle.022025.xyz --config client/config.yml
cargo run --manifest-path server/Cargo.toml -- --config server/config.yml
```

## Configuration

**Client** (`client/config.yml`):
```yaml
addr: 127.0.0.1
port: 1080
token: "your-secret-token"
server: "tunnel-oracle.022025.xyz"  # auto-prepends wss://
```

**Server** (`server/config.yml`):
```yaml
addr: 0.0.0.0
port: 9880
token: "your-secret-token"
```

Auth: client sends `X-Proxy-Token` header, server validates it.

## WebSocket Protocol

**First frame (control)**:
- TCP: `b'T' + "host:port"` (e.g., `Texample.com:80`)
- UDP: `b'U'`

**Subsequent frames**: raw bytes. UDP uses custom fragmentation (≤60KB chunks).

## Architecture

Client flow: SOCKS5 client → local SOCKS5 → WebSocket → Server → target TCP/UDP

## Testing

No test suite exists. Manual testing with mihomo.yaml (SOCKS5 config for reference).

## Release Workflow

```bash
# Build release binaries
cargo build --release --manifest-path client/Cargo.toml
cargo build --release --manifest-path server/Cargo.toml

# GitHub release: delete old, create new, upload assets
gh release delete beta0.1 --repo kurashizu/socks5-proxy --yes
gh release create beta0.1 --title "beta0.1" --notes "..." --repo kurashizu/socks5-proxy
gh release upload beta0.1 client/target/release/client server/target/release/server --repo kurashizu/socks5-proxy
```

- `client/.cargo/config.toml` enables Windows cross-compile via MinGW
- Both use `tracing_subscriber` with env filter (`RUST_LOG`)

## Key Files

- `client/src/main.rs` — TCP + UDP handling, fragmentation, WS client
- `server/src/main.rs` — TCP + UDP relay, fragmentation, WS server