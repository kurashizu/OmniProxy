use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{info, warn};

use futures_util::{sink::SinkExt, stream::StreamExt};

const BIND: &str = "0.0.0.0:9880";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new().route("/", get(ws_handler));

    let listener = tokio::net::TcpListener::bind(BIND).await.unwrap();
    info!("ws server listening on {BIND}");
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    let target = match socket.recv().await {
        Some(Ok(Message::Binary(data))) => match String::from_utf8(data.to_vec()) {
            Ok(s) => s,
            Err(e) => {
                warn!("bad target: {e}");
                return;
            }
        },
        Some(Ok(Message::Text(s))) => s.to_string(),
        other => {
            warn!("bad first frame: {other:?}");
            return;
        }
    };

    info!("[→] {target}");

    let upstream = match TcpStream::connect(&target).await {
        Ok(s) => s,
        Err(e) => {
            warn!("connect {target}: {e}");
            return;
        }
    };

    let (mut ur, mut uw) = tokio::io::split(upstream);
    let (mut ws_tx, mut ws_rx) = socket.split();

    let ws_to_tcp = async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Binary(data) => {
                    if uw.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        let _ = uw.shutdown().await;
    };

    let tcp_to_ws = async move {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            match ur.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if ws_tx
                        .send(Message::Binary(buf[..n].to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    };

    tokio::select! {
        _ = ws_to_tcp => {}
        _ = tcp_to_ws => {}
    }
}
