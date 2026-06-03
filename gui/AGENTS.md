# GUI — Agent Notes

## Overview

Tauri 2 + Next.js 15 desktop application for OmniProxy. The GUI launches
`proxy.exe` as a child process (which in turn forks `client.exe`) and exposes
controls to start/stop the proxy, monitor live stats, browse active
connections, inspect routes, and read process logs.

**`gui/src-tauri/Cargo.toml` is NOT part of the root workspace** — it
declares its own `[workspace]` to keep Tauri build artefacts separate.
Always `cd gui/src-tauri` (or use `working-directory: gui`) when building
the GUI.

---

## Directory Layout

```
gui/
├── src/                        Next.js 15 frontend (App Router, static export)
│   ├── app/                    Pages: / connections/ routes/ logs/ settings/
│   ├── components/
│   │   ├── common/             Card, Chart, Dialog, StatTile, …
│   │   ├── home/               NodeCard, ConnectionStatusCard, TrafficCard
│   │   ├── connections/        ConnectionsPage cards
│   │   ├── routes/             TunInfoCard, RoutesTableCard
│   │   ├── logs/               LogFilterCard, LogViewerCard
│   │   ├── settings/           NodeFormCard, AboutCard
│   │   └── layout/             Shell, Sidebar, TopBar
│   ├── hooks/
│   │   ├── useProxyState.ts    Listens to "proxy-state" Tauri event + polls proxy_status
│   │   ├── useAdminPoll.ts     TanStack Query wrappers for proxy_stats / client_stats / proxy_routes
│   │   ├── useTrafficSamples.ts  Ring-buffer of (up/down bytes/s) samples for the chart
│   │   ├── useAppInfo.ts
│   │   └── useElevated.ts
│   ├── lib/
│   │   ├── ipc.ts              All invoke() calls — single source of truth for Tauri commands
│   │   ├── schema.ts           TypeScript interfaces mirroring Rust serde structs
│   │   ├── format.ts           formatBytes, formatDuration, formatTimestamp, copyToClipboard
│   │   ├── i18n.ts             t() helper + pickLocale()
│   │   ├── traffic.ts          TrafficRingBuffer, diffTraffic
│   │   └── locales/            en.json, zh.json
│   └── store/
│       ├── appStore.ts         Zustand store: locale, logBuffer, filters, paused flags
│       └── queryClient.ts      TanStack Query client (staleTime=0, no background refetch)
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs              Tauri app entry: logging, setup, DWM border fix, command registration
│   │   ├── main.rs             Thin wrapper calling lib::run()
│   │   ├── state.rs            AppState, ProxyState, ProxyProcess, ProxyStateKind
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── proxy.rs        start_proxy, stop_proxy, proxy_status, proxy_stats,
│   │   │   │                   client_stats, proxy_routes, proxy_binary_path
│   │   │   ├── config.rs       get_gui_config, save_gui_config, upsert_node, …
│   │   │   ├── privilege.rs    is_elevated, check_binary_present
│   │   │   └── urls.rs         get_proxy_admin_url, get_client_admin_url
│   │   ├── config/
│   │   │   ├── mod.rs          load_or_init, write, validate, default_config_path
│   │   │   └── schema.rs       GuiConfig, NodeConfig (with serde defaults)
│   │   ├── process/
│   │   │   └── mod.rs          find_binary, resolve_binary/client, build_cli_args,
│   │   │                       spawn_proxy, attach_to_kill_job (Windows Job Object)
│   │   └── privilege/          UAC / elevation helpers
│   ├── resources/              Bundled binaries (checked in as real files, not stubs)
│   │   ├── proxy.exe
│   │   ├── client.exe
│   │   └── wintun.dll
│   ├── Cargo.toml              Standalone workspace; omniproxy-gui v1.0.6
│   ├── tauri.conf.json         Window: 900×700, decorations=false, transparent=true
│   └── build.rs                Injects requireAdministrator UAC manifest (Windows)
├── out/                        Next.js static export (gitignored, rebuilt on each build)
└── package.json / pnpm-lock.yaml
```

---

## Build Commands

```bash
# 1. Build the frontend (must run before cargo build touches Tauri)
cd gui && pnpm install && pnpm build        # → gui/out/

# 2a. Local release build (native platform)
cd gui/src-tauri && cargo build --release

# 2b. Windows cross-compile from Linux
cd gui/src-tauri && cargo build --release --target x86_64-pc-windows-gnu

# 2c. Full Tauri bundle (MSI/NSIS) — only on Windows
cd gui && pnpm tauri build --target x86_64-pc-windows-msvc

# Check only (no platform-native deps needed)
cd gui/src-tauri && cargo check --target x86_64-pc-windows-gnu
```

`pnpm build` must run before any Tauri build; the Rust crate embeds
`../out/` via `frontendDist` and will fail if the directory is missing.

---

## Deployment Layout

All four files must sit **flat in the same directory** next to each other.
`find_binary()` in `process/mod.rs` searches `<exe-dir>/` first, then
`<exe-dir>/resources/`, so placing them at the top level is simplest.

```
OmniProxy-GUI/
├── omniproxy-gui.exe   # Tauri GUI  (≈5.5 MB, statically linked)
├── proxy.exe           # TUN/routing manager (≈2 MB, statically linked)
├── client.exe          # SOCKS5-over-WS client (≈2.9 MB, statically linked)
└── wintun.dll          # WinTUN virtual NIC driver (copy once, never changes)
```

`config.yaml` is created by the GUI on first launch next to the exe.
`logs/` is created automatically at runtime.

---

## Architecture

### Process Lifecycle (`commands/proxy.rs` + `state.rs`)

```
start_proxy()
  └─ spawn proxy.exe (with --client client.exe --admin-port N …)
       └─ proxy.exe forks client.exe internally
  └─ waiter task owns Child, listens on stop_rx watch channel
       ├─ natural exit  → state = Stopped, surface last stderr line if non-zero
       └─ stop signal   → start_kill() → wait() → state = Stopped (no error msg)
stop_proxy()
  └─ sends true on stop_tx watch channel
  └─ polls until state == Stopped (max 5 s)
```

Key invariants:
- `state.child` slot holds a `ProxyProcess` (stop signal + last_error arc), NOT the `Child` itself.
- The waiter task owns `Child` exclusively to avoid deadlocking `kill()` with `wait()`.
- A monotonic `id` prevents a stale waiter from overwriting state after a rapid Stop→Start.
- Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) ensures proxy.exe and
  client.exe die when omniproxy-gui.exe is hard-killed.

### Admin HTTP Polling (`commands/proxy.rs::admin_fetch_json`)

The GUI fetches `proxy.exe` and `client.exe` admin APIs via a minimal hand-rolled
HTTP/1.1 client on the **Rust side** (Tauri command), not via the WebView2
`fetch()` API. WebView2 silently drops loopback requests in some Windows
configurations.

- **Endpoint map**:
  - `proxy_stats`  → `http://127.0.0.1:<admin_port>/stats`      (ProxyStats)
  - `client_stats` → `http://127.0.0.1:<admin_port-1>/stats`    (ClientStats)
  - `proxy_routes` → `http://127.0.0.1:<admin_port>/routes`     (ProxyRoute[])
- **`/routes` envelope**: the proxy returns `{"routes":[…]}`; `proxy_routes`
  unwraps the `.routes` field before returning to the frontend.
- **Timeout**: 2 s total budget wraps the entire round-trip (connect + write +
  read headers + read body). Only the `None` path is returned on timeout.
- **Chunked TE**: the client decodes `Transfer-Encoding: chunked` manually.
- Polling interval: 1 s for stats/connections, 5 s for routes (TanStack Query).
- Guard: commands short-circuit to `None` when `state.child` is empty (proxy
  not running), avoiding spurious connection-refused noise in logs.

### Log Streaming (`Shell.tsx`)

`proxy.rs::spawn_log_reader` emits every stdout/stderr line as a
`"proxy-log"` Tauri event: `{ ts_ms, stream, line }`.

`Shell.tsx` subscribes at mount and batches incoming lines into the Zustand
`logBuffer` (5000-line ring) with a 50 ms coalesce timer to avoid
per-line React re-renders during log bursts.

`"proxy-error"` is a separate event for lines that `looks_like_error()`
returns true for — used to surface the `ErrorBanner` without waiting for
the process to exit.

### Window / DWM (`lib.rs`)

- Window: 900 × 700 px, `decorations: false`, `transparent: true`, not resizable.
- `set_background_color(0x0f, 0x11, 0x15, 0xff)` prevents the white flash
  WebView2 paints before the HTML is ready.
- `set_dwm_border_color(hwnd)` calls `DwmSetWindowAttribute(DWMWA_BORDER_COLOR)`
  with `0x0015_110f` (COLORREF for `#0f1115`) to match the DWM 1-px system
  border to the app surface colour, eliminating the white ring on Windows 11.

### State Machine

```
Stopped ──[start]──▶ Starting ──[spawn ok]──▶ Running
                                                  │
                        ◀──[stop / crash]──────────┘
                     Stopping ──[child dead]──▶ Stopped
Stopped ◀──[error]── Error
```

`ProxyStateKind` is serialised lowercase (`"running"`, `"stopped"`, …).
Frontend checks `state.state === "running"` to enable polls.

---

## Frontend Data Flow

```
Tauri event "proxy-state"
  └─▶ useProxyState → isRunning
        └─▶ useProxyStats(isRunning)   ──▶ NodeCard, ConnectionStatusCard
        └─▶ useClientStats(isRunning)  ──▶ NodeCard, ConnectionStatusCard, TrafficCard
        └─▶ useProxyRoutes(isRunning)  ──▶ RoutesPage

Tauri event "proxy-log"
  └─▶ Shell.tsx listener → appendLog → appStore.logBuffer
        └─▶ LogViewerCard (Virtuoso virtual list, 5000-line cap)

Tauri event "proxy-error"
  └─▶ Shell.tsx listener → setLastProxyError → ErrorBanner
```

`useAdminPoll` disables TanStack Query when `running=false`; re-enabling
it on the next `"proxy-state"` event triggers an immediate fetch.

---

## Config (`config/schema.rs`)

`GuiConfig` is stored as `config.yaml` next to the exe.

| Field | Default | Notes |
|---|---|---|
| `nodes` | `[NodeConfig::default()]` | List of server profiles |
| `active_node` | `0` | Index into `nodes` |
| `node.socks_port` | `1080` | SOCKS5 listen port |
| `node.admin_port` | `10991` | proxy admin port; client uses `admin_port - 1` |
| `node.tun_name` | `"tun0"` | TUN device name |
| `node.tun_ip` | `"198.18.0.1"` | TUN IPv4 address |
| `node.tun_gw` | `"198.18.0.2"` | TUN IPv4 gateway |

`validate()` rejects empty `server`, zero ports, out-of-range prefixes,
and malformed `phys_ip`.

---

## CI (`gui-windows.yml`)

Four jobs on `windows-latest` with the MSVC toolchain:

1. **build-proxy** — `cargo build -p proxy` → uploads `proxy.exe`
2. **build-client** — `cargo build -p client` → uploads `client.exe`
3. **build-gui** — downloads real `proxy.exe`/`client.exe` into
   `gui/src-tauri/resources/` (Tauri validates bundle.resources at build
   time), downloads `wintun.dll` (amd64), then `pnpm tauri build --no-bundle`
   → uploads `omniproxy-gui.exe`
4. **package** — downloads all three exes + wintun.dll, verifies all four
   files are present, zips the **flat contents** of `dist/` (no wrapper
   folder) → `omniproxy-gui-windows-x86_64.zip`

All artifact actions use v4. `RUSTFLAGS=-Ctarget-feature=+crt-static`
produces fully statically linked binaries (no MSVC runtime DLL dependency).
`CARGO_TARGET_DIR` is set globally so all jobs share the same output root.

Trigger: push to `testing` branch, or `workflow_dispatch`.

---

## Code Style

- Standard `rustfmt` defaults (no `rustfmt.toml`).
- `anyhow::Result` + `tracing` throughout the Rust crate.
- React components are all `"use client"` (static export, no server components).
- Tailwind 4 with `@theme` CSS variables; no hardcoded hex outside `globals.css`
  and the DWM colour constant.
- `ipc.ts` is the single place that calls `invoke()`; components never call
  `invoke()` directly.
- `schema.ts` mirrors every Rust struct that crosses the IPC boundary; keep
  them in sync when changing serde shapes.
