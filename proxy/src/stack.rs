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
    Ok(child)
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

async fn wait_for_socks(port: u16, deadline: Duration) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        if TcpStream::connect(&addr).await.is_ok() {
            info!("[stack] SOCKS5 port {} is ready", port);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    anyhow::bail!(
        "SOCKS5 port {} did not become ready within {:?}",
        port,
        deadline
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

    // Relay client stderr → proxy stderr so the GUI can capture error messages.
    if let Some(stderr) = client.stderr.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            loop {
                match reader.next_line().await {
                    Ok(Some(line)) => {
                        warn!("[client] {line}");
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
    wait_for_socks(cfg.socks_port, Duration::from_secs(15))
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
