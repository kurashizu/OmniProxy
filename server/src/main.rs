use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use bytes::{BufMut, Bytes, BytesMut};
use clap::Parser;
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

// ── 帧协议 ────────────────────────────────────────────────────────────────────

const TYPE_TCP_CONNECT: u8 = 0x01;
const TYPE_TCP_CONNECTED: u8 = 0x02;
const TYPE_TCP_DATA: u8 = 0x03;
const TYPE_TCP_FIN: u8 = 0x04;
const TYPE_UDP_DATA: u8 = 0x05;

const UDP_STREAM_ID: u32 = 0;

fn encode_frame(stream_id: u32, typ: u8, payload: &[u8]) -> Bytes {
    let mut buf = BytesMut::with_capacity(5 + payload.len());
    buf.put_u32(stream_id);
    buf.put_u8(typ);
    buf.put_slice(payload);
    buf.freeze()
}

fn decode_frame(data: &[u8]) -> Result<(u32, u8, Bytes)> {
    if data.len() < 5 {
        anyhow::bail!("frame too short: {}B", data.len());
    }
    let stream_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let typ = data[4];
    let payload = Bytes::copy_from_slice(&data[5..]);
    Ok((stream_id, typ, payload))
}

fn encode_udp_payload(host: &str, port: u16, data: &[u8]) -> Bytes {
    let hb = host.as_bytes();
    let mut buf = BytesMut::with_capacity(4 + hb.len() + data.len());
    buf.put_u16(hb.len() as u16);
    buf.put_slice(hb);
    buf.put_u16(port);
    buf.put_slice(data);
    buf.freeze()
}

fn decode_udp_payload(payload: &[u8]) -> Result<(String, u16, Bytes)> {
    if payload.len() < 4 {
        anyhow::bail!("udp payload too short");
    }
    let hl = u16::from_be_bytes([payload[0], payload[1]]) as usize;
    if payload.len() < 2 + hl + 2 {
        anyhow::bail!("udp payload truncated");
    }
    let host = String::from_utf8_lossy(&payload[2..2 + hl]).to_string();
    let port = u16::from_be_bytes([payload[2 + hl], payload[3 + hl]]);
    let data = Bytes::copy_from_slice(&payload[4 + hl..]);
    Ok((host, port, data))
}

// ── CLI / Config ──────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "server")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0")]
    addr: String,
    #[arg(long, default_value = "9880")]
    port: u16,
    #[arg(long, default_value = "")]
    token: String,
    #[arg(long)]
    config: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    #[serde(default = "default_addr")]
    addr: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    token: String,
}

fn default_addr() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    9880
}

impl Config {
    fn from_cli(cli: Cli) -> Result<Self> {
        if let Some(path) = cli.config {
            let text =
                std::fs::read_to_string(&path).with_context(|| format!("read config: {path}"))?;
            return Ok(
                serde_yaml::from_str(&text).with_context(|| format!("parse config: {path}"))?
            );
        }
        Ok(Config {
            addr: cli.addr,
            port: cli.port,
            token: cli.token,
        })
    }
}

#[derive(Clone)]
struct AppState {
    cfg: Arc<Config>,
}

// ── 入口 ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = Arc::new(Config::from_cli(cli)?);
    let bind = format!("{}:{}", cfg.addr, cfg.port);

    if cfg.token.is_empty() {
        warn!("no auth token — server is open to anyone");
    }

    let state = AppState { cfg };
    let app = Router::new().route("/", get(ws_handler)).with_state(state);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    info!("ws server on {bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

// ── WebSocket 入口 ────────────────────────────────────────────────────────────

async fn ws_handler(
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

// ── 核心：单 WS 会话的多路复用处理 ───────────────────────────────────────────

async fn handle_socket(socket: WebSocket) {
    let (ws_tx, mut ws_rx) = socket.split();

    // 所有流汇聚到一个出帧 channel
    let (frame_tx, mut frame_rx) = mpsc::channel::<Bytes>(1024);

    // writer task
    let mut ws_tx = ws_tx;
    tokio::spawn(async move {
        while let Some(frame) = frame_rx.recv().await {
            if ws_tx.send(Message::Binary(frame.into())).await.is_err() {
                break;
            }
        }
        ws_tx.close().await.ok();
    });

    // stream_map: stream_id → 该流的上行数据发送端
    // 每条 TCP 流注册后，TCP_DATA 帧可以路由进去
    let stream_map: Arc<DashMap<u32, mpsc::Sender<Bytes>>> = Arc::new(DashMap::new());

    // UDP socket（整个 WS 会话共享）
    let udp_sock: Arc<UdpSocket> = match UdpSocket::bind("[::]:0").await {
        Ok(s) => Arc::new(s),
        Err(_) => match UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => Arc::new(s),
            Err(e) => {
                warn!("[udp] bind: {e}");
                return;
            }
        },
    };

    // UDP 回包 task
    {
        let udp_recv = udp_sock.clone();
        let ftx = frame_tx.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            loop {
                let (n, src) = match udp_recv.recv_from(&mut buf).await {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("[udp] recv: {e}");
                        break;
                    }
                };
                let src_host = src.ip().to_string();
                let src_port = src.port();
                let payload = encode_udp_payload(&src_host, src_port, &buf[..n]);
                let frame = encode_frame(UDP_STREAM_ID, TYPE_UDP_DATA, &payload);
                if ftx.send(frame).await.is_err() {
                    break;
                }
            }
        });
    }

    // 主分发循环
    while let Some(msg) = ws_rx.next().await {
        match msg {
            Ok(Message::Binary(data)) => {
                let (stream_id, typ, payload) = match decode_frame(&data) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("[mux] decode: {e}");
                        continue;
                    }
                };

                match typ {
                    // ── 新 TCP 流 ─────────────────────────────────────────────
                    TYPE_TCP_CONNECT => {
                        let target = String::from_utf8_lossy(&payload).to_string();
                        debug!("[TCP→] {target} sid={stream_id}");

                        // 每条 TCP 流一个上行 channel
                        let (up_tx, up_rx) = mpsc::channel::<Bytes>(64);
                        stream_map.insert(stream_id, up_tx);

                        let ftx = frame_tx.clone();
                        let sm = stream_map.clone();
                        tokio::spawn(async move {
                            run_tcp_stream(stream_id, target, up_rx, ftx).await;
                            sm.remove(&stream_id);
                        });
                    }

                    // ── TCP 上行数据 ──────────────────────────────────────────
                    TYPE_TCP_DATA => {
                        if let Some(tx) = stream_map.get(&stream_id) {
                            tx.send(payload).await.ok();
                        }
                    }

                    // ── TCP FIN（client 关闭写端）────────────────────────────
                    TYPE_TCP_FIN => {
                        stream_map.remove(&stream_id);
                    }

                    // ── UDP ──────────────────────────────────────────────────
                    TYPE_UDP_DATA => match decode_udp_payload(&payload) {
                        Ok((host, port, data)) => {
                            let target = format!("{host}:{port}");
                            let sock = udp_sock.clone();
                            tokio::spawn(async move {
                                if let Err(e) = sock.send_to(&data, &target).await {
                                    warn!("[udp] send to {target}: {e}");
                                } else {
                                    debug!("[UDP→] {target} {}B", data.len());
                                }
                            });
                        }
                        Err(e) => warn!("[udp] decode: {e}"),
                    },

                    _ => warn!("[mux] unknown type {typ:#x} sid={stream_id}"),
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                warn!("[ws] {e}");
                break;
            }
            _ => {}
        }
    }

    // WS 断开，清理所有流
    stream_map.clear();
    info!("[ws] session closed");
}

// ── TCP 流的完整生命周期 ──────────────────────────────────────────────────────

async fn run_tcp_stream(
    stream_id: u32,
    target: String,
    mut up_rx: mpsc::Receiver<Bytes>, // client → upstream
    frame_tx: mpsc::Sender<Bytes>,    // upstream → client
) {
    // 1. 连接上游
    let upstream = match TcpStream::connect(&target).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[TCP] connect {target}: {e}");
            let msg = format!("{e}");
            frame_tx
                .send(encode_frame(stream_id, TYPE_TCP_CONNECTED, msg.as_bytes()))
                .await
                .ok();
            return;
        }
    };

    // 2. 通知 client 连接成功
    if frame_tx
        .send(encode_frame(stream_id, TYPE_TCP_CONNECTED, &[]))
        .await
        .is_err()
    {
        return;
    }

    let (mut ur, mut uw) = tokio::io::split(upstream);

    // 3. 上游 → client（下行）
    let ftx = frame_tx.clone();
    let down = tokio::spawn(async move {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            match ur.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let frame = encode_frame(stream_id, TYPE_TCP_DATA, &buf[..n]);
                    if ftx.send(frame).await.is_err() {
                        break;
                    }
                }
            }
        }
        ftx.send(encode_frame(stream_id, TYPE_TCP_FIN, &[]))
            .await
            .ok();
    });

    // 4. client → 上游（上行）
    let up = tokio::spawn(async move {
        while let Some(data) = up_rx.recv().await {
            if uw.write_all(&data).await.is_err() {
                break;
            }
        }
        uw.shutdown().await.ok();
    });

    // 任意一端关闭则终止
    tokio::select! {
        _ = down => {}
        _ = up   => {}
    }
}
