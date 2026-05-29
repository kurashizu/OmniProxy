// Stack management: spawns tun2socks + client, manages lifecycle.

use crate::config::Config;
use crate::tun;
use anyhow::{Context, Result};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{timeout, Duration};
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
            let _ = timeout(Duration::from_secs(2), child.wait()).await;
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
    anyhow::bail!("SOCKS5 port {} did not become ready within {:?}", port, deadline);
}

async fn wait_for_tun(tun_name: &str, deadline: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        if tun::tun_exists(tun_name) {
            info!("[stack] TUN device {} is ready", tun_name);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    anyhow::bail!("TUN device {} did not appear within {:?}", tun_name, deadline);
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
    let mut client = spawn(&cfg.client, &client_args, "client")
        .context("spawn client")?;
    info!("[stack] client started (pid {})", client.id().unwrap_or(0));

    info!("[stack] waiting for SOCKS5 port to be ready");
    wait_for_socks(cfg.socks_port, Duration::from_secs(5))
        .await
        .context("wait for SOCKS5 ready")?;

    info!("[stack] spawning tun2socks");
    let socks_addr = format!("127.0.0.1:{}", cfg.socks_port);
    let t2s_args = vec![
        "-device".to_string(),
        cfg.tun_name.clone(),
        "-proxy".to_string(),
        format!("socks5://{socks_addr}"),
        "-loglevel".to_string(),
        "error".to_string(),
    ];
    let mut t2s = spawn(&cfg.tun2socks, &t2s_args, "tun2socks")
        .context("spawn tun2socks")?;
    info!("[stack] tun2socks started (pid {})", t2s.id().unwrap_or(0));

    info!("[stack] waiting for TUN device to appear");
    wait_for_tun(&cfg.tun_name, Duration::from_secs(10))
        .await
        .context("wait for TUN device")?;

    info!("[stack] bringing up TUN interface");
    tun::tun_up(&cfg).await?;
    info!("[stack] TUN {} is up, routes configured", cfg.tun_name);

    info!("[stack] proxy running");
    tokio::select! {
        s = t2s.wait() => { warn!("[stack] tun2socks exited: {:?}", s); }
        s = client.wait() => { warn!("[stack] client exited: {:?}", s); }
    };

    info!("[stack] tearing down");
    kill_quiet(&mut t2s).await;
    kill_quiet(&mut client).await;
    tun::tun_down(&cfg).await;
    info!("[stack] teardown complete");

    Ok(())
}

#[cfg(unix)]
extern crate libc;