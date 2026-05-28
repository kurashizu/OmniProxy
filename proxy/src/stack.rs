use crate::config::Config;
use crate::network::PhysicalRoute;
use crate::route;
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::{Child, Command};
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
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await;
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

pub async fn run_stack(cfg: Arc<Config>, phys: PhysicalRoute) -> Result<()> {
    info!(
        "[stack] physical: iface={} ip={} gw={}",
        phys.iface, phys.ip, phys.gateway
    );

    let socks_addr = format!("127.0.0.1:{}", cfg.socks_port);
    let t2s_args = vec![
        "-device".to_string(),
        format!("tun://{}", cfg.tun_name),
        "-proxy".to_string(),
        format!("socks5://{socks_addr}"),
        "-loglevel".to_string(),
        "error".to_string(),
    ];
    let mut t2s = spawn(&cfg.tun2socks, &t2s_args, "tun2socks")
        .context("spawn tun2socks")?;
    info!("[stack] tun2socks started (pid {})", t2s.id().unwrap_or(0));

    let mut client_args = vec![
        "--server".to_string(),
        cfg.server.clone(),
        "--port".to_string(),
        cfg.socks_port.to_string(),
    ];
    if cfg.phys_iface.is_some() {
        client_args.push("--outbound-ip".to_string());
        client_args.push(phys.ip.to_string());
    }
    if !cfg.token.is_empty() {
        client_args.push("--token".to_string());
        client_args.push(cfg.token.clone());
    }
    let mut client = spawn(&cfg.client, &client_args, "client")
        .context("spawn client")?;
    info!("[stack] client started (pid {})", client.id().unwrap_or(0));

    tokio::time::sleep(Duration::from_millis(100)).await;

    route::tun_up(&cfg, &phys).await?;
    info!("[stack] TUN {} is up, routes configured", cfg.tun_name);

    let result = tokio::select! {
        s = t2s.wait() => { warn!("[stack] tun2socks exited: {:?}", s); s.map(|_|()) }
        s = client.wait() => { warn!("[stack] client exited: {:?}", s); s.map(|_|()) }
    };

    info!("[stack] tearing down...");
    kill_quiet(&mut t2s).await;
    kill_quiet(&mut client).await;
    route::tun_down(&cfg).await;
    info!("[stack] torn down");

    result.map_err(|e| anyhow::anyhow!("child wait error: {e}"))
}

#[cfg(unix)]
extern crate libc;
