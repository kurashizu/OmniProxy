use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::RngCore;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::{OnceLock, RwLock};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::http::Request;
use tracing::info;

use crate::config::Config;

// DNS cache: host → (resolved IP, insertion time). Entries expire after TTL.
static DNS_CACHE: OnceLock<RwLock<HashMap<String, (IpAddr, std::time::Instant)>>> = OnceLock::new();
const DNS_TTL: std::time::Duration = std::time::Duration::from_secs(300); // 5 minutes

fn dns_cache() -> &'static RwLock<HashMap<String, (IpAddr, std::time::Instant)>> {
    DNS_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

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

    info!(host = %conn.host, "tls handshake");
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

        info!(scheme, host, port, "ws target resolved");

        Ok(Self {
            uri: format!(
                "{}://{}{}",
                scheme,
                host,
                if parsed.path().is_empty() {
                    "/"
                } else {
                    parsed.path()
                }
            ),
            host: host.to_string(),
            port,
            host_header,
            outbound_ip: cfg
                .outbound_ip
                .as_ref()
                .map(|s| {
                    s.parse()
                        .with_context(|| format!("invalid outbound-ip: {s}"))
                })
                .transpose()?,
        })
    }

    async fn connect_tcp(&self) -> Result<TcpStream> {
        match self.outbound_ip {
            Some(ip) => {
                info!(outbound_ip = %ip, "binding to outbound interface");
                let remote = Self::resolve_remote_addr(&self.host, self.port, ip).await?;
                let socket = if ip.is_ipv6() {
                    tokio::net::TcpSocket::new_v6()?
                } else {
                    tokio::net::TcpSocket::new_v4()?
                };
                bind_to_interface(&socket, ip)?;
                socket
                    .bind(SocketAddr::new(ip, 0))
                    .with_context(|| format!("bind {ip}"))?;
                socket
                    .connect(remote)
                    .await
                    .with_context(|| format!("tcp connect via {ip}"))
            }
            None => TcpStream::connect((self.host.as_str(), self.port))
                .await
                .with_context(|| format!("tcp connect to {}:{}", self.host, self.port)),
        }
    }

    /// Resolve host to SocketAddr. Caches result with TTL so stale entries are refreshed.
    async fn resolve_remote_addr(host: &str, port: u16, outbound_ip: IpAddr) -> Result<SocketAddr> {
        // Cache hit — check TTL.
        {
            let cache = dns_cache().read().ok();
            if let Some(cache) = cache
                && let Some((ip, inserted)) = cache.get(host)
                && inserted.elapsed() < DNS_TTL
            {
                info!(host, ip = %ip, "dns cache hit");
                return Ok(SocketAddr::new(*ip, port));
            }
        }

        // Cache miss or expired — platform-specific DNS query.
        let addrs = tokio::net::lookup_host((host, port))
            .await
            .with_context(|| format!("dns lookup failed: {host}"))?;

        let matched = addrs
            .into_iter()
            .find(|addr| {
                matches!((outbound_ip, addr), (IpAddr::V4(_), SocketAddr::V4(_)))
                    || matches!((outbound_ip, addr), (IpAddr::V6(_), SocketAddr::V6(_)))
            })
            .ok_or_else(|| anyhow::anyhow!("no usable server address for {host}:{port}"))?;

        // Cache for next time.
        if let Ok(mut cache) = dns_cache().write() {
            cache.insert(host.to_string(), (matched.ip(), std::time::Instant::now()));
            info!(host, ip = %matched.ip(), "dns cached");
        }

        Ok(matched)
    }
}

// ── Cross-platform interface binding ──────────────────────────────────────────

#[cfg(target_os = "linux")]
fn iface_from_ip(target: IpAddr) -> Result<String> {
    use std::ffi::CStr;

    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        anyhow::ensure!(
            libc::getifaddrs(&mut ifap) == 0,
            "getifaddrs failed: {}",
            std::io::Error::last_os_error()
        );
        struct Guard(*mut libc::ifaddrs);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe { libc::freeifaddrs(self.0) };
            }
        }
        let _guard = Guard(ifap);

        let mut cur = ifap;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null() {
                continue;
            }

            let matched = match ((*ifa.ifa_addr).sa_family as i32, target) {
                (libc::AF_INET, IpAddr::V4(target_v4)) => {
                    let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                    std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr)) == target_v4
                }
                (libc::AF_INET6, IpAddr::V6(target_v6)) => {
                    let sin6 = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                    std::net::Ipv6Addr::from(sin6.sin6_addr.s6_addr) == target_v6
                }
                _ => false,
            };

            if matched {
                let name = CStr::from_ptr(ifa.ifa_name)
                    .to_str()
                    .context("interface name is not utf-8")?
                    .to_owned();
                return Ok(name);
            }
        }
    }

    anyhow::bail!("no interface found for IP {target}")
}

#[cfg(target_os = "windows")]
fn iface_from_ip(target: IpAddr) -> Result<u32> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_INCLUDE_PREFIX, GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::Networking::WinSock::AF_INET;

    unsafe {
        let mut buf_len: u32 = 16384;
        let mut buf: Vec<u8>;
        loop {
            buf = vec![0u8; buf_len as usize];
            let ret = GetAdaptersAddresses(
                AF_INET.0 as u32,
                GAA_FLAG_INCLUDE_PREFIX,
                None,
                Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut buf_len,
            );
            match ret {
                0 => break,
                111 => continue, // ERROR_BUFFER_OVERFLOW
                e => anyhow::bail!("GetAdaptersAddresses failed: {e}"),
            }
        }

        let mut p = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !p.is_null() {
            let a = &*p;
            let mut uni = a.FirstUnicastAddress;
            while !uni.is_null() {
                let sa = (*uni).Address.lpSockaddr;
                if !sa.is_null()
                    && (*sa).sa_family
                        == windows::Win32::Networking::WinSock::ADDRESS_FAMILY(AF_INET.0 as u16)
                {
                    if let IpAddr::V4(v4) = target {
                        let sin = &*(sa as *const windows::Win32::Networking::WinSock::SOCKADDR_IN);
                        let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.S_un.S_addr));
                        if ip == v4 {
                            return Ok(a.Anonymous1.Anonymous.IfIndex);
                        }
                    }
                }
                uni = (*uni).Next;
            }
            p = a.Next;
        }
    }

    anyhow::bail!("no interface found for IP {target}")
}

#[cfg(target_os = "linux")]
fn bind_to_interface(socket: &tokio::net::TcpSocket, ip: IpAddr) -> Result<()> {
    use socket2::SockRef;

    let iface = iface_from_ip(ip)?;
    info!(iface, "SO_BINDTODEVICE");
    SockRef::from(socket)
        .bind_device(Some(iface.as_bytes()))
        .with_context(|| {
            format!("SO_BINDTODEVICE({iface}) failed — requires CAP_NET_RAW or root")
        })?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn bind_to_interface(socket: &tokio::net::TcpSocket, ip: IpAddr) -> Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows::Win32::Networking::WinSock::{IP_UNICAST_IF, IPPROTO_IP, SOCKET, setsockopt};

    let idx = iface_from_ip(ip)?;
    info!(idx, "IP_UNICAST_IF");
    let idx_be = u32::to_be(idx);
    let raw = socket.as_raw_socket();
    unsafe {
        let ret = setsockopt(
            SOCKET(raw as usize),
            IPPROTO_IP.0 as i32,
            IP_UNICAST_IF as i32,
            Some(&idx_be.to_ne_bytes()),
        );
        anyhow::ensure!(ret == 0, "IP_UNICAST_IF(idx={idx}) failed: {ret}");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn iface_from_ip(target: IpAddr) -> Result<String> {
    use std::ffi::CStr;

    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        anyhow::ensure!(
            libc::getifaddrs(&mut ifap) == 0,
            "getifaddrs failed: {}",
            std::io::Error::last_os_error()
        );
        struct Guard(*mut libc::ifaddrs);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe { libc::freeifaddrs(self.0) };
            }
        }
        let _guard = Guard(ifap);

        let mut cur = ifap;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null() {
                continue;
            }

            let matched = match ((*ifa.ifa_addr).sa_family as i32, target) {
                (libc::AF_INET, IpAddr::V4(target_v4)) => {
                    let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                    std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr)) == target_v4
                }
                (libc::AF_INET6, IpAddr::V6(target_v6)) => {
                    let sin6 = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                    std::net::Ipv6Addr::from(sin6.sin6_addr.s6_addr) == target_v6
                }
                _ => false,
            };

            if matched {
                let name = CStr::from_ptr(ifa.ifa_name)
                    .to_str()
                    .context("interface name is not utf-8")?
                    .to_owned();
                return Ok(name);
            }
        }
    }

    anyhow::bail!("no interface found for IP {target}")
}

#[cfg(target_os = "macos")]
fn bind_to_interface(socket: &tokio::net::TcpSocket, ip: IpAddr) -> Result<()> {
    use std::os::fd::AsRawFd;

    let iface = iface_from_ip(ip)?;
    let idx = unsafe { libc::if_nametoindex(iface.as_bytes().as_ptr() as *const _) };
    anyhow::ensure!(idx != 0, "if_nametoindex failed for interface {iface}");

    info!(iface, idx, "IP_BOUND_IF");
    let ret = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::IPPROTO_IP,
            libc::IP_BOUND_IF,
            &idx as *const u32 as *const _,
            std::mem::size_of::<u32>() as libc::socklen_t,
        )
    };
    anyhow::ensure!(
        ret == 0,
        "IP_BOUND_IF failed: {}",
        std::io::Error::last_os_error()
    );

    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn bind_to_interface(_socket: &tokio::net::TcpSocket, _ip: IpAddr) -> Result<()> {
    anyhow::bail!("outbound-ip interface binding not yet supported on this platform")
}
