use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use axum::{Router, routing::get, Json};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RouteEntry {
    pub destination: String,
    pub gateway: String,
    pub interface: String,
}

pub(crate) struct ProxyStats {
    pub started_at: Instant,
    pub client_alive: AtomicBool,
    pub client_pid: AtomicU32,
    pub tun_name: tokio::sync::RwLock<String>,
    pub tun_ip: tokio::sync::RwLock<String>,
    pub socks_port: tokio::sync::RwLock<u16>,
    pub routes: tokio::sync::RwLock<Vec<RouteEntry>>,
}

impl ProxyStats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started_at: Instant::now(),
            client_alive: AtomicBool::new(false),
            client_pid: AtomicU32::new(0),
            tun_name: tokio::sync::RwLock::new(String::new()),
            tun_ip: tokio::sync::RwLock::new(String::new()),
            socks_port: tokio::sync::RwLock::new(0),
            routes: tokio::sync::RwLock::new(Vec::new()),
        })
    }
}

pub(crate) async fn serve(stats: Arc<ProxyStats>, port: u16) {
    let app = Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats_handler))
        .route("/routes", get(routes_handler))
        .with_state(stats);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(port, error = %e, "admin server bind failed");
            return;
        }
    };
    tracing::info!("admin server listening on {addr}");
    axum::serve(listener, app).await.ok();
}

async fn health(axum::extract::State(s): axum::extract::State<Arc<ProxyStats>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "client_alive": s.client_alive.load(Ordering::Relaxed),
    }))
}

async fn stats_handler(axum::extract::State(s): axum::extract::State<Arc<ProxyStats>>) -> Json<serde_json::Value> {
    let uptime = s.started_at.elapsed().as_secs_f64();
    let tun_name = s.tun_name.read().await.clone();
    let tun_ip = s.tun_ip.read().await.clone();
    let socks_port = *s.socks_port.read().await;

    Json(serde_json::json!({
        "uptime_secs": uptime,
        "client": {
            "alive": s.client_alive.load(Ordering::Relaxed),
            "pid": s.client_pid.load(Ordering::Relaxed),
        },
        "tun": {
            "name": tun_name,
            "ip": tun_ip,
        },
        "socks_port": socks_port,
    }))
}

async fn routes_handler(axum::extract::State(s): axum::extract::State<Arc<ProxyStats>>) -> Json<serde_json::Value> {
    let routes = s.routes.read().await;
    Json(serde_json::json!({ "routes": *routes }))
}
