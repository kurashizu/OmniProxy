mod config;
mod icmp;
mod session;
mod tcp;
mod udp;
mod ws;

use anyhow::Result;
use axum::Router;
use axum::routing::get;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use config::{AppState, Config};

const IPIFY_V4: &str = "https://api.ipify.org?format=json";
const IPIFY_V6: &str = "https://api64.ipify.org?format=json";

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

    let outbound = Arc::new(RwLock::new(protocol::ServerInfoPayload::default()));
    spawn_outbound_refresher(outbound.clone());

    let state = AppState {
        cfg: Arc::new(cfg),
        outbound,
    };
    let app = Router::new().route("/", get(ws::handler)).with_state(state);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    info!("ws server on {bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn spawn_outbound_refresher(outbound: Arc<RwLock<protocol::ServerInfoPayload>>) {
    tokio::spawn(async move {
        let mut backoff_secs: u64 = 30;
        loop {
            match fetch_outbound().await {
                Ok(info) => {
                    *outbound.write().await = info;
                    backoff_secs = 30;
                }
                Err(e) => {
                    warn!("outbound ip lookup failed: {e:#}");
                    backoff_secs = (backoff_secs * 2).min(1800);
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
        }
    });
}

async fn fetch_outbound() -> Result<protocol::ServerInfoPayload> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let mut info = protocol::ServerInfoPayload::default();
    if let Ok(r) = client.get(IPIFY_V4).send().await
        && let Ok(v) = r.json::<serde_json::Value>().await
        && let Some(s) = v.get("ip").and_then(|x| x.as_str())
    {
        info.outbound_ipv4 = Some(s.to_string());
    }
    if let Ok(r) = client.get(IPIFY_V6).send().await
        && let Ok(v) = r.json::<serde_json::Value>().await
        && let Some(s) = v.get("ip").and_then(|x| x.as_str())
        && s.contains(':')
    {
        info.outbound_ipv6 = Some(s.to_string());
    }
    Ok(info)
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
