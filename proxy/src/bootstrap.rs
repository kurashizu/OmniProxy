use crate::config::Config;
use crate::network;
use crate::route;
use crate::stack;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;
use tracing::{error, info, warn};

pub async fn run(cfg: Config) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "proxy=info".into()),
        )
        .init();

    let cfg = Arc::new(cfg);

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    {
        let tx = shutdown_tx.clone();
        tokio::spawn(async move {
            let mut sigterm = signal(SignalKind::terminate()).expect("sigterm");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
            info!("[bootstrap] shutdown signal received");
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
            info!("[bootstrap] exiting");
            break;
        }

        let phys = match network::detect_physical_route(&cfg) {
            Ok(p) => p,
            Err(e) => {
                warn!("[bootstrap] no physical route: {e:#}, retrying in 5s...");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    _ = shutdown_rx.changed() => { break; }
                }
                continue;
            }
        };

        info!("[bootstrap] starting stack (phys ip={})...", phys.ip);

        let stack_task = {
            let cfg = cfg.clone();
            let phys = phys.clone();
            tokio::spawn(async move { stack::run_stack(cfg, phys).await })
        };

        tokio::select! {
            r = stack_task => {
                match r {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => error!("[bootstrap] stack error: {e:#}"),
                    Err(e)     => error!("[bootstrap] stack task panic: {e:#}"),
                }
                if *shutdown_rx.borrow() { break; }
                warn!("[bootstrap] stack exited, restarting in 3s...");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                    _ = shutdown_rx.changed() => { break; }
                }
            }
            _ = net_rx.changed() => {
                if *shutdown_rx.borrow() { break; }
                info!("[bootstrap] network changed, tearing down and rebuilding...");
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
