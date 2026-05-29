// Stack management: spawns client + forwarder, manages lifecycle.

use crate::config::Config;
use crate::forwarder;
use crate::forwarder::Forwarder;
use anyhow::{Context, Result};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::Duration;
use tracing::{debug, info, warn};

pub fn spawn(bin: &std::path::Path, args: &[String], label: &str) -> Result<Child> {
    let child = Command::new(bin)
        .args(args)
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
    if let Err(e) = child.kill().await {
        if e.kind() != std::io::ErrorKind::InvalidInput {
            warn!("[stack] kill: {e}");
        }
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

pub async fn run_stack(cfg: Arc<Config>, outbound_ip: IpAddr) -> Result<()> {
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
    client_args.extend(["--token".to_string(), cfg.token.clone()]);
    let mut client = spawn(&cfg.client, &client_args, "client").context("spawn client")?;
    info!("[stack] client started (pid {})", client.id().unwrap_or(0));

    info!("[stack] waiting for SOCKS5 port to be ready");
    wait_for_socks(cfg.socks_port, Duration::from_secs(5))
        .await
        .context("wait for SOCKS5 ready")?;

    info!("[stack] creating TUN device and configuring routes");
    let tun_dev = forwarder::tun_up(&cfg).context("tun_up")?;

    info!("[stack] creating forwarder");
    let mut fwd = Forwarder::new(tun_dev, cfg.socks_port)
        .context("create forwarder")?;

    info!("[stack] proxy running");
    tokio::select! {
        s = fwd.run() => { warn!("[stack] forwarder exited: {:?}", s); }
        s = client.wait() => { warn!("[stack] client exited: {:?}", s); }
    };

    info!("[stack] tearing down");
    fwd.shutdown();
    kill_quiet(&mut client).await;
    forwarder::tun_down(&cfg);
    info!("[stack] teardown complete");

    Ok(())
}
