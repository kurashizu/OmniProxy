use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, info};

mod admin;
mod bootstrap;
mod config;
mod mux;
mod socks5;
mod ws;

use crate::{config::Config, mux::Mux, socks5::handle};

#[tokio::main]
async fn main() -> Result<()> {
    bootstrap::init();

    let cfg = Config::load()?;

    let bind = format!("{}:{}", cfg.addr, cfg.port);
    let listener = TcpListener::bind(&bind).await
        .with_context(|| format!("failed to bind SOCKS5 on {bind}"))?;
    info!("SOCKS5 listening on {bind}");

    let mux = Mux::connect_mux(&cfg).await?;

    start_admin(mux.clone(), &cfg);

    info!("connected to {}", cfg.server);
    run_client(cfg, mux, listener).await
}

fn start_admin(mux: Arc<Mux>, cfg: &Config) {
    let port = cfg.admin_port;
    tokio::spawn(async move {
        admin::serve(mux, port).await;
    });
}

async fn run_client(cfg: Config, mux: Arc<Mux>, listener: TcpListener) -> Result<()> {
    info!("listening on {}:{} → {}", cfg.addr, cfg.port, cfg.server);
    loop {
        let (stream, peer) = listener.accept().await?;
        debug!(peer = %peer, "accepted");
        let mux = mux.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, mux).await {
                debug!(peer = %peer, error = %e, "connection error");
            }
        });
    }
}
