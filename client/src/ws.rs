use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::RngCore;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::http::Request;

use crate::config::Config;

pub(crate) type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;

pub(crate) async fn build_ws(cfg: &Config) -> Result<WsStream> {
    fn random_ws_key() -> String {
        let mut bytes = [0u8; 16];
        rand::rng().fill_bytes(&mut bytes);
        STANDARD.encode(bytes)
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

    let conn = WsConn::from_config(cfg)?;
    let tls = build_tls_connector();
    let tcp = conn.connect_tcp().await?;

    let tls = tls
        .connect(
            rustls::pki_types::ServerName::try_from(conn.host.as_str())
                .with_context(|| format!("invalid server name: {}", conn.host))?
                .to_owned(),
            tcp,
        )
        .await
        .with_context(|| format!("tls handshake with {}", conn.host))?;

    let request = build_ws_request(&conn, &cfg.token, random_ws_key())?;
    let (ws, _) = tokio_tungstenite::client_async(request, tls)
        .await
        .with_context(|| format!("ws handshake failed: {}", conn.host))?;

    Ok(ws)
}

fn build_ws_request(conn: &WsConn, token: &str, key: String) -> Result<Request<()>> {
    let mut req = Request::builder()
        .uri(&conn.uri)
        .header("Host", &conn.host_header)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Key", key)
        .header("Sec-WebSocket-Version", "13");

    if !token.is_empty() {
        req = req.header("X-Proxy-Token", token);
    }

    Ok(req.body(())?)
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
        let parsed = url::Url::parse(&cfg.server)
            .or_else(|_| url::Url::parse(&format!("wss://{}", &cfg.server)))
            .with_context(|| format!("invalid server url: {}", &cfg.server))?;
        let (scheme, host, port) = (
            parsed.scheme(),
            parsed.host_str().context("missing host")?,
            parsed.port().unwrap_or(match parsed.scheme() {
                "wss" => 443,
                _ => 80,
            }),
        );
        let host_header = if (scheme == "wss" && port == 443) || (scheme == "ws" && port == 80) {
            host.to_string()
        } else {
            format!("{host}:{port}")
        };

        Ok(Self {
            uri: format!(
                "{}://{}{}",
                scheme,
                host,
                if parsed.path().is_empty() { "/" } else { parsed.path() }
            ),
            host: host.to_string(),
            port,
            host_header,
            outbound_ip: cfg
            .outbound_ip
            .as_ref()
            .map(|s| s.parse().with_context(|| format!("invalid outbound-ip: {s}")))
            .transpose()?,
        })
    }

    async fn connect_tcp(&self) -> Result<TcpStream> {
        // 1) 不绑定源 IP 时，直接交给系统解析并连接。
        if let None = self.outbound_ip {
            return TcpStream::connect((self.host.as_str(), self.port))
                .await
                .with_context(|| format!("tcp connect to {}:{}", self.host, self.port));
        }

        // 2) 绑定源 IP 时，先 bind 再连具体远端地址。
        let ip = self.outbound_ip.expect("checked above");
        let socket = match ip {
            IpAddr::V4(_) => tokio::net::TcpSocket::new_v4()?,
            IpAddr::V6(_) => tokio::net::TcpSocket::new_v6()?,
        };

        socket
            .bind(SocketAddr::new(ip, 0))
            .map_err(|e| anyhow::anyhow!("failed to bind outbound_ip {ip}: {e}"))?;

        let remote = Self::resolve_remote_addr(self.host.as_str(), self.port, ip).await?;
        socket
            .connect(remote)
            .await
            .with_context(|| format!("tcp connect to {}:{} via {ip}", self.host, self.port))
    }

    async fn resolve_remote_addr(host: &str, port: u16, ip: IpAddr) -> Result<SocketAddr> {
        let addrs = tokio::net::lookup_host((host, port))
            .await
            .with_context(|| format!("DNS lookup failed: {host}"))?;

        addrs
            .into_iter()
            .find(|addr| {
                matches!((ip, addr), (IpAddr::V4(_), SocketAddr::V4(_)))
                    || matches!((ip, addr), (IpAddr::V6(_), SocketAddr::V6(_)))
            })
            .ok_or_else(|| anyhow::anyhow!("no usable server address for {host}:{port}"))
    }
}
