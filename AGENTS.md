# OmniProxy - Agent Notes

- Repo has no workspace `Cargo.toml`; build each package with its own manifest path.
- Main packages: `client/` (SOCKS5 over WebSocket), `server/` (relay), `proxy/` (TUN/routing manager).
- `client` is a single binary crate; `src/main.rs` is the entrypoint and `src/bootstrap.rs` handles runtime init.
- Client WebSocket frame format is fixed: `[4B stream_id][1B type][payload]`; UDP uses `stream_id=0`.
- `client` can auto-prepend `wss://` when `server` omits a scheme, but it no longer auto-detects outbound IP changes.
- Client reconnect is bounded to 5 retries; server does not reconnect to client.
- `proxy` auto-passes `--outbound-ip` to `client` when a physical interface IP is available.
- Keep `.opencode/` out of commits; it is gitignored.
- Release tags start with `v`; GitHub Actions builds all three packages and publishes zip artifacts.
- Verify client changes with `cargo check --manifest-path client/Cargo.toml`.
- Verify proxy/server changes with their own manifest paths; do not assume the client manifest covers them.
