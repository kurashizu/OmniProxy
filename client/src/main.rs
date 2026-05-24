use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use fast_socks5::server::{AuthMethodSuccessState, NoAuthentication, Socks5ServerProtocol};
use fast_socks5::util::target_addr::TargetAddr;
use fast_socks5::{ReplyError, Socks5Command};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};

// ── 配置 ─────────────────────────────────────────────────────────────────────
const SOCKS5_BIND: &str = "0.0.0.0:1080";
// cloudflared tunnel 暴露的域名，走 CF CDN + TLS
const WS_URL: &str = "wss://tunnel-oracle.022025.xyz";
const AUTH_TOKEN: &str = ""; // 留空不带 header

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let listener = TcpListener::bind(SOCKS5_BIND).await?;
    info!("socks5 on {SOCKS5_BIND}  →  {WS_URL}");

    loop {
        let (stream, peer) = listener.accept().await?;
        info!("[+] {peer}");
        tokio::spawn(async move {
            if let Err(e) = handle(stream).await {
                warn!("[!] {peer}: {e:#}");
            }
        });
    }
}

async fn handle(stream: tokio::net::TcpStream) -> Result<()> {
    // ── SOCKS5 握手 ───────────────────────────────────────────────────────────
    let proto = Socks5ServerProtocol::start(stream);

    let no_auth_impl = proto
        .negotiate_auth(&[NoAuthentication])
        .await
        .map_err(|e| anyhow::anyhow!("negotiate_auth: {e}"))?;

    let proto = no_auth_impl.finish_auth();

    let (proto, cmd, target_addr) = proto
        .read_command()
        .await
        .map_err(|e| anyhow::anyhow!("read_command: {e}"))?;

    if cmd != Socks5Command::TCPConnect {
        proto
            .reply_error(&ReplyError::CommandNotSupported)
            .await
            .ok();
        anyhow::bail!("unsupported cmd: {cmd:?}");
    }

    let target = target_to_string(&target_addr);
    info!("[→] {target}");

    // ── 建 WebSocket 到 Oracle/cloudflared ───────────────────────────────────
    let url: url::Url = WS_URL.parse()?;
    let host_header = url.host_str().unwrap_or("localhost").to_string()
        + url
            .port()
            .map(|p| format!(":{p}"))
            .unwrap_or_default()
            .as_str();

    let mut req_builder = Request::builder()
        .uri(WS_URL)
        .header("Host", host_header)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Key", random_ws_key())
        .header("Sec-WebSocket-Version", "13");

    if !AUTH_TOKEN.is_empty() {
        req_builder = req_builder.header("Authorization", format!("Bearer {AUTH_TOKEN}"));
    }

    let ws_stream = match connect_async(req_builder.body(())?).await {
        Ok((ws, _)) => ws,
        Err(e) => {
            proto.reply_error(&ReplyError::HostUnreachable).await.ok();
            anyhow::bail!("ws connect: {e}");
        }
    };

    // ── 回复 SOCKS5 客户端成功，拿回底层 stream ──────────────────────────────
    let bind_addr: std::net::SocketAddr = "0.0.0.0:0".parse().unwrap();
    let local_stream = proto
        .reply_success(bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("reply_success: {e}"))?;

    // ── 第一帧发目标，双向透传 ────────────────────────────────────────────────
    let (mut ws_tx, mut ws_rx) = ws_stream.split();
    ws_tx
        .send(Message::Binary(target.into_bytes().into()))
        .await?;

    let (mut lr, mut lw) = tokio::io::split(local_stream);

    let up = async move {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            let n = lr.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            ws_tx
                .send(Message::Binary(buf[..n].to_vec().into()))
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
