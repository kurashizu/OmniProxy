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

    info!("[main] entering main loop");

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
        let result = stack::run_stack(cfg.clone(), outbound_ip).await;

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
