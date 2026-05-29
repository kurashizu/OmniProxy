# OmniProxy - Agent Notes

## Project Structure
- Repo has no workspace `Cargo.toml`; build each package with its own manifest path.
- Main packages: `client/` (SOCKS5 over WebSocket), `server/` (relay), `proxy/` (TUN/routing manager).
- `client` is a single binary crate; `src/main.rs` is the entrypoint and `src/bootstrap.rs` handles runtime init.

## Build & Test Commands
- **Build client**: `cargo build --manifest-path client/Cargo.toml --release`
- **Build proxy**: `cargo build --manifest-path proxy/Cargo.toml --release`
- **Build server**: `cargo build --manifest-path server/Cargo.toml --release`
- **Check (lint)**: `cargo check --manifest-path <crate>/Cargo.toml`
- **Format check**: `cargo fmt --check --manifest-path <crate>/Cargo.toml`
- **Format fix**: `cargo fmt --manifest-path <crate>/Cargo.toml`
- **Clippy (if configured)**: `cargo clippy --manifest-path <crate>/Cargo.toml -- -D warnings`

## Code Style
- Use standard rustfmt defaults (no custom rustfmt.toml)
- Prefer `anyhow::Result` for error handling
- Use `tracing` for logging (info!, warn!, debug!, error!)
- Use `tokio` for async runtime
- Prefer `use` at top of file; group by: std, external crates, local modules
- Keep functions focused and small
- Use descriptive variable names
- Add doc comments for public APIs

## Architecture Notes
- Client WebSocket frame format is fixed: `[4B stream_id][1B type][payload]`; UDP uses `stream_id=0`.
- `client` can auto-prepend `wss://` when `server` omits a scheme, but it no longer auto-detects outbound IP changes.
- Client reconnect is bounded to 5 retries; server does not reconnect to client.
- `proxy` auto-passes `--outbound-ip` to `client` when a physical interface IP is available.
- Keep `.opencode/` out of commits; it is gitignored.
- Release tags start with `v`; GitHub Actions builds all three packages and publishes zip artifacts.

## Testing
- Verify client changes with `cargo check --manifest-path client/Cargo.toml`.
- Verify proxy/server changes with their own manifest paths; do not assume the client manifest covers them.
- Run manual tests: `curl -x socks5h://127.0.0.1:1080 <url>` for TCP, `dig @8.8.8.8 example.com` for UDP.
