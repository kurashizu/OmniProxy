use anyhow::{Context, Result};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{watch, Mutex};

use crate::config::NodeConfig;
use crate::state::ProxyProcess;

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

/// Spawn the proxy process. Returns the live `Child` (caller — the waiter
/// task — takes ownership) plus a `ProxyProcess` handle carrying the
/// stop signal and the last-error buffer.
///
/// CRITICAL: the `Child` must NOT be stored behind a `Mutex` together with
/// anything that needs to call `kill()` / `start_kill()` on it; those
/// operations take `&mut Child` and would deadlock with `child.wait()`.
pub fn spawn_proxy(
    bin: &Path,
    args: &[String],
    id: u64,
    app: &tauri::AppHandle,
) -> Result<(Child, ProxyProcess)> {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: avoid popping a console window when spawning
        // the proxy process from the elevated GUI.
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn proxy at {}", bin.display()))?;

    // Assign to a Windows Job Object so the spawned proxy and (transitively)
    // everything it spawns — including the client.exe it forks internally —
    // dies when this GUI process dies or is hard-killed. Without this,
    // stopping the proxy via TerminateProcess leaves the client.exe
    // orphaned, holding port 1080, and the next start fails with
    // EADDRINUSE.
    #[cfg(windows)]
    {
        if let Some(handle) = child.raw_handle() {
            if let Err(e) = attach_to_kill_job(handle as _) {
                tracing::warn!(error = %e, "failed to attach proxy child to job object");
            }
        }
    }

    let pid = child.id().unwrap_or(0);
    let last_error = Arc::new(Mutex::new(None));
    let (stop_tx, _stop_rx) = watch::channel(false);

    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, "stdout", id, None, app);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, "stderr", id, Some(last_error.clone()), app);
    }

    tracing::info!(pid, "proxy spawned");
    let proc = ProxyProcess {
        id,
        started_at: Instant::now(),
        last_error,
        stop_tx,
    };
    Ok((child, proc))
}

#[cfg(windows)]
fn attach_to_kill_job(child_handle_raw: *mut std::ffi::c_void) -> Result<()> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
        JobObjectExtendedLimitInformation, JOBOBJECT_BASIC_LIMIT_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    unsafe {
        // Create a "kill on close" job. We never call CloseHandle; the
        // handle stays open for the lifetime of this process. When the
        // GUI exits, the OS closes all remaining handles and the
        // kernel kills every assigned process (proxy.exe, and transitively
        // everything proxy.exe spawned, like client.exe).
        let job: HANDLE = CreateJobObjectW(None, None)?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..Default::default()
            },
            ..Default::default()
        };
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )?;
        let child_handle = HANDLE(child_handle_raw as isize);
        AssignProcessToJobObject(job, child_handle)?;
        Ok(())
    }
}

fn spawn_log_reader<R>(
    reader: R,
    stream: &'static str,
    run_id: u64,
    last_error: Option<Arc<Mutex<Option<String>>>>,
    app: &tauri::AppHandle,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let app = app.clone();
    // Per-run log file. Path: <gui-exe-dir>/logs/proxy-<id>-<stream>.log.
    // Lets users open these in any text editor when the on-screen
    // Logs page is hard to read or the proxy is dumping too fast.
    let log_file = open_run_log_file(run_id, stream);
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
                    if let Some(file) = &log_file {
                        // best-effort — we never want logging to kill
                        // the reader if the disk is full
                        let mut f = file.lock().await;
                        let ts = chrono::Utc::now().to_rfc3339();
                        let _ = writeln!(f, "{ts} [{stream}] {line}");
                        let _ = f.flush();
                    }
                    let _ = app.emit(
                        "proxy-log",
                        serde_json::json!({
                            "ts_ms": chrono::Utc::now().timestamp_millis(),
                            "stream": stream,
                            "line": line,
                        }),
                    );

                    // Heuristic: surface common failure modes as a
                    // dedicated `proxy-error` event so the frontend can
                    // show a banner WITHOUT waiting for the child to
                    // exit. The proxy often keeps retrying after these
                    // errors, so the user would otherwise never see them
                    // until they manually clicked Stop.
                    if stream == "stderr" && looks_like_error(&line) {
                        let _ = app.emit(
                            "proxy-error",
                            serde_json::json!({
                                "ts_ms": chrono::Utc::now().timestamp_millis(),
                                "line": line,
                            }),
                        );
                    }
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

/// Best-effort open of `<logs>/proxy-<run_id>-<stream>.log` for append.
/// Returns None if the dir can't be located / created; the reader
/// continues with stdout / frontend-only logging in that case.
fn open_run_log_file(
    run_id: u64,
    stream: &'static str,
) -> Option<Arc<tokio::sync::Mutex<std::fs::File>>> {
    use std::fs::OpenOptions;
    let dir = crate::log_dir();
    let path = dir.join(format!("proxy-{run_id}-{stream}.log"));
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => Some(Arc::new(tokio::sync::Mutex::new(f))),
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "could not open per-run log file");
            None
        }
    }
}

/// Detect a real error/warning in a proxy stderr line.
///
/// Primary signal: the tracing log level. `tracing_subscriber::fmt()`
/// emits lines like
///
///     2026-06-03T11:16:41.123Z  INFO client::ws: tls handshake
///     2026-06-03T11:16:41.456Z  ERROR proxy::main: fatal error
///
/// so we look for ` ERROR ` / ` WARN ` as space-padded substrings —
/// this catches actual log-level errors and warnings without
/// false-matching INFO lines that happen to contain words like
/// "error" or "auth".
///
/// Fallback: a small set of specific tokens that should always be
/// treated as errors even if the upstream logger doesn't tag them
/// with a level (e.g. raw `eprintln!` output, panic backtraces).
fn looks_like_error(line: &str) -> bool {
    // Space-padded so we don't false-match tokens like "FATAL" inside
    // an identifier, or "ERROR" inside a JSON string.
    if line.contains(" ERROR ") || line.contains(" WARN ") {
        return true;
    }
    let lower = line.to_ascii_lowercase();
    const KW: &[&str] = &[
        "fatal error",
        "panic",
        "connection refused",
        "connection reset",
        "no such host",
        "timed out",
        "unauthorized",
        "forbidden",
        "tls handshake failed",
        "certificate verify",
        "wrong token",
    ];
    KW.iter().any(|k| lower.contains(k))
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
