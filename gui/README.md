# OmniProxy GUI

Tauri 2 + Next.js desktop application for [OmniProxy](../README.md).

## Prerequisites

- Node.js ≥ 20
- pnpm ≥ 9 (`npm install -g pnpm`)
- Rust stable + `cargo` (see https://tauri.app/start/prerequisites/)
- Windows: WebView2 runtime, MSVC build tools
- macOS: Xcode CLI tools
- Linux: webkit2gtk-4.1, libssl-dev, etc.

## Development

```bash
# from repo root
cargo build -p proxy -p client --release

cd gui
pnpm install
pnpm tauri dev
```

The first run will copy `proxy.exe` and `client.exe` from
`../target/release/` into the GUI build output.

## Production build

```bash
cd gui
pnpm tauri build
```

This produces platform-specific installers in
`src-tauri/target/release/bundle/`.

On Windows this triggers UAC at startup; the bundled MSI installs the app
to `%ProgramFiles%` and embeds a `requireAdministrator` manifest.

## Configuration

The GUI's own `config.yaml` lives next to the installed binary:

- Production: `<install-dir>/config.yaml`
- Dev mode: `gui/config.yaml`

Edit it via the **Settings** page, or stop the GUI and edit it directly.
Restart the GUI after manual edits.

The GUI does not write a `proxy` config file. Each node is passed to the
`proxy` process as CLI arguments when the user clicks **Start**.

## Architecture

- `src/` — Next.js (React 19, Tailwind 4, React Query 5, Zustand 5, uPlot)
- `src-tauri/` — Rust + Tauri 2 backend
- Frontend talks to the proxy/client admin HTTP API over loopback

See `../.opencode/plans/1780312491374-glowing-rocket.md` for the full design.
