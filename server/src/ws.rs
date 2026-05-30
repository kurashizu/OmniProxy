use axum::{
    extract::{State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use tracing::{info, warn};

use crate::config::AppState;
use crate::session::handle_socket;

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes()
        .iter()
        .zip(b.as_bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

pub(crate) async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let peer_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    if !state.cfg.token.is_empty() {
        let provided = headers
            .get("x-proxy-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let provided_len = provided.len();
        let expected_len = state.cfg.token.len();

        if provided.is_empty() {
            warn!(
                peer = peer_ip,
                ua = user_agent,
                "[auth] rejected: no token provided (expected {} chars)",
                expected_len
            );
            return (StatusCode::UNAUTHORIZED, "Unauthorized: missing token").into_response();
        }

        if provided_len != expected_len {
            warn!(
                peer = peer_ip,
                ua = user_agent,
                provided_len,
                expected_len,
                "[auth] rejected: token length mismatch"
            );
            return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
        }

        if !constant_time_eq(provided, &state.cfg.token) {
            warn!(
                peer = peer_ip,
                ua = user_agent,
                token_len = provided_len,
                "[auth] rejected: token mismatch"
            );
            return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
        }

        info!(
            peer = peer_ip,
            ua = user_agent,
            "[auth] accepted (token auth)"
        );
    } else {
        info!(
            peer = peer_ip,
            ua = user_agent,
            "[auth] accepted (no token configured, open access)"
        );
    }

    ws.on_upgrade(handle_socket)
}
