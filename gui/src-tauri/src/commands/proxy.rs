use crate::process::{resolve_binary, resolve_client_binary, spawn_proxy};
use crate::state::{AppState, ProxyState, ProxyStateKind};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

const STOP_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

#[tauri::command]
pub async fn start_proxy(app: AppHandle, state: State<'_, AppState>) -> Result<ProxyState, String> {
    // If a process is currently running OR being stopped, wait for it to
    // settle first. This makes Start idempotent under rapid clicking and
    // also lets a Stop be re-issued without racing the prior waiter.
    {
        let s = state.proxy_state.lock().await.clone();
        if matches!(s.state, ProxyStateKind::Running | ProxyStateKind::Stopping) {
            if let Some(proc) = state.child.lock().await.as_ref() {
                let _ = proc.stop_tx.send(true);
            }
            let deadline = std::time::Instant::now() + STOP_WAIT_TIMEOUT;
            loop {
                let s = state.proxy_state.lock().await.clone();
                if !matches!(s.state, ProxyStateKind::Running | ProxyStateKind::Stopping) {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    return Err("previous proxy did not stop in time".into());
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }

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
        return finish_with_error(&app, &state, "no active node configured").await;
    };
    if !node.enabled {
        return finish_with_error(&app, &state, "active node is disabled").await;
    }
    if let Err(e) = crate::config::validate(&node) {
        return finish_with_error(&app, &state, &format!("invalid config: {e}")).await;
    }

    let bin = match resolve_binary() {
        Ok(b) => b,
        Err(e) => return finish_with_error(&app, &state, &format!("{e:#}")).await,
    };
    *state.proxy_binary.lock().await = Some(bin.clone());

    let client_bin = match resolve_client_binary() {
        Ok(b) => b,
        Err(e) => return finish_with_error(&app, &state, &format!("{e:#}")).await,
    };

    let args = crate::process::build_cli_args(&node, &client_bin);
    let id = state.next_proxy_id.fetch_add(1, Ordering::SeqCst) + 1;
    let (mut child, proc) = match spawn_proxy(&bin, &args, id, &app) {
        Ok(p) => p,
        Err(e) => return finish_with_error(&app, &state, &format!("spawn failed: {e:#}")).await,
    };
    let pid = child.id().unwrap_or(0);
    let mut stop_rx = proc.stop_tx.subscribe();
    let last_error = proc.last_error.clone();

    *state.child.lock().await = Some(proc);

    // CRITICAL: if the user already asked to stop while we were spawning,
    // honor it now. The waiter would set state to Stopped at the end of its
    // run, racing with our Running transition; checking here makes the
    // intent explicit.
    {
        let mut s = state.proxy_state.lock().await;
        if matches!(s.state, ProxyStateKind::Stopping | ProxyStateKind::Stopped) {
            let _ = stop_rx; // drop so the channel knows we're not listening
            return Ok(s.clone());
        }
        s.state = ProxyStateKind::Running;
        s.pid = pid;
    }
    emit_state(&app, &state).await;

    // The waiter task owns the Child exclusively. stop_proxy / window
    // close signal it via the watch channel; on signal it kills the child
    // and waits for it to die. On natural exit it just waits. In both
    // cases it then transitions state to Stopped and clears the slot,
    // but only if its `id` still matches the slot — otherwise a newer
    // start_proxy has already taken over.
    let app_w = app.clone();
    let state_w = state.proxy_state.clone();
    let child_slot_w = state.child.clone();
    let id_w = id;
    tokio::spawn(async move {
        let exit = tokio::select! {
            res = child.wait() => res,
            _ = stop_rx.changed() => {
                // Stop requested — terminate the child, then wait for it.
                let _ = child.start_kill();
                child.wait().await
            }
        };

        // Only touch shared state if our run is still the current one.
        let still_current = child_slot_w
            .lock()
            .await
            .as_ref()
            .map(|p| p.id == id_w)
            .unwrap_or(false);

        if still_current {
            let mut s = state_w.lock().await;

            // CRITICAL: always transition to Stopped when the child has
            // actually died — including the case where stop_proxy has
            // already set state to Stopping. The previous guard
            // `if matches!(s.state, Running | Starting)` was wrong: when
            // user clicked Stop, stop_proxy had already set state to
            // Stopping, so the guard skipped the update and the UI was
            // stuck on "停止中…" forever.
            //
            // We capture the previous state to decide whether to surface
            // an error message (Stop = user-initiated, no error).
            let was_stopping = s.state == ProxyStateKind::Stopping;
            let code = exit.as_ref().ok().and_then(|e| e.code());
            let old_message = s.message.take();

            s.state = ProxyStateKind::Stopped;
            s.pid = 0;
            s.exit_code = code;

            if was_stopping {
                // User-initiated stop — don't surface a misleading error.
                s.message = None;
            } else if code == Some(0) {
                // Clean exit.
                s.message = None;
            } else {
                // Unexpected exit (crash / bad token / can't connect).
                // Prefer the last stderr line we captured; fall back to
                // the existing message; fall back to a generic format.
                let err = last_error.lock().await.clone();
                s.message = Some(err.unwrap_or_else(|| {
                    old_message.unwrap_or_else(|| {
                        format!("proxy exited with code {}", code.unwrap_or(-1))
                    })
                }));
            }
            drop(s);

            // Clear the slot only if it still points at us.
            let mut slot = child_slot_w.lock().await;
            if slot.as_ref().map(|p| p.id) == Some(id_w) {
                *slot = None;
            }
            drop(slot);

            let snapshot = state_w.lock().await.clone();
            let _ = app_w.emit("proxy-state", snapshot);
        } else {
            // A newer start_proxy has taken over. We were probably
            // triggered by its pre-start stop-signal; the new waiter
            // owns the slot now. Just exit.
            tracing::debug!(id, "waiter exiting: superseded by newer start_proxy");
        }
    });

    Ok(state.proxy_state.lock().await.clone())
}

#[tauri::command]
pub async fn stop_proxy(app: AppHandle, state: State<'_, AppState>) -> Result<ProxyState, String> {
    // Snapshot the stop signal first; if there's nothing to stop, return
    // current state immediately (don't flip to Stopping/Stopped on a noop).
    let stop_tx = state.child.lock().await.as_ref().map(|p| p.stop_tx.clone());

    let Some(stop_tx) = stop_tx else {
        return Ok(state.proxy_state.lock().await.clone());
    };

    {
        let mut s = state.proxy_state.lock().await;
        // If a stop is already in flight, just wait for it.
        if s.state == ProxyStateKind::Stopping {
            // fall through to wait
        } else if matches!(s.state, ProxyStateKind::Running | ProxyStateKind::Starting) {
            s.state = ProxyStateKind::Stopping;
            s.message = None;
            let _ = app.emit("proxy-state", s.clone());
        } else {
            return Ok(s.clone());
        }
    }

    let _ = stop_tx.send(true);

    // Wait for the waiter to actually transition the state to Stopped.
    let deadline = std::time::Instant::now() + STOP_WAIT_TIMEOUT;
    loop {
        let s = state.proxy_state.lock().await.clone();
        if s.state == ProxyStateKind::Stopped {
            return Ok(s);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(s);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
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

/// Common error path: flip state to Error with a message, emit, and return
/// the snapshot for the caller to forward to the frontend.
async fn finish_with_error(
    app: &AppHandle,
    state: &State<'_, AppState>,
    message: &str,
) -> Result<ProxyState, String> {
    let snapshot = {
        let mut s = state.proxy_state.lock().await;
        s.state = ProxyStateKind::Error;
        s.message = Some(message.to_string());
        s.clone()
    };
    let _ = app.emit("proxy-state", snapshot.clone());
    Ok(snapshot)
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

/// Returns the proxy's `/stats` JSON by fetching `http://127.0.0.1:<admin_port>/stats`
/// from Rust. We don't use the webview's `fetch()` because WebView2
/// has been observed to silently fail on loopback requests in some
/// configurations, leaving the UI with permanent placeholder state.
///
/// Returns `None` on any error (proxy not running, connection refused,
/// timeout, malformed JSON). The frontend treats `None` as "no data
/// yet" and falls back to default placeholders.
#[tauri::command]
pub async fn proxy_stats(state: State<'_, AppState>) -> Result<Option<serde_json::Value>, String> {
    if state.child.lock().await.is_none() {
        return Ok(None);
    }
    let cfg = state.config.lock().await.clone();
    let Some(node) = cfg.active_node().cloned() else {
        return Ok(None);
    };
    let host_port = format!("127.0.0.1:{}", node.admin_port);
    Ok(admin_fetch_json(&host_port, "/stats").await)
}

#[tauri::command]
pub async fn client_stats(state: State<'_, AppState>) -> Result<Option<serde_json::Value>, String> {
    if state.child.lock().await.is_none() {
        return Ok(None);
    }
    let cfg = state.config.lock().await.clone();
    let Some(node) = cfg.active_node().cloned() else {
        return Ok(None);
    };
    let port = node.admin_port.saturating_sub(1).max(1);
    let host_port = format!("127.0.0.1:{port}");
    Ok(admin_fetch_json(&host_port, "/stats").await)
}

#[tauri::command]
pub async fn proxy_routes(state: State<'_, AppState>) -> Result<Option<serde_json::Value>, String> {
    if state.child.lock().await.is_none() {
        return Ok(None);
    }
    let cfg = state.config.lock().await.clone();
    let Some(node) = cfg.active_node().cloned() else {
        return Ok(None);
    };
    let host_port = format!("127.0.0.1:{}", node.admin_port);
    // The proxy admin /routes endpoint returns `{"routes": [...]}` but the
    // frontend expects a bare `ProxyRoute[]` array. Unwrap the envelope here
    // so the frontend never has to know about the wrapper object.
    Ok(admin_fetch_json(&host_port, "/routes").await.and_then(|v| {
        v.get("routes").cloned()
    }))
}

// ---------------------------------------------------------------------------
// Minimal async HTTP/1.1 GET helper — uses tokio::net::TcpStream directly
// in the async Tauri command context (no spawn_blocking needed).
// ---------------------------------------------------------------------------

async fn admin_fetch_json(host_port: &str, path: &str) -> Option<serde_json::Value> {
    // Wrap the entire HTTP round-trip in a single deadline.  Only the TCP
    // connect was previously guarded; stalled writes or reads on a
    // half-open connection would block the Tauri command forever and leave
    // the UI permanently empty.  2 s is generous enough for the proxy admin
    // server to start up after clicking Start on a slow Windows machine,
    // yet short enough that a dead proxy is noticed within one poll cycle.
    let deadline = std::time::Duration::from_millis(2000);
    match tokio::time::timeout(deadline, admin_fetch_json_inner(host_port, path)).await {
        Ok(result) => result,
        Err(_elapsed) => {
            tracing::debug!(%host_port, %path, "admin_fetch_json: timed out");
            None
        }
    }
}

async fn admin_fetch_json_inner(host_port: &str, path: &str) -> Option<serde_json::Value> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    let addr = host_port.parse::<std::net::SocketAddr>().ok()?;
    let mut stream = tokio::net::TcpStream::connect(&addr).await.ok()?;

    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\nAccept: application/json\r\n\r\n"
    );
    if let Err(e) = stream.write_all(req.as_bytes()).await {
        tracing::warn!(%host_port, error = %e, "admin_fetch_json: write failed");
        return None;
    }

    let mut reader = BufReader::new(&mut stream);

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    let mut status_line: Option<String> = None;
    loop {
        let mut line = String::new();
        let read = match reader.read_line(&mut line).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(%host_port, error = %e, "admin_fetch_json: read header failed");
                return None;
            }
        };
        if read == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        let trimmed = line.trim();
        if status_line.is_none() {
            status_line = Some(trimmed.to_string());
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(len) = lower
            .strip_prefix("content-length:")
        {
            content_length = len.trim().parse::<usize>().ok();
        }
        if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            chunked = true;
        }
    }

    let status_ok = status_line
        .as_deref()
        .and_then(|l| l.split_ascii_whitespace().nth(1))
        .map(|c| c == "200")
        .unwrap_or(false);
    if !status_ok {
        tracing::warn!(%host_port, status = ?status_line, "admin_fetch_json: non-200 status");
        return None;
    }

    let body: Vec<u8> = if let Some(len) = content_length {
        let mut buf = vec![0u8; len];
        if let Err(e) = reader.read_exact(&mut buf).await {
            tracing::warn!(%host_port, error = %e, "admin_fetch_json: read body failed");
            return None;
        }
        buf
    } else if chunked {
        // Decode HTTP/1.1 chunked transfer encoding manually.
        // Each chunk: "<hex-size>\r\n<data>\r\n", terminated by "0\r\n\r\n".
        let mut body = Vec::new();
        loop {
            let mut size_line = String::new();
            match reader.read_line(&mut size_line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            // Strip optional chunk extensions after ";"
            let hex = size_line.trim().split(';').next().unwrap_or("");
            let chunk_size = usize::from_str_radix(hex, 16).unwrap_or(0);
            if chunk_size == 0 {
                break;
            }
            let mut chunk = vec![0u8; chunk_size];
            if reader.read_exact(&mut chunk).await.is_err() {
                break;
            }
            body.extend_from_slice(&chunk);
            // Consume trailing \r\n after the chunk data.
            let mut crlf = [0u8; 2];
            let _ = reader.read_exact(&mut crlf).await;
        }
        body
    } else {
        // No Content-Length and not chunked: read until connection closes.
        // This is the fallback path; axum always sends Content-Length for
        // small JSON payloads, so we should rarely end up here.
        let mut buf = Vec::new();
        if let Err(e) = reader.read_to_end(&mut buf).await {
            tracing::warn!(%host_port, error = %e, "admin_fetch_json: read body (close-delimited) failed");
            return None;
        }
        buf
    };

    match serde_json::from_slice(&body) {
        Ok(v) => Some(v),
        Err(e) => {
            let preview = String::from_utf8_lossy(&body[..body.len().min(200)]);
            tracing::warn!(%host_port, error = %e, body = %preview, "admin_fetch_json: json parse failed");
            None
        }
    }
}
