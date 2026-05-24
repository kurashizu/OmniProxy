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

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "server", about = "WebSocket proxy server")]
struct Cli {
    /// Bind address
    #[arg(long, default_value = "0.0.0.0")]
    addr: String,

    /// Bind port
    #[arg(long, default_value = "9880")]
    port: u16,

    /// Auth token (clients must send X-Proxy-Token header)
    #[arg(long, default_value = "")]
    token: String,

    /// Path to YAML config file
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
            let cfg: Config =
                serde_yaml::from_str(&text).with_context(|| format!("parse config: {path}"))?;
            return Ok(cfg);
        }
        Ok(Config {
            addr: cli.addr,
            port: cli.port,
            token: cli.token,
        })
    }
}

// ── 应用状态 ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    cfg: Arc<Config>,
}

// ── 分片重组 ──────────────────────────────────────────────────────────────────

#[derive(Default)]
struct FragBuf {
    frags: Vec<Option<Bytes>>,
    received: u8,
    total: u8,
}

type FragMap = Arc<DashMap<(u16, u8), FragBuf>>;

fn reassemble(
    map: &FragMap,
    frag_id: u16,
    frag_no: u8,
    frag_total: u8,
    data: Bytes,
) -> Option<Bytes> {
    let key = (frag_id, frag_total);
    let mut entry = map.entry(key).or_insert_with(|| FragBuf {
        frags: vec![None; frag_total as usize],
        received: 0,
        total: frag_total,
    });
    if entry.frags[frag_no as usize].is_none() {
        entry.frags[frag_no as usize] = Some(data);
        entry.received += 1;
    }
    if entry.received == entry.total {
        let mut out = BytesMut::new();
        for f in entry.frags.iter() {
            out.extend_from_slice(f.as_ref().unwrap());
        }
        drop(entry);
        map.remove(&key);
        Some(out.freeze())
    } else {
        None
    }
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
        warn!("no auth token set — server is open to anyone");
    }

    let state = AppState { cfg };

    // axum 原生支持 HTTP/1.1 和 HTTP/2，cloudflared HTTP/2 转发完全兼容
    let app = Router::new().route("/", get(ws_handler)).with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    info!("ws server listening on {bind}");

    axum::serve(listener, app).await?;
    Ok(())
}

// ── WebSocket 入口 ────────────────────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // 鉴权
    if !state.cfg.token.is_empty() {
        let provided = headers
            .get("x-proxy-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != state.cfg.token {
            warn!("[auth] rejected: bad token");
            return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
        }
    }

    ws.on_upgrade(move |socket| handle_socket(socket))
}

async fn handle_socket(mut socket: WebSocket) {
    // 第一帧：控制帧
    // TCP: b'T' + "host:port"
    // UDP: b'U'
    let first = match socket.recv().await {
        Some(Ok(Message::Binary(data))) => data,
        _ => return,
    };

    if first.is_empty() {
        return;
    }

    match first[0] {
        b'T' => {
            let target = match std::str::from_utf8(&first[1..]) {
                Ok(s) => s.to_string(),
                Err(_) => return,
            };
            info!("[TCP→] {target}");
            handle_tcp(socket, target).await;
        }
        b'U' => {
            info!("[UDP] relay");
            handle_udp(socket).await;
        }
        _ => {
            warn!("[?] unknown mode byte: {:#x}", first[0]);
        }
    }
}

// ── TCP 透传 ──────────────────────────────────────────────────────────────────

async fn handle_tcp(socket: WebSocket, target: String) {
    let upstream = match TcpStream::connect(&target).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[TCP] connect {target}: {e}");
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
                    let data = Bytes::copy_from_slice(&buf[..n]);
                    if ws_tx.send(Message::Binary(data.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
        ws_tx.close().await.ok();
    };

    tokio::select! {
        _ = ws_to_tcp => {}
        _ = tcp_to_ws => {}
    }
}

// ── UDP 中继 + 分片重组 ────────────────────────────────────────────────────────

async fn handle_udp(socket: WebSocket) {
    // 每个 UDP 会话独立绑一个 socket
    let udp = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => Arc::new(s),
        Err(e) => {
            warn!("[UDP] bind: {e}");
            return;
        }
    };

    let (ws_tx, ws_rx) = socket.split();

    // 写 channel：合并 udp_to_ws 的写需求
    let (tx, mut rx) = mpsc::channel::<Bytes>(512);

    // writer task（独占 ws_tx）
    let mut ws_tx = ws_tx;
    let writer = tokio::spawn(async move {
        while let Some(pkt) = rx.recv().await {
            if ws_tx.send(Message::Binary(pkt.into())).await.is_err() {
                break;
            }
        }
        ws_tx.close().await.ok();
    });

    let frag_map: FragMap = Arc::new(DashMap::new());

    // ── WS → UDP（client 发来的数据→目标服务器）────────────────────────────────
    let udp_send = udp.clone();
    let frag_map_recv = frag_map.clone();
    let mut ws_rx = ws_rx;

    let ws_to_udp = async move {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    let (frag_id, frag_no, frag_total, host, port, payload) =
                        match decode_udp_frame(&data) {
                            Ok(v) => v,
                            Err(e) => {
                                warn!("[UDP] decode: {e}");
                                continue;
                            }
                        };

                    // 重组分片
                    let full = if frag_total == 1 {
                        payload
                    } else {
                        match reassemble(&frag_map_recv, frag_id, frag_no, frag_total, payload) {
                            Some(p) => p,
                            None => continue,
                        }
                    };

                    let target = format!("{host}:{port}");
                    if let Err(e) = udp_send.send_to(&full, &target).await {
                        warn!("[UDP] send to {target}: {e}");
                    } else {
                        debug!("[UDP→] {target} {}B", full.len());
                    }
                }
                Ok(Message::Close(_)) => break,
                _ => {}
            }
        }
    };

    // ── UDP 回包 → WS（目标服务器回包→client）────────────────────────────────
    let udp_recv = udp.clone();
    let tx_up = tx.clone();
    let mut frag_id_counter: u16 = 0;

    let udp_to_ws = async move {
        let mut buf = vec![0u8; 65535];
        const MAX_FRAG: usize = 60 * 1024;

        loop {
            let (n, src) = match udp_recv.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(e) => {
                    warn!("[UDP] recv: {e}");
                    break;
                }
            };

            let src_host = src.ip().to_string();
            let src_port = src.port();
            let payload = Bytes::copy_from_slice(&buf[..n]);

            debug!("[UDP←] {src} {}B", n);

            // 分片
            let chunks: Vec<Bytes> = if payload.len() <= MAX_FRAG {
                vec![payload]
            } else {
                payload
                    .chunks(MAX_FRAG)
                    .map(|c| Bytes::copy_from_slice(c))
                    .collect()
            };

            frag_id_counter = frag_id_counter.wrapping_add(1);
            let frag_id = frag_id_counter;
            let total = chunks.len() as u8;

            for (i, chunk) in chunks.into_iter().enumerate() {
                let pkt = encode_udp_frame(
                    0x02, // S2C
                    frag_id, i as u8, total, &src_host, src_port, &chunk,
                );
                if tx_up.send(pkt).await.is_err() {
                    break;
                }
            }
        }
    };

    tokio::select! {
        _ = ws_to_udp => {}
        _ = udp_to_ws => {}
    }

    writer.abort();
}

// ── 编解码（与 client 对称）──────────────────────────────────────────────────

fn encode_udp_frame(
    typ: u8,
    frag_id: u16,
    frag_no: u8,
    frag_total: u8,
    host: &str,
    port: u16,
    data: &[u8],
) -> Bytes {
    let host_bytes = host.as_bytes();
    let mut buf = BytesMut::with_capacity(7 + host_bytes.len() + 2 + data.len());
    buf.put_u8(typ);
    buf.put_u16(frag_id);
    buf.put_u8(frag_no);
    buf.put_u8(frag_total);
    buf.put_u16(host_bytes.len() as u16);
    buf.put_slice(host_bytes);
    buf.put_u16(port);
    buf.put_slice(data);
    buf.freeze()
}

fn decode_udp_frame(data: &[u8]) -> Result<(u16, u8, u8, String, u16, Bytes)> {
    if data.len() < 9 {
        anyhow::bail!("frame too short ({}B)", data.len());
    }
    // [0]: type, [1-2]: frag_id, [3]: frag_no, [4]: frag_total
    // [5-6]: host_len, [7..7+host_len]: host, [7+h..9+h]: port, rest: data
    let frag_id = u16::from_be_bytes([data[1], data[2]]);
    let frag_no = data[3];
    let frag_total = data[4];
    let host_len = u16::from_be_bytes([data[5], data[6]]) as usize;
    if data.len() < 7 + host_len + 2 {
        anyhow::bail!("frame truncated");
    }
    let host = String::from_utf8_lossy(&data[7..7 + host_len]).to_string();
    let port = u16::from_be_bytes([data[7 + host_len], data[8 + host_len]]);
    let payload = Bytes::copy_from_slice(&data[9 + host_len..]);
    Ok((frag_id, frag_no, frag_total, host, port, payload))
}

