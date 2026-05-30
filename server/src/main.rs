mod config;
mod icmp;
mod session;
mod tcp;
mod udp;
mod ws;

use anyhow::Result;
use axum::Router;
use axum::routing::get;
use tracing::info;
use tracing_subscriber::EnvFilter;

use config::{AppState, Config};

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(unix)]
    raise_nofile_limit();

    #[cfg(feature = "console")]
    {
        use tracing_subscriber::prelude::*;
        let (console_layer, server) = console_subscriber::ConsoleLayer::new();
        tokio::spawn(server.serve());
        let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "server=info".into());
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_filter(env_filter))
            .with(console_layer)
            .init();
    }

    #[cfg(not(feature = "console"))]
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "server=info".into()))
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

#[cfg(unix)]
fn raise_nofile_limit() {
    unsafe {
        let mut rl = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut rl) == 0 {
            let target = rl.rlim_max.min(65535);
            if rl.rlim_cur < target {
                rl.rlim_cur = target;
                libc::setrlimit(libc::RLIMIT_NOFILE, &rl);
            }
        }
    }
}
