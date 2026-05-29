use axum::{
    extract::{ws::WebSocketUpgrade, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use tracing::warn;

use crate::config::AppState;
use crate::session::handle_socket;

pub(crate) async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !state.cfg.token.is_empty() {
        let provided = headers
            .get("x-proxy-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != state.cfg.token {
            warn!("[auth] rejected");
            return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
        }
    }
    ws.on_upgrade(handle_socket)
}
