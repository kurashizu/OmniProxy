use crate::commands::proxy::{client_admin_url, proxy_admin_url};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_proxy_admin_url(state: State<'_, AppState>) -> Result<String, String> {
    let cfg = state.config.lock().await;
    let node = cfg
        .active_node()
        .ok_or_else(|| "no active node".to_string())?;
    Ok(proxy_admin_url(node))
}

#[tauri::command]
pub async fn get_client_admin_url(state: State<'_, AppState>) -> Result<String, String> {
    let cfg = state.config.lock().await;
    let node = cfg
        .active_node()
        .ok_or_else(|| "no active node".to_string())?;
    Ok(client_admin_url(node))
}
