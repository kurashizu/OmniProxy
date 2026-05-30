use std::sync::Arc;
use axum::{Router, routing::get, Json};

use crate::mux::Mux;

pub(crate) async fn serve(mux: Arc<Mux>, port: u16) {
    let app = Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats_handler))
        .with_state(mux);

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

async fn health(axum::extract::State(mux): axum::extract::State<Arc<Mux>>) -> Json<serde_json::Value> {
    let s = mux.stats();
    Json(serde_json::json!({
        "status": "ok",
        "connected": s.ws_connected.load(std::sync::atomic::Ordering::Relaxed),
    }))
}

async fn stats_handler(axum::extract::State(mux): axum::extract::State<Arc<Mux>>) -> Json<serde_json::Value> {
    use std::sync::atomic::Ordering;
    let s = mux.stats();
    let inner = mux.inner().read().await;
    let uptime = s.started_at.elapsed().as_secs_f64();

    Json(serde_json::json!({
        "connected": s.ws_connected.load(Ordering::Relaxed),
        "uptime_secs": uptime,
        "reconnect_count": s.reconnect_count.load(Ordering::Relaxed),
        "active": {
            "tcp": inner.stream_count(),
            "udp": inner.udp_count(),
            "icmp": inner.icmp_count(),
        },
        "bytes": {
            "tx": s.bytes_tx.load(Ordering::Relaxed),
            "rx": s.bytes_rx.load(Ordering::Relaxed),
        },
        "socks5": format!("{}:{}", mux.config().addr, mux.config().port),
        "server": mux.config().server,
        "connections": inner.connections_snapshot().into_iter().map(|c| {
            let elapsed = c.started_at.elapsed().as_secs_f64();
            serde_json::json!({
                "id": c.id,
                "protocol": c.protocol,
                "target": c.target,
                "source": c.source,
                "duration_secs": elapsed,
            })
        }).collect::<Vec<_>>(),
    }))
}
