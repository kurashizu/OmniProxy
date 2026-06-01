use axum::{
    extract::{FromRequestParts, State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode, header, Request},
    response::{Html, IntoResponse, Response},
};
use tracing::{info, warn};

use crate::config::AppState;
use crate::session::handle_socket;

const INDEX_HTML: &str = include_str!("web.html");

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
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
) -> Response {
    let is_ws = headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if !is_ws {
        return Html(INDEX_HTML).into_response();
    }

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

    let (mut parts, _body) = request.into_parts();
    let state_for_socket = state.clone();
    match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(ws) => ws.on_upgrade(move |socket| handle_socket(socket, state_for_socket)),
        Err(rejection) => rejection.into_response(),
    }
}
