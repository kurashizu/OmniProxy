use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use bytes::{BufMut, Bytes, BytesMut};
use clap::Parser;
use fast_socks5::server::{AuthMethodSuccessState, NoAuthentication, Socks5ServerProtocol};
use fast_socks5::util::target_addr::TargetAddr;
use fast_socks5::{ReplyError, Socks5Command};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::Message;
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
#[command(name = "client")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1")]
    addr: String,
    #[arg(long, default_value = "1080")]
    port: u16,
    #[arg(long, default_value = "")]
    token: String,
    #[arg(long, default_value = "")]
    server: String,
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
            return Ok(
                serde_yaml::from_str(&text).with_context(|| format!("parse config: {path}"))?
            );
        }
        if cli.server.is_empty() {
            anyhow::bail!("--server is required");
        }
        Ok(Config {
            addr: cli.addr,
            port: cli.port,
            token: cli.token,
            server: cli.server,
        })
    }

    fn ws_url(&self) -> String {
        let h = &self.server;
        if h.starts_with("ws://") || h.starts_with("wss://") {
            h.clone()
        } else {
            format!("wss://{h}")
        }
    }
}

// ── 服务器信息 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ServerInfo {
    addr: SocketAddr,
    host: String,
    scheme: String,
    path: String,
    token: String,
}

impl ServerInfo {
    async fn resolve(cfg: &Config) -> Result<Self> {
        let ws_url = cfg.ws_url();
        let url: url::Url = ws_url
            .parse()
            .with_context(|| format!("invalid server url: {ws_url}"))?;

        let scheme = url.scheme().to_string();
        let host = url.host_str().context("missing host")?.to_string();
        let def_port = if scheme == "wss" { 443u16 } else { 80u16 };
        let port = url.port().unwrap_or(def_port);
        let path = if url.path().is_empty() {
            "/".to_string()
        } else {
            url.path().to_string()
        };

        let addr = tokio::net::lookup_host(format!("{host}:{port}"))
            .await
            .with_context(|| format!("DNS lookup failed: {host}"))?
            .next()
            .with_context(|| format!("no addr for {host}"))?;

        info!("resolved {host}:{port} → {addr}");
        Ok(ServerInfo {
            addr,
            host,
            scheme,
            path,
            token: cfg.token.clone(),
        })
    }

    fn host_header(&self) -> String {
        let def = if self.scheme == "wss" { 443u16 } else { 80u16 };
        if self.addr.port() == def {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.addr.port())
        }
    }
}

// ── WS 连接 ───────────────────────────────────────────────────────────────────

type WsStream = tokio_tungstenite::WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;

async fn build_ws(srv: &ServerInfo) -> Result<WsStream> {
    use rustls::pki_types::ServerName;
    use tokio_rustls::TlsConnector;

    let tcp = TcpStream::connect(srv.addr)
        .await
        .with_context(|| format!("tcp connect to {}", srv.addr))?;

    let tls_config = Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            })
            .with_no_client_auth(),
    );
    let connector = TlsConnector::from(tls_config);
    let server_name = ServerName::try_from(srv.host.as_str())
        .with_context(|| format!("invalid server name: {}", srv.host))?
        .to_owned();
    let tls = connector
        .connect(server_name, tcp)
        .await
        .with_context(|| format!("tls handshake with {}", srv.host))?;

    let uri = format!("{}://{}{}", srv.scheme, srv.host_header(), srv.path);
    let mut req = Request::builder()
        .uri(&uri)
        .header("Host", srv.host_header())
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Key", random_ws_key())
        .header("Sec-WebSocket-Version", "13");
    if !srv.token.is_empty() {
        req = req.header("X-Proxy-Token", &srv.token);
    }
    let (ws, _) = tokio_tungstenite::client_async(req.body(())?, tls)
        .await
        .context("ws handshake failed")?;
    Ok(ws)
}

// ── Mux 内部状态（共享） ──────────────────────────────────────────────────────

struct MuxInner {
    // stream_id → 下行数据 channel
    streams: HashMap<u32, mpsc::Sender<Bytes>>,
    // stream_id → 连接结果 oneshot
    connect_notify: HashMap<u32, oneshot::Sender<Result<()>>>,
    // UDP 回包 channel
    udp_tx: Option<mpsc::Sender<(String, u16, Bytes)>>,
    // 出帧 channel（发给 WS writer task）
    frame_tx: Option<mpsc::Sender<Bytes>>,
}

impl MuxInner {
    fn new() -> Self {
        MuxInner {
            streams: HashMap::new(),
            connect_notify: HashMap::new(),
            udp_tx: None,
            frame_tx: None,
        }
    }

    fn clear(&mut self) {
        self.streams.clear();
        self.connect_notify.clear();
        self.udp_tx = None;
        self.frame_tx = None;
    }
}

// ── Mux（带自动重连） ─────────────────────────────────────────────────────────

struct Mux {
    next_id: AtomicU32,
    inner: RwLock<MuxInner>,
    srv: Arc<ServerInfo>,
}

impl Mux {
    fn new(srv: Arc<ServerInfo>) -> Self {
        Mux {
            next_id: AtomicU32::new(1),
            inner: RwLock::new(MuxInner::new()),
            srv,
        }
    }

    fn alloc_id(&self) -> u32 {
        loop {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            if id != UDP_STREAM_ID {
                return id;
            }
        }
    }

    // 获取出帧 channel，如果没有（WS 断了）则返回 None
    async fn frame_tx(&self) -> Option<mpsc::Sender<Bytes>> {
        self.inner.read().await.frame_tx.clone()
    }

    async fn send_frame(&self, frame: Bytes) -> Result<()> {
        let tx = self.frame_tx().await.context("ws not connected")?;
        tx.send(frame)
            .await
            .map_err(|_| anyhow::anyhow!("ws writer closed"))
    }

    // 建立新 WS 连接，启动 writer + dispatcher
    // 返回一个 oneshot，当连接断开时触发（用于外部重连循环）
    async fn connect(self: &Arc<Self>) -> Result<tokio::sync::oneshot::Receiver<()>> {
        let ws = build_ws(&self.srv).await?;
        let (ws_tx, ws_rx) = ws.split();
        let (frame_tx, mut frame_rx) = mpsc::channel::<Bytes>(1024);

        // writer task（含心跳）
        let writer = tokio::spawn(async move {
            let mut ws_tx = ws_tx;
            let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
            heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    frame = frame_rx.recv() => {
                        match frame {
                            Some(f) => {
                                if ws_tx.send(Message::Binary(f.into())).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    _ = heartbeat.tick() => {
                        if ws_tx.send(Message::Ping(bytes::Bytes::new())).await.is_err() {
                            break;
                        }
                    }
                }
            }
            ws_tx.close().await.ok();
        });

        {
            let mut inner = self.inner.write().await;
            inner.frame_tx = Some(frame_tx);
        }

        // disconnected 信号
        let (disc_tx, disc_rx) = tokio::sync::oneshot::channel::<()>();

        // dispatcher task（只负责收帧，不做重连）
        let mux = self.clone();
        tokio::spawn(async move {
            mux.dispatch(ws_rx, writer).await;
            disc_tx.send(()).ok();
        });

        Ok(disc_rx)
    }

    async fn dispatch(
        self: &Arc<Self>,
        mut ws_rx: impl StreamExt<Item = std::result::Result<Message, tokio_tungstenite::tungstenite::Error>>
            + Unpin
            + Send
            + 'static,
        writer: tokio::task::JoinHandle<()>,
    ) {
        loop {
            match ws_rx.next().await {
                Some(Ok(Message::Binary(data))) => {
                    let (id, typ, payload) = match decode_frame(&data) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("[mux] decode: {e}");
                            continue;
                        }
                    };
                    match typ {
                        TYPE_TCP_CONNECTED => {
                            let mut inner = self.inner.write().await;
                            if let Some(tx) = inner.connect_notify.remove(&id) {
                                let r = if payload.is_empty() {
                                    Ok(())
                                } else {
                                    Err(anyhow::anyhow!("{}", String::from_utf8_lossy(&payload)))
                                };
                                tx.send(r).ok();
                            }
                        }
                        TYPE_TCP_DATA => {
                            let inner = self.inner.read().await;
                            if let Some(tx) = inner.streams.get(&id) {
                                tx.send(payload).await.ok();
                            }
                        }
                        TYPE_TCP_FIN => {
                            self.inner.write().await.streams.remove(&id);
                        }
                        TYPE_UDP_DATA => {
                            if let Ok((host, port, data)) = decode_udp_payload(&payload) {
                                let inner = self.inner.read().await;
                                if let Some(tx) = &inner.udp_tx {
                                    tx.send((host, port, data)).await.ok();
                                }
                            }
                        }
                        _ => warn!("[mux] unknown type {typ:#x} sid={id}"),
                    }
                }
                Some(Ok(Message::Ping(_))) => {}
                Some(Ok(Message::Pong(_))) => {}
                Some(Ok(Message::Close(_))) => {
                    info!("[mux] server closed ws");
                    break;
                }
                Some(Err(e)) => {
                    warn!("[mux] ws error: {e}");
                    break;
                }
                None => break,
                _ => {}
            }
        }

        writer.abort();
        self.inner.write().await.clear();
    }

    async fn tcp_connect(
        &self,
        target: &str,
    ) -> Result<(u32, mpsc::Receiver<Bytes>, oneshot::Receiver<Result<()>>)> {
        let id = self.alloc_id();
        let (data_tx, data_rx) = mpsc::channel::<Bytes>(64);
        let (conn_tx, conn_rx) = oneshot::channel::<Result<()>>();
        {
            let mut inner = self.inner.write().await;
            inner.streams.insert(id, data_tx);
            inner.connect_notify.insert(id, conn_tx);
        }
        self.send_frame(encode_frame(id, TYPE_TCP_CONNECT, target.as_bytes()))
            .await?;
        Ok((id, data_rx, conn_rx))
    }

    async fn tcp_data(&self, id: u32, data: Bytes) -> Result<()> {
        self.send_frame(encode_frame(id, TYPE_TCP_DATA, &data))
            .await
    }

    async fn tcp_fin(&self, id: u32) {
        self.send_frame(encode_frame(id, TYPE_TCP_FIN, &[]))
            .await
            .ok();
        self.inner.write().await.streams.remove(&id);
    }

    async fn register_udp(&self) -> mpsc::Receiver<(String, u16, Bytes)> {
        let (tx, rx) = mpsc::channel(256);
        self.inner.write().await.udp_tx = Some(tx);
        rx
    }

    async fn udp_send(&self, host: &str, port: u16, data: &[u8]) -> Result<()> {
        let payload = encode_udp_payload(host, port, data);
        self.send_frame(encode_frame(UDP_STREAM_ID, TYPE_UDP_DATA, &payload))
            .await
    }
}

// ── fd 限制 ───────────────────────────────────────────────────────────────────

fn raise_nofile_limit() {
    #[cfg(unix)]
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
                info!("raised nofile limit to {target}");
            }
        }
    }
}

// ── 入口 ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    raise_nofile_limit();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "client=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = Config::from_cli(cli)?;
    let srv = Arc::new(ServerInfo::resolve(&cfg).await?);
    let bind = format!("{}:{}", cfg.addr, cfg.port);

    let mux = Arc::new(Mux::new(srv.clone()));
    let disc_rx = mux.connect().await?;

    info!(
        "socks5 on {bind}  →  {}://{}{}",
        srv.scheme, srv.host, srv.path
    );

    // 重连循环
    {
        let mux = mux.clone();
        tokio::spawn(async move {
            let mut disc = disc_rx;
            loop {
                let _ = disc.await;
                info!("[mux] disconnected, reconnecting in 3s...");
                tokio::time::sleep(Duration::from_secs(3)).await;
                loop {
                    match mux.connect().await {
                        Ok(new_disc) => {
                            info!("[mux] reconnected");
                            disc = new_disc;
                            break;
                        }
                        Err(e) => {
                            warn!("[mux] reconnect failed: {e:#}, retry in 5s");
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }
                    }
                }
            }
        });
    }

    let listener = TcpListener::bind(&bind).await?;
    loop {
        let (stream, peer) = listener.accept().await?;
        debug!("[+] {peer}");
        let mux = mux.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, mux).await {
                debug!("[!] {peer}: {e:#}");
            }
        });
    }
}

// ── SOCKS5 dispatch ───────────────────────────────────────────────────────────

async fn handle(stream: TcpStream, mux: Arc<Mux>) -> Result<()> {
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
        Socks5Command::TCPConnect => handle_tcp(proto, target_addr, mux).await,
        Socks5Command::UDPAssociate => handle_udp(proto, mux).await,
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
        TcpStream,
        fast_socks5::server::states::CommandRead,
    >,
    target_addr: TargetAddr,
    mux: Arc<Mux>,
) -> Result<()> {
    let target = target_to_string(&target_addr);
    debug!("[TCP→] {target}");

    let (stream_id, mut data_rx, conn_rx) = match mux.tcp_connect(&target).await {
        Ok(v) => v,
        Err(e) => {
            proto.reply_error(&ReplyError::HostUnreachable).await.ok();
            return Err(e);
        }
    };

    match conn_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            proto.reply_error(&ReplyError::HostUnreachable).await.ok();
            anyhow::bail!("server connect failed: {e}");
        }
        Err(_) => {
            proto.reply_error(&ReplyError::HostUnreachable).await.ok();
            anyhow::bail!("mux closed before connect ack");
        }
    }

    let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let local_stream = proto
        .reply_success(bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("reply_success: {e}"))?;
    let (mut lr, mut lw) = tokio::io::split(local_stream);
    let mux_up = mux.clone();

    let up = async move {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            let n = lr.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            mux_up
                .tcp_data(stream_id, Bytes::copy_from_slice(&buf[..n]))
                .await?;
        }
        mux_up.tcp_fin(stream_id).await;
        anyhow::Ok(())
    };

    let down = async move {
        while let Some(data) = data_rx.recv().await {
            lw.write_all(&data).await?;
        }
        anyhow::Ok(())
    };

    tokio::select! {
        r = up   => { r.ok(); }
        r = down => { r.ok(); }
    }
    Ok(())
}

// ── UDP ───────────────────────────────────────────────────────────────────────

async fn handle_udp(
    proto: fast_socks5::server::Socks5ServerProtocol<
        TcpStream,
        fast_socks5::server::states::CommandRead,
    >,
    mux: Arc<Mux>,
) -> Result<()> {
    debug!("[UDP] associate");

    let udp = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let local_addr = udp.local_addr()?;
    let local_stream = proto
        .reply_success(local_addr)
        .await
        .map_err(|e| anyhow::anyhow!("reply_success: {e}"))?;

    let mut udp_rx = mux.register_udp().await;
    let client_addr: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));

    let udp_recv = udp.clone();
    let mux_up = mux.clone();
    let ca_up = client_addr.clone();

    let local_to_mux = async move {
        let mut buf = vec![0u8; 65535];
        loop {
            let (n, src) = udp_recv.recv_from(&mut buf).await?;
            {
                let mut a = ca_up.lock().await;
                if a.is_none() {
                    *a = Some(src);
                }
            }
            let (host, port, data_offset) = match parse_socks5_udp_header(&buf[..n]) {
                Ok(v) => v,
                Err(e) => {
                    warn!("[UDP] bad header: {e}");
                    continue;
                }
            };
            mux_up.udp_send(&host, port, &buf[data_offset..n]).await?;
        }
        #[allow(unreachable_code)]
        anyhow::Ok(())
    };

    let udp_send = udp.clone();
    let ca_down = client_addr.clone();

    let mux_to_local = async move {
        while let Some((src_host, src_port, data)) = udp_rx.recv().await {
            let resp = build_socks5_udp_response(&src_host, src_port, &data);
            if let Some(a) = *ca_down.lock().await {
                udp_send.send_to(&resp, a).await.ok();
            }
        }
        anyhow::Ok(())
    };

    let (mut tcp_rd, _) = tokio::io::split(local_stream);
    let tcp_watch = async move {
        let mut buf = [0u8; 1];
        let _ = tcp_rd.read(&mut buf).await;
    };

    tokio::select! {
        r = local_to_mux => { r.ok(); }
        r = mux_to_local => { r.ok(); }
        _ = tcp_watch    => { debug!("[UDP] tcp control closed"); }
    }
    Ok(())
}

// ── SOCKS5 UDP 头 ─────────────────────────────────────────────────────────────

fn parse_socks5_udp_header(buf: &[u8]) -> Result<(String, u16, usize)> {
    if buf.len() < 4 {
        anyhow::bail!("udp header too short");
    }
    match buf[3] {
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
        t => anyhow::bail!("unknown atyp: {t:#x}"),
    }
}

fn build_socks5_udp_response(src_host: &str, src_port: u16, payload: &[u8]) -> Vec<u8> {
    let mut resp = vec![0u8, 0u8, 0u8];
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

fn target_to_string(addr: &TargetAddr) -> String {
    match addr {
        TargetAddr::Ip(s) => s.to_string(),
        TargetAddr::Domain(host, port) => format!("{host}:{port}"),
    }
}

fn random_ws_key() -> String {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    STANDARD.encode(b)
}
