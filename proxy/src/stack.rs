// Stack management: spawns client + forwarder, manages lifecycle.

use crate::admin::ProxyStats;
use crate::config::Config;
use crate::forwarder;
use crate::forwarder::Forwarder;
use anyhow::{Context, Result};
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::Duration;
use tracing::{debug, info, warn};

pub fn spawn(bin: &std::path::Path, args: &[String], label: &str) -> Result<Child> {
    let child = Command::new(bin)
        .args(args)
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!("spawn {label} ({bin:?}): {e}"))?;

    // CRITICAL: assign the spawned child to a Windows Job Object that
    // kills its members when the job handle is closed. The proxy
    // (us) is spawned with `kill_on_drop`, but that only fires when
    // the Child struct is dropped — and when WE are killed by
    // TerminateProcess from the GUI, our Child struct is never
    // dropped. Without the Job Object, the spawned child (client.exe)
    // would survive our death, hold its ports (e.g. 1080), and
    // EADDRINUSE on the next start.
    #[cfg(windows)]
    {
        if let Some(handle) = child.raw_handle() {
            if let Err(e) = assign_to_kill_job(handle as _) {
                // Non-fatal: log and continue. The spawn itself succeeded;
                // we'll just lack the orphan-cleanup guarantee.
                warn!("[stack] failed to attach child to job object: {e}");
            }
        }
    }

    Ok(child)
}

#[cfg(windows)]
fn assign_to_kill_job(child_handle_raw: *mut std::ffi::c_void) -> Result<()> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
        JobObjectExtendedLimitInformation, JOBOBJECT_BASIC_LIMIT_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    unsafe {
        // Create a "kill on close" job. We never call CloseHandle on
        // the resulting HANDLE; it stays open for the lifetime of this
        // proxy process. When the proxy exits, the OS closes all
        // remaining handles and the kernel kills every assigned
        // process. HANDLE is a plain isize (Copy) so we drop the
        // binding here — that's fine, the kernel resource is
        // independent of the Rust value.
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

pub async fn kill_quiet(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
    }
    if let Err(e) = child.kill().await
        && e.kind() != std::io::ErrorKind::InvalidInput
    {
        warn!("[stack] kill: {e}");
    }
    if let Err(e) = child.wait().await {
        debug!("[stack] wait: {e}");
    }
}

/// Returns the captured last-error line of the client process (a
/// short string the user can read), if the process has died while we
/// were waiting for the SOCKS5 port. `last_error` is a shared buffer
/// that the stderr-relay task is filling in real time.
async fn check_client_death(
    client: &mut Child,
    last_error: &Arc<tokio::sync::Mutex<Option<String>>>,
) -> Option<String> {
    match client.try_wait() {
        Ok(Some(status)) => {
            let code = status.code().unwrap_or(-1);
            let captured = last_error.lock().await.clone();
            let reason = captured.unwrap_or_else(|| match code {
                10 => "authentication failed: proxy token was rejected by the server".to_string(),
                11 => "server unreachable: could not connect to the proxy server".to_string(),
                _ => format!("client exited with code {code} before SOCKS5 was ready"),
            });
            Some(reason)
        }
        _ => None,
    }
}

async fn wait_for_socks(
    port: u16,
    deadline: Duration,
    client: &mut Child,
    last_error: &Arc<tokio::sync::Mutex<Option<String>>>,
) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let start = std::time::Instant::now();
    let mut poll_ms = 100u64;
    while start.elapsed() < deadline {
        if let Some(reason) = check_client_death(client, last_error).await {
            // Surface the actual cause of the failure, not a generic
            // "did not become ready". On a bad server the client exits
            // almost immediately with a useful message; on a good server
            // it lives forever and the loop terminates by SOCKS5 probe.
            anyhow::bail!("{reason}");
        }
        if TcpStream::connect(&addr).await.is_ok() {
            info!("[stack] SOCKS5 port {} is ready", port);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(poll_ms)).await;
        poll_ms = (poll_ms * 3 / 2).min(500);
    }
    // One last death check at the deadline — if the client died in
    // the final window we still want a precise error.
    if let Some(reason) = check_client_death(client, last_error).await {
        anyhow::bail!("{reason}");
    }
    anyhow::bail!(
        "SOCKS5 port {} did not become ready within {:?} (proxy is running but the client never started listening)",
        port, deadline
    );
}

pub async fn run_stack(
    cfg: Arc<Config>,
    outbound_ip: IpAddr,
    stats: Arc<ProxyStats>,
) -> Result<()> {
    info!("[stack] outbound IP: {}", outbound_ip);

    info!("[stack] spawning client");
    let mut client_args = vec![
        "--server".to_string(),
        cfg.server.clone(),
        "--port".to_string(),
        cfg.socks_port.to_string(),
    ];
    client_args.push("--outbound-ip".to_string());
    client_args.push(outbound_ip.to_string());
    client_args.push("--admin-port".to_string());
    client_args.push(cfg.admin_port.saturating_sub(1).to_string());
    client_args.extend(["--token".to_string(), cfg.token.clone()]);
    let mut client = spawn(&cfg.client, &client_args, "client").context("spawn client")?;
    let pid = client.id().unwrap_or(0);
    info!("[stack] client started (pid {})", pid);

    // Relay client stderr → proxy stderr so the GUI can capture error
    // messages. IMPORTANT: we use eprintln! (→ stderr) rather than
    // warn!() (→ tracing, which the proxy's `tracing_subscriber::fmt()`
    // writes to stdout by default). The GUI's spawn_log_reader only
    // updates `last_error` from stderr lines; routing client errors via
    // tracing would lose them in the "proxy exited with code N" fallback.
    //
    // We also keep the last line in a shared buffer so the SOCKS5
    // readiness loop can surface the client's actual error instead
    // of a generic "did not become ready" timeout.
    let last_error: Arc<tokio::sync::Mutex<Option<String>>> =
        Arc::new(tokio::sync::Mutex::new(None));
    if let Some(stderr) = client.stderr.take() {
        let last_error_for_reader = last_error.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            loop {
                match reader.next_line().await {
                    Ok(Some(line)) => {
                        {
                            let mut g = last_error_for_reader.lock().await;
                            *g = Some(line.clone());
                        }
                        eprintln!("[client] {line}");
                    }
                    Ok(None) => break,
                    Err(e) => {
                        debug!("[stack] client stderr reader: {e}");
                        break;
                    }
                }
            }
        });
    }

    stats.client_alive.store(true, Ordering::Relaxed);
    stats.client_pid.store(pid, Ordering::Relaxed);
    *stats.socks_port.write().await = cfg.socks_port;
    *stats.tun_name.write().await = cfg.tun_name.clone();
    *stats.tun_ip.write().await = cfg.tun_ip.clone();

    info!("[stack] waiting for SOCKS5 port to be ready");
    wait_for_socks(
        cfg.socks_port,
        Duration::from_secs(8),
        &mut client,
        &last_error,
    )
    .await
    .context("wait for SOCKS5 ready")?;

    // Inner restart loop: forwarder is restarted on non-critical exits.
    // Critical TUN↔stack errors break out to the caller (run_loop handles full restart).
    let max_restarts = 10u32;
    let mut restarts = 0u32;

    loop {
        info!("[stack] creating TUN device and configuring routes");
        let tun_dev = forwarder::tun_up(&cfg).context("tun_up")?;

        let routes = vec![
            crate::admin::RouteEntry {
                destination: "0.0.0.0/0".into(),
                gateway: cfg.tun_gw.clone(),
                interface: cfg.tun_name.clone(),
            },
            crate::admin::RouteEntry {
                destination: "::/0".into(),
                gateway: cfg.tun_gw6.clone(),
                interface: cfg.tun_name.clone(),
            },
        ];
        *stats.routes.write().await = routes;

        info!("[stack] creating forwarder");
        let mut fwd = Forwarder::new(tun_dev, cfg.socks_port).context("create forwarder")?;

        info!("[stack] proxy running");
        let fwd_result = tokio::select! {
            s = fwd.run() => { s }
            s = client.wait() => {
                let status = s?;
                let code = status.code().unwrap_or(-1);
                let msg = match code {
                    10 => "authentication failed: proxy token was rejected by the server",
                    11 => "server unreachable: could not connect to the proxy server",
                    _ => "client exited unexpectedly",
                };
                warn!("[stack] client exited (code {code}): {msg}");
                fwd.shutdown();
                forwarder::tun_down(&cfg);
                anyhow::bail!("{msg}");
            }
        };

        fwd.shutdown();
        forwarder::tun_down(&cfg);

        match fwd_result {
            Ok(()) => {
                // Subtask exit (non-critical) — restart forwarder
                restarts += 1;
                if restarts >= max_restarts {
                    anyhow::bail!(
                        "[stack] forwarder restarted {} times, giving up",
                        max_restarts
                    );
                }
                warn!(
                    "[stack] forwarder exited (restart {}/{})",
                    restarts, max_restarts
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
            Err(e) => {
                // Critical error — propagate to caller
                anyhow::bail!("[stack] forwarder critical error: {e}");
            }
        }
    }
}
