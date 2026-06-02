mod commands;
mod config;
mod privilege;
mod process;
mod state;

use tauri::{Manager, RunEvent, WindowEvent};

use crate::state::{AppState, ProxyStateKind};

/// Directory used for the GUI's rotating log files.
///
/// In release builds the process runs with the `windows` subsystem, so
/// stdout/stderr are detached and panic messages vanish. We instead write
/// to a file under `<exe-dir>/logs/` (or `%LOCALAPPDATA%/OmniProxy/logs/`
/// if the exe directory is not writable, e.g. under Program Files). The
/// `Open logs` shortcut on the Settings page reveals this path.
pub fn log_dir() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let dir = dir.join("logs");
        if std::fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let dir = PathBuf::from(local).join("OmniProxy").join("logs");
        if std::fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    std::env::temp_dir().join("omniproxy-gui").join("logs")
}

/// Initialise the global tracing subscriber. Writes to a daily-rotating
/// log file AND to stderr (so `cargo run` / dev builds still see output).
/// Must be called exactly once at process start.
fn init_logging() {
    use tracing_appender::rolling::{Rotation, RollingFileAppender};
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let log_dir = log_dir();
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "omniproxy-gui.log");
    let (file_writer, _file_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "omniproxy_gui=info,tauri=info,wry=warn".into());

    let file_layer = fmt::layer()
        .with_writer(file_writer)
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

    // Emit a single banner so users can confirm logging is working
    // and locate the file after a crash.
    tracing::info!(
        log_dir = %log_dir.display(),
        exe     = ?std::env::current_exe().ok().map(|p| p.display().to_string()),
        pid     = std::process::id(),
        "OmniProxy GUI starting"
    );

    // Capture panics to the same log file. Without this, a panic during
    // setup or runtime silently kills the GUI in release builds and the
    // user has no diagnostic trail.
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
        tracing::error!(panic.payload = %payload, panic.location = %location, "PANIC");
        // Still invoke the default hook so the OS can show a dialog in
        // debug builds / terminate the process in release.
        default_hook(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_path = config::default_config_path();
            let cfg = match config::load_or_init(&config_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = %format!("{e:#}"), path = %config_path.display(), "failed to init config");
                    return Err(format!("failed to init config: {e:#}").into());
                }
            };
            let app_state = AppState::new(config_path.clone(), cfg);
            app.manage(app_state);
            tracing::info!(config_path = %config_path.display(), "GUI setup complete");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            commands::is_elevated,
            commands::check_binary_present,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build tauri app")
        .run(|app_handle, event| {
            if let RunEvent::WindowEvent {
                event: WindowEvent::Destroyed,
                ..
            } = event
            {
                // best-effort: kill child on window destroy (Windows "X" close)
                let state: tauri::State<'_, AppState> = app_handle.state();
                let child_arc = state.child.clone();
                let proxy_state = state.proxy_state.clone();
                tokio::spawn(async move {
                    let mut g = child_arc.lock().await;
                    if let Some(mut p) = g.take() {
                        crate::process::kill_child(&mut p.child).await;
                    }
                    let mut s = proxy_state.lock().await;
                    s.state = ProxyStateKind::Stopped;
                    s.pid = 0;
                });
            }
            if let RunEvent::ExitRequested { .. } = event {
                // allow exit; cleanup happens via Drop on AppState
                let _ = app_handle.state::<AppState>();
            }
        });
}
