use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::RngCore;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::http::Request;

use crate::config::Config;

pub(crate) type WsStream = tokio_tungstenite::WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;

pub(crate) async fn build_ws(cfg: &Config) -> Result<WsStream> {
    // 1) 先把用户输入的完整 URL 解析成连接参数。
    let conn = WsConn::from_config(cfg)?;

    // 2) 准备 TLS 连接器和 SNI。
    let tls = build_tls_connector();
    let server_name = conn.server_name()?;

    // 3) 建立 TCP 连接。只有显式指定 outbound_ip 时才先 bind。
    let tcp = conn.connect_tcp().await?;

    // 4) 在 TCP 上完成 TLS 握手。
    let tls = tls
        .connect(server_name, tcp)
        .await
        .with_context(|| format!("tls handshake with {}", conn.host))?;

    // 5) 发起 WebSocket Upgrade。
    let mut req = Request::builder()
        .uri(&conn.uri)
        .header("Host", conn.host_header())
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Key", random_ws_key())
        .header("Sec-WebSocket-Version", "13");
    if !cfg.token.is_empty() {
        req = req.header("X-Proxy-Token", &cfg.token);
    }

    let (ws, _) = tokio_tungstenite::client_async(req.body(())?, tls)
        .await
        .with_context(|| format!("ws handshake failed: {}", conn.host))?;
    Ok(ws)
}

fn build_tls_connector() -> tokio_rustls::TlsConnector {
    use tokio_rustls::TlsConnector;

    let tls_config = Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            })
            .with_no_client_auth(),
    );
    TlsConnector::from(tls_config)
}

struct WsConn {
    host: String,
    port: u16,
    uri: String,
    host_header: String,
    outbound_ip: Option<IpAddr>,
}

impl WsConn {
    fn from_config(cfg: &Config) -> Result<Self> {
        let ws_url = normalize_ws_url(&cfg.server);
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

        Ok(WsConn {
            host_header: if port == def_port { host.clone() } else { format!("{host}:{port}") },
            uri: format!("{}://{}{}", scheme, host, path),
            host,
            port,
            outbound_ip: cfg
                .outbound_ip
                .as_ref()
                .map(|s| s.parse())
                .transpose()
                .with_context(|| "invalid outbound-ip")?,
        })
    }

    fn server_name(&self) -> Result<rustls::pki_types::ServerName<'static>> {
        use rustls::pki_types::ServerName;

        ServerName::try_from(self.host.as_str())
            .with_context(|| format!("invalid server name: {}", self.host))
            .map(|n| n.to_owned())
    }

    async fn connect_tcp(&self) -> Result<TcpStream> {
        if let Some(ip) = self.outbound_ip {
            self.connect_tcp_bound(ip).await
        } else {
            TcpStream::connect((self.host.as_str(), self.port))
                .await
                .with_context(|| format!("tcp connect to {}:{}", self.host, self.port))
        }
    }

    async fn connect_tcp_bound(&self, ip: IpAddr) -> Result<TcpStream> {
        // 绑定源 IP 后，需要先解析出一个具体的远端 SocketAddr。
        let socket = match ip {
            IpAddr::V4(_) => tokio::net::TcpSocket::new_v4()?,
            IpAddr::V6(_) => tokio::net::TcpSocket::new_v6()?,
        };
        if let Err(e) = socket.bind(SocketAddr::new(ip, 0)) {
            return Err(anyhow::anyhow!("bind outbound_ip {ip}: IP可能已失效(网络切换?) ({e})"));
        }

        let addrs = tokio::net::lookup_host((self.host.as_str(), self.port))
            .await
            .with_context(|| format!("DNS lookup failed: {}", self.host))?;
        let addr = addrs
            .into_iter()
            .find(|addr| {
                matches!((ip, addr), (IpAddr::V4(_), SocketAddr::V4(_)))
                    || matches!((ip, addr), (IpAddr::V6(_), SocketAddr::V6(_)))
            })
            .ok_or_else(|| anyhow::anyhow!("no usable server address for {}:{}", self.host, self.port))?;

        socket
            .connect(addr)
            .await
            .with_context(|| format!("tcp connect to {}:{} via {ip}", self.host, self.port))
    }

    fn host_header(&self) -> String {
        self.host_header.clone()
    }
}

fn normalize_ws_url(server: &str) -> String {
    if server.contains("://") {
        server.to_string()
    } else {
        format!("wss://{server}")
    }
}

fn random_ws_key() -> String {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    STANDARD.encode(b)
}
