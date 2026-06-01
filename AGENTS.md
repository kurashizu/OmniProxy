# OmniProxy - Agent Notes

## Project Structure
- Workspace root `Cargo.toml` manages all crates; use `-p <crate>` to build individually.
- Four packages: `client/` (SOCKS5 over WebSocket), `server/` (relay), `proxy/` (TUN/routing manager), `protocol/` (shared wire-format codec).
- `client` is a single binary; `src/main.rs` + `src/bootstrap.rs`.
- `proxy` is a single binary; `src/main.rs` + `src/stack.rs` + `src/forwarder/`.
- `server` is a single binary; `src/main.rs` + `src/session.rs` (WebSocket multiplexer).
- `gui/` — Tauri 2 + Next.js 15 desktop application (see GUI section below).

## Build & Test Commands
- **Build all (release)**: `cargo build --release`
- **Build single crate**: `cargo build -p client --release`, `-p proxy`, `-p server`
- **Check**: `cargo check`
- **Format**: `cargo fmt` / `cargo fmt --check`
- **Clippy**: `cargo clippy -- -D warnings`
- **Console feature** (tokio-console): requires `RUSTFLAGS="--cfg tokio_unstable"` and `--features console`. CI uses this for client and server builds. Not used in proxy.
- **Windows cross-compile check**: `cargo check -p server -p client --target x86_64-pc-windows-gnu`

## Architecture Notes

### Wire Protocol (`protocol/src/lib.rs`)
- Frame: `[4B stream_id][1B type][payload]`
- `decode_frame(data: Bytes)` — caller passes `Bytes` (zero-copy); **do not pass `&[u8]`**
- `encode_frame_bytes(stream_id, typ, payload: Bytes)` — use for Bytes payloads to avoid double-copy
- `decode_udp_payload(payload: &Bytes)`, `decode_icmp_payload(payload: &[u8])` — decode helpers

### Client MUX (`client/src/mux.rs`)
- Multiplexes all TCP/UDP/ICMP streams over a single WebSocket connection.
- **Three priority channels** for outbound frames:
  - `frame_hi` (cap 64): control frames — CONNECT, FIN, ICMP register. Strict priority.
  - `frame_mi` (cap 256): low-volume data (interactive traffic).
  - `frame_lo` (cap 1024): high-volume data (bulk transfers).
- Writer drains hi → mi (max 16/frame iteration) → lo. `biased select!` ensures hi is always checked first.
- **Per-stream rate tracking** (`RateTracker`): every 16 frames, checks accumulated bytes. ≥100KB/s → reclassify to lo, ≤10KB/s → reclassify to mi. Thresholds: `RATE_HI_THRESHOLD`, `RATE_LO_THRESHOLD`.
- **Stream ID allocation** (`alloc_id_locked`): collision detection loop checks all four HashMaps (`streams`, `connect_notify`, `udp_txs`, `icmp_txs`, `rate_trackers`). Alloc + insert happen inside the same `write()` lock to eliminate TOCTOU.
- `frame_tx` is the old single-channel name — now uses `frame_hi`/`frame_mi`/`frame_lo`. All `send_*` methods clone the sender from `MuxInner` under `read()` lock, then send without holding the lock.

### Client ↔ Server Communication
- Client connects via secure WebSocket to server.
- `client` registers streams with `server` via `TYPE_TCP_CONNECT / TYPE_UDP_DATA / TYPE_ICMP_DATA` frames.
- Client SOCKS5 server listens on `127.0.0.1:1080`; proxy SOCKS5 server is also on `127.0.0.1:1080`.
- Server `session.rs` is the WebSocket session multiplexer; it dispatches frames to TCP/UDP/ICMP handlers.
- Server uses `DashMap<u32, ...>` for stream, UDP socket, and ICMP stream tables.
- Server uses `tokio::sync::Semaphore` (4096 permits) to limit concurrent streams.
- **Server TCP backpressure**: `tx.send(payload)` is wrapped in `tokio::time::timeout(5s, ...)`. If a stream's channel is full for >5s, the stream is dropped rather than blocking the entire session mux.
- **Server UDP bind atomicity**: `udp_sockets.entry(stream_id)` directly — no `get()` then `entry()` two-phase pattern (was a race window).

### Proxy Architecture
- `proxy` runs `netstack-smoltcp` as the TUN interface's network stack.
- ICMP packets are intercepted **before** the netstack via the `icmp::IcmpHandler` in `proxy/src/forwarder/icmp.rs`.
- ICMP passthrough uses a custom SOCKS5 CMD=0xA1 over a dedicated TCP connection.
- UDP sessions are relay-based: proxy binds a local UDP socket and relays via SOCKS5 UDP ASSOCIATE.
- **Forwarder task separation** (`proxy/src/forwarder/mod.rs`):
  - Infrastructure tasks (`tun_to_stack`, `stack_to_tun`, `tun_writer`) — exit means TUN/netstack is broken, triggers full restart via `stack.rs`.
  - Service tasks (`tcp_task`, `udp_task`, `icmp_task`) — exit does NOT trigger restart. Wrapped in `tokio::select!` with a broadcast shutdown channel. ICMP exit only loses ping, TCP/UDP survive.
  - Infrastructure `select!` fires → `shutdown_tx.send(())` → sleep 1s → return `Ok(())` → `stack.rs` restart loop handles tun_down/tun_up.

### Platform-Specific
- `server/src/icmp/` — raw ICMP socket code, **Unix-only** (`#[cfg(unix)]`). Windows builds use a stub that logs a warning.
- `proxy/src/network.rs` and `proxy/src/forwarder/tun_device.rs` — route/TUN setup, platform-specific (`target_os = "linux"`, `macos`, `windows`).
- Client `proxy/src/ws.rs` — `DNS_CACHE` with 5-minute TTL; `getifaddrs` / `GetAdaptersAddresses` for interface binding.

### Client Reconnect
- On WebSocket disconnect, client uses **exponential backoff** (1s → 2s → ... → 30s cap) with **infinite retries**.

### Client Latency Measurement
- `client/src/mux.rs::spawn_latency_pinger` sends `TYPE_PING` (0x08) frames every 1s; server echoes back as `TYPE_PONG` (0x09).
- RTT recorded in `Stats::latency_ms` (u32::MAX means timeout).
- `/stats` exposes `latency_ms` and `latency_jitter_ms`.

### Client Server Info
- `client/src/mux.rs::refresh_outbound_ips` queries `api.ipify.org` (v4) and `api64.ipify.org` (v6) every 5 min.
- `Mux::server_info` field caches the result; `extract_host` resolves `cfg.server` → IP at startup.
- Server-side outbound IPs come from `TYPE_SERVER_INFO` (0x07) frame sent at WS handshake (see `server/src/session.rs`).

## GUI (`gui/`)
- Tauri 2 + Next.js 15 desktop application.
- `gui/src-tauri/Cargo.toml` is **not** part of the root workspace (its own `[workspace]` empty annotation).
- Frontend builds static (`next build` → `gui/out/`); Tauri embeds via `tauri.conf.json:build.frontendDist = "../out"`.
- **Dev**: `cd gui && pnpm install && pnpm tauri dev` (requires `proxy.exe` + `client.exe` in `target/release/`).
- **Build**: `cd gui && pnpm tauri build` → MSI/NSIS in `gui/src-tauri/target/release/bundle/`.
- **UAC**: `gui/src-tauri/build.rs` injects `requireAdministrator` manifest into the Windows binary.
- **Layout** (仿 Clash Verge Rev):
  - Sidebar 72px wide, 5 icon nav (Home/Connections/Routes/Logs/Settings) + bottom Start/Stop button.
  - Each page balanced 2-3 cards; no duplicate info across pages.
  - Home: NodeCard / ConnectionStatusCard / TrafficCard.
  - Connections: 3 cards (summary / search+filter / live table).
  - Routes: TunInfoCard / RoutesTableCard.
  - Logs: LogFilterCard (no DEBUG level) / LogViewerCard (5000-line ring buffer).
  - Settings: NodeFormCard / AboutCard (author kurashizu, GitHub, version).
- **App starts `stopped`**: never auto-spawns proxy. User must click the sidebar Start/Stop button.
- **Configuration**: GUI stores its own `config.yaml` next to the GUI exe; passes node settings to `proxy` as CLI args.

## Code Style
- Standard rustfmt defaults (no custom `rustfmt.toml`).
- `anyhow::Result` for error handling; `tracing` for logging.
- Prefer `use` at top of file, group by: std → external → local.
- Doc comments for public APIs.

## Release
- Update version in `Cargo.toml` (`[workspace.package]`), commit, then tag `v*` and push.
- GitHub Actions builds all three packages and publishes zip (Windows) or tar.xz (Linux/macOS).
- Keep `.opencode/` out of commits; it is gitignored.
