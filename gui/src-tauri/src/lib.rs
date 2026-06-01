mod commands;
mod config;
mod privilege;
mod process;
mod state;

use tauri::{Manager, RunEvent, WindowEvent};

use crate::state::{AppState, ProxyStateKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "omniproxy_gui=info".into()),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_path = config::default_config_path();
            let cfg = config::load_or_init(&config_path)
                .map_err(|e| format!("failed to init config: {e:#}"))?;
            let app_state = AppState::new(config_path, cfg);
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_gui_config,
            commands::save_gui_config,
            commands::default_config_path,
            commands::open_config_dir,
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
