use crate::config::{self, GuiConfig, NodeConfig};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn get_gui_config(state: State<'_, AppState>) -> Result<GuiConfig, String> {
    let guard = state.config.try_lock().map_err(|e| e.to_string())?;
    Ok(guard.clone())
}

#[tauri::command]
pub async fn save_gui_config(cfg: GuiConfig, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(node) = cfg.active_node() {
        config::validate(node).map_err(|e| e.to_string())?;
    }
    {
        let mut guard = state.config.lock().await;
        *guard = cfg;
    }
    let path = state.config_path.clone();
    let snapshot = state.config.lock().await.clone();
    config::write(&path, &snapshot).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn default_config_path(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.config_path.display().to_string())
}

#[tauri::command]
pub async fn open_config_dir(state: State<'_, AppState>) -> Result<(), String> {
    let path = state.config_path.clone();
    let dir = path.parent().ok_or("config path has no parent")?;
    open_in_os_file_manager(dir).map_err(|e| e.to_string())
}

/// Return the path to the directory containing the GUI's rotating log
/// files. Surfaced in the UI so users can find the file after a crash.
#[tauri::command]
pub fn log_dir() -> String {
    crate::log_dir().display().to_string()
}

/// Open the GUI's log directory in the OS file manager.
#[tauri::command]
pub async fn open_log_dir() -> Result<(), String> {
    let dir = crate::log_dir();
    open_in_os_file_manager(&dir).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn open_in_os_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn open_in_os_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_in_os_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[tauri::command]
pub async fn upsert_node(
    index: Option<usize>,
    node: NodeConfig,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    config::validate(&node).map_err(|e| e.to_string())?;
    let mut guard = state.config.lock().await;
    let new_index = match index {
        Some(i) if i < guard.nodes.len() => {
            guard.nodes[i] = node;
            i
        }
        _ => {
            guard.nodes.push(node);
            guard.nodes.len() - 1
        }
    };
    let snapshot = guard.clone();
    drop(guard);
    config::write(&state.config_path, &snapshot).map_err(|e| e.to_string())?;
    Ok(new_index)
}
