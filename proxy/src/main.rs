use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{error, info, warn};

mod network;
mod process;
mod route;

use network::PhysicalRoute;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "proxy", about = "Transparent proxy stack manager")]
struct Cli {
    #[arg(long, short = 'c')]
    config: Option<PathBuf>,

    #[arg(long)]
    client: Option<PathBuf>,
    #[arg(long)]
    tun2socks: Option<PathBuf>,
    #[arg(long)]
    server: Option<String>,
    #[arg(long)]
    token: Option<String>,
    #[arg(long)]
    socks_port: Option<u16>,
    #[arg(long)]
    tun_name: Option<String>,
    #[arg(long)]
    tun_ip: Option<String>,
    #[arg(long)]
    tun_ip6: Option<String>,
    #[arg(long)]
    tun_prefix: Option<u8>,
    #[arg(long)]
    tun_prefix6: Option<u8>,
    #[arg(long)]
    tun_gw: Option<String>,
    #[arg(long)]
    tun_gw6: Option<String>,
    #[arg(long)]
    phys_iface: Option<String>,
}

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub client: PathBuf,
    #[serde(default)]
    pub tun2socks: PathBuf,
    pub server: String,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default = "default_tun_name")]
    pub tun_name: String,
    #[serde(default = "default_tun_ip")]
    pub tun_ip: String,
    #[serde(default = "default_tun_ip6")]
    pub tun_ip6: String,
    #[serde(default = "default_tun_prefix")]
    pub tun_prefix: u8,
    #[serde(default = "default_tun_prefix6")]
    pub tun_prefix6: u8,
    #[serde(default = "default_tun_gw")]
    pub tun_gw: String,
    #[serde(default = "default_tun_gw6")]
    pub tun_gw6: String,
    #[serde(default)]
    pub phys_iface: Option<String>,
}

fn default_socks_port() -> u16 {
    1080
}
#[cfg(target_os = "macos")]
fn default_tun_name() -> String {
    "utun0".into()
}
#[cfg(not(target_os = "macos"))]
fn default_tun_name() -> String {
    "tun0".into()
}
fn default_tun_ip() -> String {
    "198.18.0.1".into()
}
fn default_tun_ip6() -> String {
    "fd00::1".into()
}
fn default_tun_prefix() -> u8 {
    16
}
fn default_tun_prefix6() -> u8 {
    64
}
fn default_tun_gw() -> String {
    "198.18.0.2".into()
}
fn default_tun_gw6() -> String {
    "fd00::2".into()
}

impl Config {
    fn load(cli: &Cli) -> Result<Self> {
        let mut cfg: Config = if let Some(ref path) = cli.config {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read config: {}", path.display()))?;
            serde_yaml::from_str(&text)
                .with_context(|| format!("parse config: {}", path.display()))?
        } else {
            Config {
                client: cli.client.clone().unwrap_or_default(),
                tun2socks: cli.tun2socks.clone().unwrap_or_default(),
                server: cli
                    .server
                    .clone()
                    .context("--server or -c config is required")?,
                token: cli.token.clone().unwrap_or_default(),
                socks_port: cli.socks_port.unwrap_or_else(default_socks_port),
                tun_name: cli.tun_name.clone().unwrap_or_else(default_tun_name),
                tun_ip: cli.tun_ip.clone().unwrap_or_else(default_tun_ip),
                tun_ip6: cli.tun_ip6.clone().unwrap_or_else(default_tun_ip6),
                tun_prefix: cli.tun_prefix.unwrap_or_else(default_tun_prefix),
                tun_prefix6: cli.tun_prefix6.unwrap_or_else(default_tun_prefix6),
                tun_gw: cli.tun_gw.clone().unwrap_or_else(default_tun_gw),
                tun_gw6: cli.tun_gw6.clone().unwrap_or_else(default_tun_gw6),
                phys_iface: cli.phys_iface.clone(),
            }
        };

        if let Some(ref v) = cli.client {
            cfg.client = v.clone();
        }
        if let Some(ref v) = cli.tun2socks {
            cfg.tun2socks = v.clone();
        }
        if let Some(ref v) = cli.server {
            cfg.server = v.clone();
        }
        if let Some(ref v) = cli.token {
            cfg.token = v.clone();
        }
        if let Some(v) = cli.socks_port {
            cfg.socks_port = v;
        }
        if let Some(ref v) = cli.tun_name {
            cfg.tun_name = v.clone();
        }
        if let Some(ref v) = cli.tun_ip {
            cfg.tun_ip = v.clone();
        }
        if let Some(ref v) = cli.tun_ip6 {
            cfg.tun_ip6 = v.clone();
        }
        if let Some(v) = cli.tun_prefix {
            cfg.tun_prefix = v;
        }
        if let Some(v) = cli.tun_prefix6 {
            cfg.tun_prefix6 = v;
        }
        if let Some(ref v) = cli.tun_gw {
            cfg.tun_gw = v.clone();
        }
        if let Some(ref v) = cli.tun_gw6 {
            cfg.tun_gw6 = v.clone();
        }
        if let Some(ref v) = cli.phys_iface {
            cfg.phys_iface = Some(v.clone());
        }

        // Automatically complete relative path mappings based on working platform
        let self_dir = std::env::current_exe()
            .map(|p| p.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or(None)
            .unwrap_or_else(|| PathBuf::from("."));

        #[cfg(windows)]
        let (ext_client, ext_t2s) = ("client.exe", "tun2socks.exe");
        #[cfg(not(windows))]
        let (ext_client, ext_t2s) = ("client", "tun2socks");

        if cfg.client.as_os_str().is_empty() {
            cfg.client = self_dir.join(ext_client);
        }
        if cfg.tun2socks.as_os_str().is_empty() {
            cfg.tun2socks = self_dir.join(ext_t2s);
        }

        Ok(cfg)
    }
}

// ── Stack ─────────────────────────────────────────────────────────────────────

async fn run_stack(cfg: Arc<Config>, phys: PhysicalRoute) -> Result<()> {
    info!(
        "[stack] physical: iface={} ip={} gw={}",
        phys.iface, phys.ip, phys.gateway
    );

    // ── 1. Start tun2socks FIRST ─────────────────────────────────────────────
    let socks_addr = format!("127.0.0.1:{}", cfg.socks_port);
    let t2s_args = vec![
        "-device".to_string(),
        format!("tun://{}", cfg.tun_name),
        "-proxy".to_string(),
        format!("socks5://{socks_addr}"),
        "-loglevel".to_string(),
        "error".to_string(), // Adjusted: tuned level to error to avoid noisy refuse loops
    ];
    let mut t2s = process::spawn(&cfg.tun2socks, &t2s_args, "tun2socks")?;
    info!("[stack] tun2socks started (pid {})", t2s.id().unwrap_or(0));

    // ── 2. Start client SECOND ───────────────────────────────────────────────
    // Spin up the core backend proxy outbound before making network routing mutations
    let mut client_args = vec![
        "--server".to_string(),
        cfg.server.clone(),
        "--port".to_string(),
        cfg.socks_port.to_string(),
    ];
    // 如果指定了物理网卡 IP（多用于 Windows），则绑定出站 IP，避免流量进入 TUN 回环
    if cfg.phys_iface.is_some() {
        client_args.push("--outbound-ip".to_string());
        client_args.push(phys.ip.to_string());
    }
    if !cfg.token.is_empty() {
        client_args.push("--token".to_string());
        client_args.push(cfg.token.clone());
    }
    let mut client = process::spawn(&cfg.client, &client_args, "client")?;
    info!("[stack] client started (pid {})", client.id().unwrap_or(0));

    // Allow a 100ms socket binding breather room to clear race condition
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ── 3. Bring up TUN + Configure Routes LAST ──────────────────────────────
    route::tun_up(&cfg, &phys).await?;
    info!("[stack] TUN {} is up, routes configured", cfg.tun_name);

    // ── 4. Wait for either child to exit ─────────────────────────────────────
    let result = tokio::select! {
        s = t2s.wait()    => { warn!("[stack] tun2socks exited: {:?}", s); s.map(|_|()) }
        s = client.wait() => { warn!("[stack] client exited: {:?}", s);    s.map(|_|()) }
    };

    // ── 5. Tear down ──────────────────────────────────────────────────────────
    info!("[stack] tearing down...");
    process::kill_quiet(&mut t2s).await;
    process::kill_quiet(&mut client).await;
    route::tun_down(&cfg).await;
    info!("[stack] torn down");

    result.map_err(|e| anyhow::anyhow!("child wait error: {e}"))
}

// ── Main loop ─────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "proxy=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = Arc::new(Config::load(&cli)?);

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    {
        let tx = shutdown_tx.clone();
        tokio::spawn(async move {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigterm = signal(SignalKind::terminate()).expect("sigterm");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = sigterm.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                tokio::signal::ctrl_c().await.ok();
            }
            info!("[proxy] shutdown signal received");
            tx.send(true).ok();
        });
    }

    let (net_tx, mut net_rx) = watch::channel(());
    {
        let cfg2 = cfg.clone();
        tokio::spawn(async move {
            network::watch_changes(cfg2, net_tx).await;
        });
    }

    loop {
        if *shutdown_rx.borrow() {
            info!("[proxy] exiting");
            break;
        }

        let phys = match network::detect_physical_route(&cfg) {
            Ok(p) => p,
            Err(e) => {
                warn!("[proxy] no physical route: {e:#}, retrying in 5s...");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    _ = shutdown_rx.changed() => { break; }
                }
                continue;
            }
        };

        info!("[proxy] starting stack (phys ip={})...", phys.ip);

        let stack_task = {
            let cfg = cfg.clone();
            let phys = phys.clone();
            tokio::spawn(async move { run_stack(cfg, phys).await })
        };

        tokio::select! {
            r = stack_task => {
                match r {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => error!("[proxy] stack error: {e:#}"),
                    Err(e)     => error!("[proxy] stack task panic: {e}"),
                }
                if *shutdown_rx.borrow() { break; }
                warn!("[proxy] stack exited, restarting in 3s...");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                    _ = shutdown_rx.changed() => { break; }
                }
            }
            _ = net_rx.changed() => {
                if *shutdown_rx.borrow() { break; }
                info!("[proxy] network changed, tearing down and rebuilding...");
                route::tun_down(&cfg).await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            _ = shutdown_rx.changed() => {
                route::tun_down(&cfg).await;
                break;
            }
        }
    }

    Ok(())
}
