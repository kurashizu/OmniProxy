mod config;
mod session;
mod tcp;
mod udp;
mod ws;

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use tracing::info;
use tracing_subscriber::EnvFilter;

use config::{AppState, Config};

#[tokio::main]
async fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "server=info".into());

    #[cfg(feature = "console")]
    {
        use tracing_subscriber::prelude::*;
        let (console_layer, server) = console_subscriber::ConsoleLayer::new();
        tokio::spawn(server.serve());
        tracing_subscriber::registry()
            .with(env_filter)
            .with(console_layer)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    #[cfg(not(feature = "console"))]
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .init();

    let cfg = Config::load()?;
    let bind = format!("{}:{}", cfg.addr, cfg.port);

    if cfg.token.is_empty() {
        tracing::warn!("no auth token — server is open to anyone");
    }

    let state = AppState {
        cfg: std::sync::Arc::new(cfg),
    };
    let app = Router::new().route("/", get(ws::handler)).with_state(state);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    info!("ws server on {bind}");
    axum::serve(listener, app).await?;
    Ok(())
}
