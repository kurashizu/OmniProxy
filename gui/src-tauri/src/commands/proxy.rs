use crate::state::{AppState, ProxyState, ProxyStateKind};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn start_proxy(app: AppHandle, state: State<'_, AppState>) -> Result<ProxyState, String> {
    {
        let mut s = state.proxy_state.lock().await;
        if matches!(s.state, ProxyStateKind::Starting | ProxyStateKind::Running) {
            return Err("proxy is already starting or running".into());
        }
        s.state = ProxyStateKind::Starting;
        s.pid = 0;
        s.exit_code = None;
        s.message = None;
    }
    emit_state(&app, &state).await;

    let node = {
        let cfg = state.config.lock().await;
        cfg.active_node().cloned()
    };
    let Some(node) = node else {
        let mut s = state.proxy_state.lock().await;
        s.state = ProxyStateKind::Error;
        s.message = Some("no active node configured".into());
        emit_state(&app, &state).await;
        return Ok(s.clone());
    };
    if !node.enabled {
        let mut s = state.proxy_state.lock().await;
        s.state = ProxyStateKind::Error;
        s.message = Some("active node is disabled".into());
        emit_state(&app, &state).await;
        return Ok(s.clone());
    }

    if let Err(e) = crate::config::validate(&node) {
        let mut s = state.proxy_state.lock().await;
        s.state = ProxyStateKind::Error;
        s.message = Some(format!("invalid config: {e}"));
        emit_state(&app, &state).await;
        return Ok(s.clone());
    }

    let bin = match crate::process::resolve_binary() {
        Ok(b) => b,
        Err(e) => {
            let mut s = state.proxy_state.lock().await;
            s.state = ProxyStateKind::Error;
            s.message = Some(format!("{e:#}"));
            emit_state(&app, &state).await;
            return Ok(s.clone());
        }
    };
    {
        let mut p = state.proxy_binary.lock().await;
        *p = Some(bin.clone());
    }

    let args = crate::process::build_cli_args(&node);
    let child = match crate::process::spawn_proxy(&bin, &args, &app) {
        Ok(c) => c,
        Err(e) => {
            let mut s = state.proxy_state.lock().await;
            s.state = ProxyStateKind::Error;
            s.message = Some(format!("spawn failed: {e:#}"));
            emit_state(&app, &state).await;
            return Ok(s.clone());
        }
    };
    let pid = child.id().unwrap_or(0);
    {
        let mut s = state.proxy_state.lock().await;
        s.state = ProxyStateKind::Running;
        s.pid = pid;
    }
    {
        let mut child_slot = state.child.lock().await;
        *child_slot = Some(crate::state::ProxyProcess {
            child,
            started_at: std::time::Instant::now(),
        });
    }
    emit_state(&app, &state).await;

    // Spawn a waiter task that detects child exit and resets state.
    let app_for_waiter = app.clone();
    let state_arc = state.proxy_state.clone();
    let child_arc = state.child.clone();
    tokio::spawn(async move {
        let mut guard = child_arc.lock().await;
        let Some(proc) = guard.as_mut() else {
            return;
        };
        let child = &mut proc.child;
        let exit = child.wait().await;
        let mut s = state_arc.lock().await;
        // Only update if we were running (not stopped intentionally)
        if matches!(s.state, ProxyStateKind::Running | ProxyStateKind::Starting) {
            s.state = ProxyStateKind::Stopped;
            s.pid = 0;
            s.exit_code = exit.as_ref().ok().and_then(|e| e.code());
            s.message = exit.err().map(|e| e.to_string());
        }
        drop(s);
        let mut guard = child_arc.lock().await;
        *guard = None;
        drop(guard);
        let snapshot = state_arc.lock().await.clone();
        let _ = app_for_waiter.emit("proxy-state", snapshot);
    });

    Ok(state.proxy_state.lock().await.clone())
}

#[tauri::command]
pub async fn stop_proxy(app: AppHandle, state: State<'_, AppState>) -> Result<ProxyState, String> {
    {
        let mut s = state.proxy_state.lock().await;
        s.state = ProxyStateKind::Stopping;
        s.message = None;
    }
    emit_state(&app, &state).await;

    let mut child_guard = state.child.lock().await;
    if let Some(mut proc) = child_guard.take() {
        crate::process::kill_child(&mut proc.child).await;
    }
    drop(child_guard);

    let mut s = state.proxy_state.lock().await;
    s.state = ProxyStateKind::Stopped;
    s.pid = 0;
    s.message = None;
    let snapshot = s.clone();
    drop(s);
    let _ = app.emit("proxy-state", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
pub async fn proxy_status(state: State<'_, AppState>) -> Result<ProxyState, String> {
    Ok(state.proxy_state.lock().await.clone())
}

#[tauri::command]
pub async fn proxy_binary_path(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let p = state.proxy_binary.lock().await.clone();
    Ok(p.map(|p| p.display().to_string()))
}

async fn emit_state(app: &AppHandle, state: &State<'_, AppState>) {
    let snapshot = state.proxy_state.lock().await.clone();
    let _ = app.emit("proxy-state", snapshot);
}

/// Helper: build the proxy admin URL from a node config.
pub fn proxy_admin_url(node: &crate::config::NodeConfig) -> String {
    format!("http://127.0.0.1:{}", node.admin_port)
}

/// Helper: build the client admin URL (admin_port - 1).
pub fn client_admin_url(node: &crate::config::NodeConfig) -> String {
    let p = node.admin_port.saturating_sub(1).max(1);
    format!("http://127.0.0.1:{p}")
}

/// Helper used by the `urls` module.
pub fn _ensure_path(p: PathBuf) -> PathBuf {
    p
}
