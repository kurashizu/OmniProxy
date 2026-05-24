use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use bytes::{BufMut, Bytes, BytesMut};
use clap::Parser;
use dashmap::DashMap;
use fast_socks5::server::{AuthMethodSuccessState, NoAuthentication, Socks5ServerProtocol};
use fast_socks5::util::target_addr::TargetAddr;
use fast_socks5::{ReplyError, Socks5Command};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "client", about = "SOCKS5 proxy client over WebSocket")]
struct Cli {
    /// Bind address
    #[arg(long, default_value = "127.0.0.1")]
    addr: String,

    /// Bind port
    #[arg(long, default_value = "1080")]
    port: u16,

    /// Auth token (must match server)
    #[arg(long, default_value = "")]
    token: String,

    /// Server WebSocket URL (e.g. tunnel-oracle.022025.xyz)
    #[arg(long, default_value = "")]
    server: String,

    /// Path to YAML config file (overrides other flags if provided)
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
    #[serde(default)]
    server: String,
}

fn default_addr() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    1080
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
        if cli.server.is_empty() {
            anyhow::bail!("--server is required (or set via --config)");
        }
        Ok(Config {
            addr: cli.addr,
            port: cli.port,
            token: cli.token,
            server: cli.server,
        })
    }

    fn ws_url(&self) -> String {
        let host = &self.server;
        if host.starts_with("ws://") || host.starts_with("wss://") {
            host.clone()
        } else {
            format!("wss://{host}")
        }
    }
}

// ── WS 帧格式 ─────────────────────────────────────────────────────────────────
//
// 第一帧（控制）:
//   TCP模式: b"T" + host_bytes + b":" + port_str
//   UDP模式: b"U"
//
// 后续帧（TCP）: 原始字节
//
// 后续帧（UDP）:
//   [1B type][2B frag_id][1B frag_no][1B frag_total]
//   [2B host_len][host][2B port][data]
//   type: 0x01=client→server, 0x02=server→client

const UDP_C2S: u8 = 0x01;

// ── UDP 分片重组 ───────────────────────────────────────────────────────────────

#[derive(Default)]
struct FragBuf {
    frags: Vec<Option<Bytes>>,
    received: u8,
    total: u8,
}

type FragMap = Arc<DashMap<(u16, u8), FragBuf>>; // key: (frag_id, total)

/// 尝试拼装分片，返回完整 payload（若尚未齐全返回 None）
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
                .unwrap_or_else(|_| "client=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = Arc::new(Config::from_cli(cli)?);

    let bind = format!("{}:{}", cfg.addr, cfg.port);
    let listener = TcpListener::bind(&bind).await?;
    info!("socks5 on {bind}  →  {}", cfg.ws_url());

    loop {
        let (stream, peer) = listener.accept().await?;
        debug!("[+] {peer}");
        let cfg = cfg.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, cfg).await {
                warn!("[!] {peer}: {e:#}");
            }
        });
    }
}

// ── 连接处理 ──────────────────────────────────────────────────────────────────

async fn handle(stream: tokio::net::TcpStream, cfg: Arc<Config>) -> Result<()> {
    let proto = Socks5ServerProtocol::start(stream);
    let no_auth = proto
        .negotiate_auth(&[NoAuthentication])
        .await
        .map_err(|e| anyhow::anyhow!("negotiate_auth: {e}"))?;
    let proto = no_auth.finish_auth();
    let (proto, cmd, target_addr) = proto
        .read_command()
        .await
        .map_err(|e| anyhow::anyhow!("read_command: {e}"))?;

    match cmd {
        Socks5Command::TCPConnect => handle_tcp(proto, target_addr, cfg).await,
        Socks5Command::UDPAssociate => handle_udp_associate(proto, cfg).await,
        _ => {
            proto
                .reply_error(&ReplyError::CommandNotSupported)
                .await
                .ok();
            anyhow::bail!("unsupported cmd: {cmd:?}")
        }
    }
}

// ── TCP ───────────────────────────────────────────────────────────────────────

async fn handle_tcp(
    proto: fast_socks5::server::Socks5ServerProtocol<
        tokio::net::TcpStream,
        fast_socks5::server::states::CommandRead,
    >,
    target_addr: TargetAddr,
    cfg: Arc<Config>,
) -> Result<()> {
    let target = target_to_string(&target_addr);
    info!("[TCP→] {target}");

    let ws_stream = match build_ws(&cfg).await {
        Ok(ws) => ws,
        Err(e) => {
            proto.reply_error(&ReplyError::HostUnreachable).await.ok();
            anyhow::bail!("ws: {e}");
        }
    };

    let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let local_stream = proto
        .reply_success(bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("reply_success: {e}"))?;

    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // 第一帧：TCP 控制帧
    let mut ctrl = BytesMut::new();
    ctrl.put_u8(b'T');
    ctrl.extend_from_slice(target.as_bytes());
    ws_tx.send(Message::Binary(ctrl.freeze().into())).await?;

    let (mut lr, mut lw) = tokio::io::split(local_stream);

    let up = async move {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            let n = lr.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            ws_tx
                .send(Message::Binary(Bytes::copy_from_slice(&buf[..n]).into()))
                .await?;
        }
        ws_tx.close().await?;
        anyhow::Ok(())
    };

    let down = async move {
        while let Some(msg) = ws_rx.next().await {
            match msg? {
                Message::Binary(data) => lw.write_all(&data).await?,
                Message::Close(_) => break,
                _ => {}
            }
        }
        anyhow::Ok(())
    };

    tokio::select! {
        r = up   => { r.ok(); }
        r = down => { r.ok(); }
    }
    Ok(())
}

// ── UDP ASSOCIATE + 分片 ──────────────────────────────────────────────────────

async fn handle_udp_associate(
    proto: fast_socks5::server::Socks5ServerProtocol<
        tokio::net::TcpStream,
        fast_socks5::server::states::CommandRead,
    >,
    cfg: Arc<Config>,
) -> Result<()> {
    info!("[UDP] associate");

    // 本地 UDP socket，应用把 UDP 包发这里
    let udp_local = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let local_udp_addr = udp_local.local_addr()?;

    let local_stream = proto
        .reply_success(local_udp_addr)
        .await
        .map_err(|e| anyhow::anyhow!("reply_success: {e}"))?;

    // 建 WS，发 UDP 控制帧
    let ws_stream = build_ws(&cfg).await?;
    let (ws_tx, ws_rx) = ws_stream.split();

    // channel 把两个 task 的写需求合并到一个 ws_tx
    let (tx, mut rx) = mpsc::channel::<Bytes>(256);

    // ws_tx writer task
    let mut ws_tx = ws_tx;
    let writer_task = tokio::spawn(async move {
        // 先发控制帧
        let ctrl = Bytes::from_static(b"U");
        if ws_tx.send(Message::Binary(ctrl.into())).await.is_err() {
            return;
        }
        while let Some(pkt) = rx.recv().await {
            if ws_tx.send(Message::Binary(pkt.into())).await.is_err() {
                break;
            }
        }
        ws_tx.close().await.ok();
    });

    let frag_map: FragMap = Arc::new(DashMap::new());

    // 记录客户端 UDP 地址（第一个包来的地址）
    let client_udp_addr: Arc<tokio::sync::Mutex<Option<SocketAddr>>> =
        Arc::new(tokio::sync::Mutex::new(None));

    // ── 本地 UDP → WS ────────────────────────────────────────────────────────
    let udp_recv = udp_local.clone();
    let tx_up = tx.clone();
    let client_addr_up = client_udp_addr.clone();

    let local_to_ws = async move {
        let mut buf = vec![0u8; 65535];
        let mut frag_id_counter: u16 = 0;

        loop {
            let (n, src) = udp_recv.recv_from(&mut buf).await?;
            {
                let mut addr = client_addr_up.lock().await;
                if addr.is_none() {
                    *addr = Some(src);
                }
            }

            let (target_host, target_port, data_offset) = match parse_socks5_udp_header(&buf[..n]) {
                Ok(v) => v,
                Err(e) => {
                    warn!("[UDP] bad header: {e}");
                    continue;
                }
            };

            // FRAG field from SOCKS5 UDP header (buf[2]) — reserved, ignored
            let payload = Bytes::copy_from_slice(&buf[data_offset..n]);

            // 把 payload 切成 ≤60KB 的分片（WebSocket 单帧限制）
            const MAX_FRAG: usize = 60 * 1024;
            let chunks: Vec<Bytes> = if payload.len() <= MAX_FRAG {
                vec![payload]
            } else {
                payload
                    .chunks(MAX_FRAG)
                    .map(|c| Bytes::copy_from_slice(c))
                    .collect()
            };

            let total = chunks.len() as u8;
            frag_id_counter = frag_id_counter.wrapping_add(1);
            let frag_id = frag_id_counter;

            for (i, chunk) in chunks.into_iter().enumerate() {
                let pkt = encode_udp_frame(
                    UDP_C2S,
                    frag_id,
                    i as u8,
                    total,
                    &target_host,
                    target_port,
                    &chunk,
                );
                if tx_up.send(pkt).await.is_err() {
                    break;
                }
            }
        }
        #[allow(unreachable_code)]
        anyhow::Ok(())
    };

    // ── WS → 本地 UDP ────────────────────────────────────────────────────────
    let udp_send = udp_local.clone();
    let client_addr_down = client_udp_addr.clone();
    let frag_map_down = frag_map.clone();
    let mut ws_rx = ws_rx;

    let ws_to_local = async move {
        while let Some(msg) = ws_rx.next().await {
            match msg? {
                Message::Binary(data) => {
                    // 解包 UDP 帧
                    let (frag_id, frag_no, frag_total, src_host, src_port, payload) =
                        match decode_udp_frame(&data) {
                            Ok(v) => v,
                            Err(e) => {
                                warn!("[UDP] decode: {e}");
                                continue;
                            }
                        };

                    // 重组分片
                    let full_payload = if frag_total == 1 {
                        payload
                    } else {
                        match reassemble(&frag_map_down, frag_id, frag_no, frag_total, payload) {
                            Some(p) => p,
                            None => continue,
                        }
                    };

                    // 封装成 SOCKS5 UDP 响应头，发给本地应用
                    let resp = build_socks5_udp_response(&src_host, src_port, &full_payload);
                    let addr = client_addr_down.lock().await;
                    if let Some(client_addr) = *addr {
                        udp_send.send_to(&resp, client_addr).await.ok();
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        anyhow::Ok(())
    };

    // TCP 控制连接断开时终止（SOCKS5 规范）
    let (mut tcp_rd, _tcp_wr) = tokio::io::split(local_stream);
    let tcp_watch = async move {
        let mut buf = [0u8; 1];
        let _ = tcp_rd.read(&mut buf).await;
    };

    tokio::select! {
        r = local_to_ws => { r.ok(); }
        r = ws_to_local => { r.ok(); }
        _ = tcp_watch   => { info!("[UDP] tcp control closed"); }
    }

    writer_task.abort();
    Ok(())
}

// ── 编解码 ────────────────────────────────────────────────────────────────────

/// 编码 UDP WS 帧：
/// [1B type][2B frag_id][1B frag_no][1B frag_total]
/// [2B host_len][host bytes][2B port][data]
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

/// 解码 UDP WS 帧
fn decode_udp_frame(data: &[u8]) -> Result<(u16, u8, u8, String, u16, Bytes)> {
    if data.len() < 7 {
        anyhow::bail!("frame too short");
    }
    // typ = data[0], 忽略
    let frag_id = u16::from_be_bytes([data[1], data[2]]);
    let frag_no = data[3];
    let frag_total = data[4];
    let host_len = u16::from_be_bytes([data[5], data[6]]) as usize;
    if data.len() < 7 + host_len + 2 {
        anyhow::bail!("frame host truncated");
    }
    let host = String::from_utf8_lossy(&data[7..7 + host_len]).to_string();
    let port = u16::from_be_bytes([data[7 + host_len], data[8 + host_len]]);
    let payload = Bytes::copy_from_slice(&data[9 + host_len..]);
    Ok((frag_id, frag_no, frag_total, host, port, payload))
}

/// 解析 SOCKS5 UDP 请求头，返回 (host, port, data_offset)
fn parse_socks5_udp_header(buf: &[u8]) -> Result<(String, u16, usize)> {
    // +----+------+------+----------+----------+
    // |RSV | FRAG | ATYP | DST.ADDR | DST.PORT |
    // | 2  |  1   |  1   | variable |    2     |
    if buf.len() < 4 {
        anyhow::bail!("udp header too short");
    }
    let atyp = buf[3];
    match atyp {
        0x01 => {
            if buf.len() < 10 {
                anyhow::bail!("ipv4 truncated");
            }
            let host = format!("{}.{}.{}.{}", buf[4], buf[5], buf[6], buf[7]);
            let port = u16::from_be_bytes([buf[8], buf[9]]);
            Ok((host, port, 10))
        }
        0x03 => {
            if buf.len() < 5 {
                anyhow::bail!("domain len truncated");
            }
            let len = buf[4] as usize;
            if buf.len() < 5 + len + 2 {
                anyhow::bail!("domain truncated");
            }
            let host = String::from_utf8_lossy(&buf[5..5 + len]).to_string();
            let port = u16::from_be_bytes([buf[5 + len], buf[6 + len]]);
            Ok((host, port, 7 + len))
        }
        0x04 => {
            if buf.len() < 22 {
                anyhow::bail!("ipv6 truncated");
            }
            let mut ip = [0u8; 16];
            ip.copy_from_slice(&buf[4..20]);
            let host = std::net::Ipv6Addr::from(ip).to_string();
            let port = u16::from_be_bytes([buf[20], buf[21]]);
            Ok((host, port, 22))
        }
        _ => anyhow::bail!("unknown atyp: {atyp:#x}"),
    }
}

/// 封装 SOCKS5 UDP 响应头
fn build_socks5_udp_response(src_host: &str, src_port: u16, payload: &[u8]) -> Vec<u8> {
    let mut resp = vec![0u8, 0u8, 0u8]; // RSV(2) + FRAG(0)
    if let Ok(ip) = src_host.parse::<std::net::Ipv4Addr>() {
        resp.push(0x01);
        resp.extend_from_slice(&ip.octets());
    } else if let Ok(ip) = src_host.parse::<std::net::Ipv6Addr>() {
        resp.push(0x04);
        resp.extend_from_slice(&ip.octets());
    } else {
        resp.push(0x03);
        let b = src_host.as_bytes();
        resp.push(b.len() as u8);
        resp.extend_from_slice(b);
    }
    resp.extend_from_slice(&src_port.to_be_bytes());
    resp.extend_from_slice(payload);
    resp
}

// ── WebSocket 连接 ────────────────────────────────────────────────────────────

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn build_ws(cfg: &Config) -> Result<WsStream> {
    let ws_url = cfg.ws_url();
    let url: url::Url = ws_url.parse()?;
    let host_header = format!(
        "{}{}",
        url.host_str().unwrap_or("localhost"),
        url.port().map(|p| format!(":{p}")).unwrap_or_default()
    );

    let mut req_builder = Request::builder()
        .uri(ws_url.as_str())
        .header("Host", &host_header)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Key", random_ws_key())
        .header("Sec-WebSocket-Version", "13");

    if !cfg.token.is_empty() {
        req_builder = req_builder.header("X-Proxy-Token", &cfg.token);
    }

    let (ws, _) = connect_async(req_builder.body(())?).await?;
    Ok(ws)
}

fn target_to_string(addr: &TargetAddr) -> String {
    match addr {
        TargetAddr::Ip(s) => s.to_string(),
        TargetAddr::Domain(host, port) => format!("{host}:{port}"),
    }
}

fn random_ws_key() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    STANDARD.encode(bytes)
}
