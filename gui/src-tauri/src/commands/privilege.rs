use crate::privilege;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn is_elevated() -> bool {
    privilege::is_elevated()
}

#[tauri::command]
pub async fn check_binary_present(state: State<'_, AppState>) -> Result<bool, String> {
    match crate::process::resolve_binary() {
        Ok(p) => {
            let mut g = state.proxy_binary.lock().await;
            *g = Some(p);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}
