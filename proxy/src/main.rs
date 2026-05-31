mod admin;
mod config;
mod forwarder;
mod network;
mod stack;

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(windows)]
    disable_quick_edit_mode();

    info!("[main] initializing tracing");
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "proxy=info".into()),
        )
        .init();

    info!("[main] parsing CLI arguments");
    let cli = config::Cli::parse();

    info!("[main] loading config");
    let cfg = Arc::new(config::Config::load(&cli)?);

    let stats = admin::ProxyStats::new();
    tokio::spawn(admin::serve(stats.clone(), cfg.admin_port));

    info!("[main] entering main loop");

    let shutdown = wait_shutdown_signal();
    tokio::pin!(shutdown);

    tokio::select! {
        _ = run_loop(cfg, stats) => {}
        _ = shutdown => {
            info!("[main] received shutdown signal");
        }
    }

    info!("[main] exiting");
    Ok(())
}

async fn wait_shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
    }
}

#[cfg(windows)]
fn disable_quick_edit_mode() {
    use windows::Win32::System::Console::{
        ENABLE_QUICK_EDIT_MODE, GetConsoleMode, GetStdHandle, SetConsoleMode,
        STD_INPUT_HANDLE,
    };
    unsafe {
        if let Ok(handle) = GetStdHandle(STD_INPUT_HANDLE) {
            let mut mode = Default::default();
            if GetConsoleMode(handle, &mut mode).is_ok() {
                let _ = SetConsoleMode(handle, mode & !ENABLE_QUICK_EDIT_MODE);
            }
        }
    }
}

async fn run_loop(cfg: Arc<config::Config>, stats: Arc<admin::ProxyStats>) -> Result<()> {
    loop {
        info!("[main] detecting physical route");
        let outbound_ip = match network::detect_physical_route(&cfg) {
            Ok(ip) => ip,
            Err(e) => {
                warn!("[main] no physical route: {e:#}, retrying in 5s...");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        info!("[main] physical route detected: ip={}", outbound_ip);

        info!("[main] starting stack");
        let result = stack::run_stack(cfg.clone(), outbound_ip, stats.clone()).await;

        match result {
            Ok(()) => {
                info!("[main] stack exited normally");
            }
            Err(e) => {
                error!("[main] stack error: {e:#}");
            }
        }

        warn!("[main] stack exited, restarting in 3s...");
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
