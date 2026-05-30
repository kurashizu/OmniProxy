mod config;
mod session;
mod tcp;
mod udp;
mod ws;

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use tracing::info;
#[cfg(not(feature = "console"))]
use tracing_subscriber::EnvFilter;

use config::{AppState, Config};

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(feature = "console")]
    console_subscriber::init();

    #[cfg(not(feature = "console"))]
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "server=info".into()),
        )
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
