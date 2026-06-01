use crate::privilege;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn is_elevated() -> bool {
    privilege::is_elevated()
}

#[tauri::command]
pub fn check_binary_present(state: State<'_, AppState>) -> Result<bool, String> {
    match crate::process::resolve_binary() {
        Ok(p) => {
            // cache
            let bin = state.proxy_binary.clone();
            tokio::spawn(async move {
                let mut g = bin.lock().await;
                *g = Some(p);
            });
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}
