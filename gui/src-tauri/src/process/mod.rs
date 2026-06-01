pub mod unix;
pub mod windows;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::config::NodeConfig;

/// Locates the `proxy` binary.
///
/// Resolution order:
/// 1. `<gui-exe-dir>/proxy[.exe]`
/// 2. `<cwd>/../../target/release/proxy[.exe]` (dev fallback)
pub fn resolve_binary() -> Result<PathBuf> {
    let exe_name = if cfg!(windows) { "proxy.exe" } else { "proxy" };

    if let Ok(gui_exe) = std::env::current_exe()
        && let Some(dir) = gui_exe.parent()
    {
        let p = dir.join(exe_name);
        if p.is_file() {
            return Ok(p);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd
            .join("..")
            .join("..")
            .join("target")
            .join("release")
            .join(exe_name);
        if p.is_file() {
            return Ok(p);
        }
    }

    Err(anyhow::anyhow!(
        "proxy binary not found: looked for {} next to GUI exe and at <cwd>/../../target/release/",
        exe_name
    ))
}

/// Convert a `NodeConfig` into the proxy CLI argument vector.
pub fn build_cli_args(node: &NodeConfig) -> Vec<String> {
    let mut args = vec![
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

/// Spawn the proxy process. Returns the running child and the kill handle.
pub fn spawn_proxy(bin: &Path, args: &[String], app: &tauri::AppHandle) -> Result<Child> {
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

    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, "stdout", app);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, "stderr", app);
    }

    tracing::info!(pid, "proxy spawned");
    Ok(child)
}

fn spawn_log_reader<R>(reader: R, stream: &'static str, app: &tauri::AppHandle)
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
