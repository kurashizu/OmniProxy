pub mod unix;
pub mod windows;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::config::NodeConfig;
use crate::state::ProxyProcess;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Locates a bundled binary (`proxy` or `client`) by searching:
///
/// 1. `<gui-exe-dir>/<name>`                    — manually staged next to exe
/// 2. `<gui-exe-dir>/resources/<name>`          — Tauri MSI/NSIS bundle.resources
/// 3. `<cwd>/../../target/release/<name>`       — dev fallback
fn find_binary(name: &str) -> Result<PathBuf> {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let filename = format!("{name}{ext}");

    if let Ok(gui_exe) = std::env::current_exe()
        && let Some(dir) = gui_exe.parent()
    {
        let p = dir.join(&filename);
        if p.is_file() {
            return Ok(p);
        }
        let p = dir.join("resources").join(&filename);
        if p.is_file() {
            return Ok(p);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join("..").join("..").join("target").join("release").join(&filename);
        if p.is_file() {
            return Ok(p);
        }
    }

    Err(anyhow::anyhow!(
        "binary {filename} not found: looked next to GUI exe, in <exe-dir>/resources/, and at <cwd>/../../target/release/"
    ))
}

/// Locates the `proxy` binary (calls `find_binary("proxy")`).
pub fn resolve_binary() -> Result<PathBuf> {
    find_binary("proxy")
}

/// Locates the `client` binary (calls `find_binary("client")`).
pub fn resolve_client_binary() -> Result<PathBuf> {
    find_binary("client")
}

/// Convert a `NodeConfig` + client binary path into the proxy CLI argument vector.
pub fn build_cli_args(node: &NodeConfig, client_path: &Path) -> Vec<String> {
    let mut args = vec![
        "--client".to_string(),
        client_path.display().to_string(),
        "--server".to_string(),
        node.server.clone(),
        "--socks-port".to_string(),
        node.socks_port.to_string(),
        "--admin-port".to_string(),
        node.admin_port.to_string(),
        "--tun-name".to_string(),
        node.tun_name.clone(),
        "--tun-ip".to_string(),
        node.tun_ip.clone(),
        "--tun-ip6".to_string(),
        node.tun_ip6.clone(),
        "--tun-prefix".to_string(),
        node.tun_prefix.to_string(),
        "--tun-prefix6".to_string(),
        node.tun_prefix6.to_string(),
        "--tun-gw".to_string(),
        node.tun_gw.clone(),
        "--tun-gw6".to_string(),
        node.tun_gw6.clone(),
    ];
    if !node.token.is_empty() {
        args.push("--token".to_string());
        args.push(node.token.clone());
    }
    if let Some(ip) = &node.phys_ip
        && !ip.is_empty()
    {
        args.push("--phys-ip".to_string());
        args.push(ip.clone());
    }
    args
}

/// Spawn the proxy process. Returns a `ProxyProcess` containing the child
/// handle and a shared buffer holding the last stderr line (for errors).
pub fn spawn_proxy(bin: &Path, args: &[String], app: &tauri::AppHandle) -> Result<ProxyProcess> {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: avoid popping a console window when spawning the
        // proxy process from the elevated GUI.
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn proxy at {}", bin.display()))?;

    let pid = child.id().unwrap_or(0);
    let last_error = Arc::new(Mutex::new(None));

    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, "stdout", app, None);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, "stderr", app, Some(last_error.clone()));
    }

    tracing::info!(pid, "proxy spawned");
    Ok(ProxyProcess {
        child,
        started_at: std::time::Instant::now(),
        last_error,
    })
}

fn spawn_log_reader<R>(
    reader: R,
    stream: &'static str,
    app: &tauri::AppHandle,
    last_error: Option<Arc<Mutex<Option<String>>>>,
)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let app = app.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let line = truncate_line(&line, 8 * 1024);
                    // Keep a copy of the last stderr line so the waiter
                    // can surface it as a user-facing error message.
                    if let Some(ref buf) = last_error
                        && stream == "stderr"
                    {
                        let mut g = buf.lock().await;
                        *g = Some(line.clone());
                    }
                    let _ = app.emit(
                        "proxy-log",
                        serde_json::json!({
                            "ts_ms": chrono::Utc::now().timestamp_millis(),
                            "stream": stream,
                            "line": line,
                        }),
                    );
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::debug!(error = %e, "log reader error");
                    break;
                }
            }
        }
    });
}

fn truncate_line(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // keep last `max` bytes by char boundary
        let mut idx = s.len().saturating_sub(max);
        while !s.is_char_boundary(idx) && idx < s.len() {
            idx += 1;
        }
        format!("…{}", &s[idx..])
    }
}

/// Kill a child process, escalating from SIGTERM to SIGKILL if needed.
pub async fn kill_child(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            if tokio::time::timeout(std::time::Duration::from_secs(2), child.wait())
                .await
                .is_ok()
            {
                return;
            }
        }
    }

    #[cfg(not(unix))]
    {
        // On Windows there is no graceful signal; just terminate.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await;
    }

    if let Err(e) = child.kill().await
        && e.kind() != std::io::ErrorKind::InvalidInput
    {
        tracing::warn!(error = %e, "child kill");
    }
    if let Err(e) = child.wait().await {
        tracing::debug!(error = %e, "child wait");
    }
}
