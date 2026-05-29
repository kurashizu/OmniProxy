# OmniProxy - Agent Notes

## Project Structure
- Workspace root `Cargo.toml` manages all crates; use `-p <crate>` to build individually.
- Main packages: `client/` (SOCKS5 over WebSocket), `server/` (relay), `proxy/` (TUN/routing manager).
- `protocol/` is a shared library crate for wire-format codec used by both client and server.
- `client` is a single binary crate; `src/main.rs` is the entrypoint and `src/bootstrap.rs` handles runtime init.

## Build & Test Commands
- **Build client**: `cargo build -p client --release`
- **Build proxy**: `cargo build -p proxy --release`
- **Build server**: `cargo build -p server --release`
- **Build all**: `cargo build --release`
- **Check (lint)**: `cargo check`
- **Check single crate**: `cargo check -p server`
- **Format check**: `cargo fmt --check`
- **Format fix**: `cargo fmt`
- **Clippy (if configured)**: `cargo clippy -- -D warnings`

## Code Style
- Use standard rustfmt defaults (no custom rustfmt.toml)
- Prefer `anyhow::Result` for error handling
- Use `tracing` for logging (info!, warn!, debug!, error!)
- Use `tokio` for async runtime
- Prefer `use` at top of file; group by: std, external crates, local modules
- Keep functions focused and small
- Use descriptive variable names
- Add doc comments for public APIs and CLI fields

## Architecture Notes
- Client WebSocket frame format is fixed: `[4B stream_id][1B type][payload]`; UDP uses `stream_id=0`.
- Wire protocol codec lives in `protocol/src/lib.rs`, shared by client and server.
- `client` can auto-prepend `wss://` when `server` omits a scheme, but it no longer auto-detects outbound IP changes.
- Client reconnect is bounded to 5 retries; server does not reconnect to client.
- `proxy` auto-passes `--outbound-ip` to `client` when a physical interface IP is available.
- Keep `.opencode/` out of commits; it is gitignored.
- Release tags start with `v`; GitHub Actions builds all three packages and publishes zip (Windows) or tar.xz (Linux/macOS) artifacts.

## Testing
- Verify changes with `cargo check`.
- Run manual tests: `curl -x socks5h://127.0.0.1:1080 <url>` for TCP, `dig @8.8.8.8 example.com` for UDP.
