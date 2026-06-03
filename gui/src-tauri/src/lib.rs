mod commands;
mod config;
mod privilege;
mod process;
mod state;

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use tauri::{Manager, RunEvent, WindowEvent};

use crate::state::AppState;

/// Global handle to the GUI's log file. Wrapped in a `Mutex<File>` so
/// writes are synchronous — no background thread that can be killed
/// before flushing. The file is opened in append mode so each launch
/// continues the previous run's log.
static LOG_FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

/// Path of the file the global `LOG_FILE` is writing to, set the moment
/// we successfully open it. Surfaced via the `log_dir` command so users
/// can locate it from the UI.
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Pick a writable directory for the GUI's log file. Order:
/// 1. `<exe-dir>\logs\`                   — next to the binary (preferred,
///                                          so logs ship with the bundle
///                                          and the user can find them
///                                          next to omniproxy-gui.exe)
/// 2. `%LOCALAPPDATA%\OmniProxy\logs\`   — Windows standard app-data
/// 3. `%TEMP%\omniproxy-gui\logs\`        — last-resort fallback
pub fn log_dir() -> PathBuf {
    let candidates: [PathBuf; 3] = [
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("logs")))
            .unwrap_or_default(),
        std::env::var("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("OmniProxy").join("logs"))
            .unwrap_or_default(),
        std::env::temp_dir().join("omniproxy-gui").join("logs"),
    ];

    for c in &candidates {
        if c.as_os_str().is_empty() {
            continue;
        }
        match std::fs::create_dir_all(c) {
            Ok(_) => {
                // Probe-write a tiny file to confirm we have actual
                // write permission (create_dir_all succeeds for
                // read-only dirs on some filesystems).
                let probe = c.join(".write-probe");
                if std::fs::write(&probe, b"ok").is_ok() {
                    let _ = std::fs::remove_file(&probe);
                    return c.clone();
                }
            }
            Err(e) => {
                eprintln!("log_dir candidate {} failed: {e}", c.display());
            }
        }
    }

    // Should be unreachable: %TEMP% is always writable, but if even
    // that fails we still return something so the caller can try.
    std::env::temp_dir().join("omniproxy-gui").join("logs")
}

/// Append a line to the global log file, if it has been opened. Also
/// mirrors to stderr. Synchronous: the bytes are on disk before this
/// returns, so a panic / crash immediately after still leaves a record.
fn log_line(level: &str, msg: &str) {
    eprintln!("[{level}] {msg}");
    if let Some(lock) = LOG_FILE.get() {
        if let Ok(mut f) = lock.lock() {
            let ts = chrono::Utc::now().to_rfc3339();
            let _ = writeln!(f, "{ts} [{level}] {msg}");
            let _ = f.flush();
        }
    }
}

/// Initialise the global log file + panic hook. Must be called exactly
/// once, as the very first thing in `run()` (or `main()`).
fn init_logging() {
    let dir = log_dir();
    let path = dir.join("omniproxy-gui.log");
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(file) => {
            // First-line banner BEFORE installing the subscriber, so we
            // know the file works even if the subscriber init fails.
            if let Ok(mut f) = file.try_clone() {
                let _ = writeln!(
                    f,
                    "{} [INFO] === OmniProxy GUI starting === exe={:?} pid={} log={}",
                    chrono::Utc::now().to_rfc3339(),
                    std::env::current_exe().ok().map(|p| p.display().to_string()),
                    std::process::id(),
                    path.display()
                );
                let _ = f.flush();
            }
            let _ = LOG_FILE.set(Mutex::new(file));
            let _ = LOG_PATH.set(path.clone());
            eprintln!("[INFO] logging to {}", path.display());
        }
        Err(e) => {
            eprintln!("[ERROR] failed to open log file {}: {e}", path.display());
        }
    }

    // tracing → log file + stderr
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "omniproxy_gui=info,tauri=info,wry=warn".into());

    let file_layer = fmt::layer()
        .with_writer(|| LogFileGuard)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false);

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(true);

    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(stderr_layer)
        .try_init();

    // Mirror tracing events into our plain-text log file so the two
    // stay in sync. (The file_layer already writes formatted tracing
    // events; this is belt-and-braces for crash forensics.)
    log_line(
        "INFO",
        &format!(
            "tracing initialised; log_dir={} log_path={}",
            dir.display(),
            LOG_PATH.get().map(|p| p.display().to_string()).unwrap_or_default()
        ),
    );

    // Panic hook: capture payload + location, flush, then re-invoke
    // the default hook (so the OS can show a dialog / terminate).
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "<non-string panic payload>".to_string()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "<unknown>".into());
        let msg = format!("PANIC at {location}: {payload}");
        log_line("ERROR", &msg);
        tracing::error!(panic.payload = %payload, panic.location = %location, "PANIC");
        default_hook(info);
    }));
}

/// `tracing::MakeWriter` guard that writes to the global `LOG_FILE`.
/// Used in a closure `|| LogFileGuard` with `tracing_subscriber::fmt::Layer::with_writer`.
/// The closure implements `MakeWriter` automatically via the blanket impl.
struct LogFileGuard;
impl Write for LogFileGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(lock) = LOG_FILE.get() {
            if let Ok(mut f) = lock.lock() {
                return f.write(buf);
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(lock) = LOG_FILE.get() {
            if let Ok(mut f) = lock.lock() {
                return f.flush();
            }
        }
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    log_line("INFO", "Tauri::Builder::default()...");

    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    log_line("INFO", "Tauri::Builder::setup()...");
    let builder = builder.setup(|app| {
        let config_path = config::default_config_path();
        log_line(
            "INFO",
            &format!("config path resolved to {}", config_path.display()),
        );
        let cfg = match config::load_or_init(&config_path) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("failed to init config at {}: {e:#}", config_path.display());
                log_line("ERROR", &msg);
                tracing::error!(error = %format!("{e:#}"), "failed to init config");
                return Err(msg.into());
            }
        };
        let app_state = AppState::new(config_path.clone(), cfg);
        app.manage(app_state);

        // Match the WebView2 native background to the page surface.
        // Without this, WebView2 paints a default white background
        // around the rendered HTML, producing a visible white frame
        // around the dark UI (and a white flash on startup / resize).
        // The format is RGBA in `0..=255`; `0f1115` is the value of
        // `--color-surface` in tailwind theme.
        for window in app.webview_windows().values() {
            let _ = window.set_background_color(Some(tauri::webview::Color(
                0x0f, 0x11, 0x15, 0xff,
            )));
        }

        // On Windows 11, DWM adds a 1-px system border around every
        // window — even ones with `decorations: false`.  Set the border
        // colour to match the app surface so it blends in, eliminating
        // the white ring visible on dark-themed desktops.
        #[cfg(windows)]
        {
            use tauri::Manager as _;
            for window in app.webview_windows().values() {
                if let Ok(hwnd) = window.hwnd() {
                    set_dwm_border_color(hwnd.0 as _);
                }
            }
        }

        log_line("INFO", "setup callback complete");
        Ok(())
    });

    log_line("INFO", "Tauri::generate_handler!()...");
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::get_gui_config,
        commands::save_gui_config,
        commands::default_config_path,
        commands::open_config_dir,
        commands::open_log_dir,
        commands::log_dir,
        commands::upsert_node,
        commands::start_proxy,
        commands::stop_proxy,
        commands::proxy_status,
        commands::proxy_binary_path,
        commands::get_proxy_admin_url,
        commands::get_client_admin_url,
        commands::proxy_stats,
        commands::client_stats,
        commands::proxy_routes,
        commands::is_elevated,
        commands::check_binary_present,
    ]);

    log_line("INFO", "Tauri::Builder::build()...");
    let app = match builder.build(tauri::generate_context!()) {
        Ok(a) => {
            log_line("INFO", "Tauri app built; entering run loop");
            a
        }
        Err(e) => {
            let msg = format!("failed to build tauri app: {e:#}");
            log_line("ERROR", &msg);
            tracing::error!(error = %format!("{e:#}"), "failed to build tauri app");
            eprintln!("[FATAL] {msg}");
            std::process::exit(1);
        }
    };

    app.run(|app_handle, event| {
        if let RunEvent::WindowEvent {
            event: WindowEvent::Destroyed,
            ..
        } = event
        {
            let state: tauri::State<'_, AppState> = app_handle.state();
            let child_arc = state.child.clone();
            // Signal the waiter to stop the proxy child. The waiter owns
            // the `Child` and is the only place that should call kill();
            // we just hand it a stop signal and let it tear down. The GUI
            // process will exit shortly after, and `kill_on_drop` on the
            // `Child` is the final safety net if the waiter is somehow
            // cancelled mid-shutdown.
            tauri::async_runtime::spawn(async move {
                let tx = {
                    let guard = child_arc.lock().await;
                    guard.as_ref().map(|p| p.stop_tx.clone())
                };
                if let Some(tx) = tx {
                    let _ = tx.send(true);
                }
            });
        }
        if let RunEvent::ExitRequested { .. } = event {
            let _ = app_handle.state::<AppState>();
        }
    });
}

/// Set the DWM border colour for an HWND to match the app surface (#0f1115).
///
/// On Windows 11, DWM renders a 1-px coloured border around every top-level
/// window regardless of `decorations: false`.  Calling this with the surface
/// colour makes the border invisible against the dark background.
///
/// `DWMWA_BORDER_COLOR` (value 34) accepts a `COLORREF` (0x00BBGGRR).
/// `#0f1115` in COLORREF byte order: R=0x0f, G=0x11, B=0x15 → 0x00_15_11_0f.
/// Using `DWMWA_COLOR_NONE` (0xFFFF_FFFF) would ask DWM to use no border at
/// all, but that leaves a transparent gap; matching the colour is cleaner.
#[cfg(windows)]
fn set_dwm_border_color(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR};

    // COLORREF: 0x00BBGGRR.  Surface colour #0f1115 → R=0x0f G=0x11 B=0x15.
    let color: u32 = 0x0015_110f;
    unsafe {
        let _ = DwmSetWindowAttribute(
            HWND(hwnd as _),
            DWMWA_BORDER_COLOR,
            &color as *const u32 as *const _,
            std::mem::size_of::<u32>() as u32,
        );
    }
}
