use anyhow::Result;
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
    // 1) Runtime init.
    bootstrap::init();

    // 2) Load CLI / config file.
    let cfg = Config::load()?;

    // 3) Connect to remote, establish mux, and start reconnect loop.
    let mux = Mux::connect_mux(&cfg).await?;

    // 4) Start admin HTTP server.
    start_admin(mux.clone(), &cfg);

    // 5) Listen on local SOCKS5 and forward requests via mux.
    run_client(cfg, mux).await
}

fn start_admin(mux: Arc<Mux>, cfg: &Config) {
    let port = cfg.admin_port;
    tokio::spawn(async move {
        admin::serve(mux, port).await;
    });
}

async fn run_client(cfg: Config, mux: Arc<Mux>) -> Result<()> {
    // Local SOCKS5 listen address.
    let bind = format!("{}:{}", cfg.addr, cfg.port);

    // Accept local client connections, each handled independently.
    let listener = TcpListener::bind(&bind).await?;
    info!("listening on {bind} → {}", cfg.server);
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
