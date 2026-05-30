# OmniProxy - Agent Notes

## Project Structure
- Workspace root `Cargo.toml` manages all crates; use `-p <crate>` to build individually.
- Four packages: `client/` (SOCKS5 over WebSocket), `server/` (relay), `proxy/` (TUN/routing manager), `protocol/` (shared wire-format codec).
- `client` is a single binary; `src/main.rs` + `src/bootstrap.rs`.
- `proxy` is a single binary; `src/main.rs` + `src/stack.rs` + `src/forwarder/`.
- `server` is a single binary; `src/main.rs` + `src/session.rs` (WebSocket multiplexer).

## Build & Test Commands
- **Build all (release)**: `cargo build --release`
- **Build single crate**: `cargo build -p client --release`, `-p proxy`, `-p server`
- **Check**: `cargo check`
- **Format**: `cargo fmt` / `cargo fmt --check`
- **Clippy**: `cargo clippy -- -D warnings`
- **Windows cross-compile check**: `cargo check -p server -p client --target x86_64-pc-windows-gnu`

## Architecture Notes

### Wire Protocol (`protocol/src/lib.rs`)
- Frame: `[4B stream_id][1B type][payload]`
- `decode_frame(data: Bytes)` — caller passes `Bytes` (zero-copy); **do not pass `&[u8]`**
- `encode_frame_bytes(stream_id, typ, payload: Bytes)` — use for Bytes payloads to avoid double-copy
- `decode_udp_payload(payload: &Bytes)`, `decode_icmp_payload(payload: &[u8])` — decode helpers

### Client ↔ Server Communication
- Client connects via secure WebSocket to server.
- `client` registers streams with `server` via `TYPE_TCP_CONNECT / TYPE_UDP_DATA / TYPE_ICMP_DATA` frames.
- Client SOCKS5 server listens on `127.0.0.1:1080`; proxy SOCKS5 server is also on `127.0.0.1:1080`.
- Server `session.rs` is the WebSocket session multiplexer; it dispatches frames to TCP/UDP/ICMP handlers.
- Server uses `DashMap<u32, ...>` for stream, UDP socket, and ICMP stream tables.
- Server uses `tokio::sync::Semaphore` (4096 permits) to limit concurrent streams.

### Proxy Architecture
- `proxy` runs `netstack-smoltcp` as the TUN interface's network stack.
- ICMP packets are intercepted **before** the netstack via the `icmp::IcmpHandler` in `proxy/src/forwarder/icmp.rs`.
- ICMP passthrough uses a custom SOCKS5 CMD=0xA1 over a dedicated TCP connection.
- UDP sessions are relay-based: proxy binds a local UDP socket and relays via SOCKS5 UDP ASSOCIATE.

### Platform-Specific
- `server/src/icmp/` — raw ICMP socket code, **Unix-only** (`#[cfg(unix)]`). Windows builds use a stub that logs a warning.
- `proxy/src/network.rs` and `proxy/src/forwarder/tun_device.rs` — route/TUN setup, platform-specific (`target_os = "linux"`, `macos`, `windows`).
- Client `proxy/src/ws.rs` — `DNS_CACHE` with 5-minute TTL; `getifaddrs` / `GetAdaptersAddresses` for interface binding.

### Client Reconnect
- On WebSocket disconnect, client uses **exponential backoff** (1s → 2s → ... → 30s cap) with **infinite retries**.
- Previously had a 5-retry cap; the cap was removed in v1.0.3-beta.2.

## Code Style
- Standard rustfmt defaults (no custom `rustfmt.toml`).
- `anyhow::Result` for error handling; `tracing` for logging.
- Prefer `use` at top of file, group by: std → external → local.
- Doc comments for public APIs.

## Release
- Update version in `Cargo.toml` (`[workspace.package]`), commit, then tag `v*` and push.
- GitHub Actions builds all three packages and publishes zip (Windows) or tar.xz (Linux/macOS).
- Keep `.opencode/` out of commits; it is gitignored.