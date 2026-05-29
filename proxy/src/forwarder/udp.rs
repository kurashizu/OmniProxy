use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
#[cfg(unix)]
use libc;
use netstack_smoltcp::udp::{ReadHalf, WriteHalf, UdpMsg};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const SESSION_TIMEOUT: Duration = Duration::from_secs(120);
const REAP_INTERVAL: Duration = Duration::from_secs(30);

/// 四元组 session key：(src, dst) 完整标识一条 UDP 会话
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SessionKey {
    src: SocketAddr,
    dst: SocketAddr,
}

struct UdpSession {
    relay_socket: Arc<UdpSocket>,
    relay_addr: SocketAddr,      // SOCKS5 relay UDP 地址
    _control: TcpStream,         // SOCKS5 TCP 控制连接（保持 alive）
    last_active: Instant,
}

pub(crate) async fn run_udp_handler(
    mut read_half: ReadHalf,
    mut write_half: WriteHalf,
    socks_port: u16,
) -> Result<()> {
    info!("[udp] handler started, socks_port={socks_port}");
    let socks_addr = format!("127.0.0.1:{socks_port}");

    let mut sessions: HashMap<SessionKey, UdpSession> = HashMap::new();
    let (inbound_tx, mut inbound_rx) = mpsc::channel::<(SessionKey, Vec<u8>)>(2048);

    let mut reap_interval = tokio::time::interval(REAP_INTERVAL);

    // 后台任务：把响应包写回 TUN
    tokio::spawn(async move {
        while let Some((key, payload)) = inbound_rx.recv().await {
            // 响应的 src 是原始 dst，dst 是原始 src
            let msg: UdpMsg = (payload, key.dst, key.src);
            if let Err(e) = write_half.send(msg).await {
                warn!("[udp] write to tun: {e}");
            }
        }
    });

    loop {
        tokio::select! {
            msg = read_half.next() => {
                let (data, local, remote) = match msg {
                    Some(m) => m,
                    None => {
                        warn!("[udp] read_half stream ended");
                        break;
                    }
                };
                // local = src（应用），remote = dst（目标）
                let key = SessionKey { src: local, dst: remote };
                handle_outbound(
                    &mut sessions,
                    &socks_addr,
                    key,
                    data,
                    inbound_tx.clone(),
                ).await;
            }

            _ = reap_interval.tick() => {
                reap_idle(&mut sessions);
            }
        }
    }

    Ok(())
}

async fn handle_outbound(
    sessions: &mut HashMap<SessionKey, UdpSession>,
    socks_addr: &str,
    key: SessionKey,
    data: Vec<u8>,
    inbound_tx: mpsc::Sender<(SessionKey, Vec<u8>)>,
) {
    // 已有 session，直接发
    if let Some(session) = sessions.get_mut(&key) {
        session.last_active = Instant::now();
        let pkt = build_socks5_udp_packet(key.dst, &data);
        if let Err(e) = session.relay_socket.send_to(&pkt, session.relay_addr).await {
            warn!("[udp] send to relay {}: {e}", session.relay_addr);
            sessions.remove(&key);
        }
        return;
    }

    // 新 session
    let session = match create_session(socks_addr, key, data, inbound_tx).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[udp] create session failed for {key:?}: {e}");
            return;
        }
    };
    sessions.insert(key, session);
}

async fn create_session(
    socks_addr: &str,
    key: SessionKey,
    first_payload: Vec<u8>,
    inbound_tx: mpsc::Sender<(SessionKey, Vec<u8>)>,
) -> Result<UdpSession> {
    // 1. SOCKS5 握手 + UDP ASSOCIATE
    let mut control = TcpStream::connect(socks_addr)
        .await
        .context("connect socks5")?;

    control.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut buf = [0u8; 2];
    control.read_exact(&mut buf).await?;
    anyhow::ensure!(buf[0] == 0x05 && buf[1] == 0x00, "socks5 auth failed");

    let req = [0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
    control.write_all(&req).await?;

    let mut hdr = [0u8; 4];
    control.read_exact(&mut hdr).await?;
    anyhow::ensure!(hdr[1] == 0x00, "socks5 UDP ASSOCIATE failed: {}", hdr[1]);

    let relay_addr = match hdr[3] {
        0x01 => {
            let mut addr = [0u8; 4];
            control.read_exact(&mut addr).await?;
            let mut port_buf = [0u8; 2];
            control.read_exact(&mut port_buf).await?;
            SocketAddr::new(std::net::Ipv4Addr::from(addr).into(), u16::from_be_bytes(port_buf))
        }
        0x04 => {
            let mut addr = [0u8; 16];
            control.read_exact(&mut addr).await?;
            let mut port_buf = [0u8; 2];
            control.read_exact(&mut port_buf).await?;
            SocketAddr::new(std::net::Ipv6Addr::from(addr).into(), u16::from_be_bytes(port_buf))
        }
        other => anyhow::bail!("unexpected ATYP in UDP ASSOCIATE reply: {other:#x}"),
    };

    // 2. 绑定独立的 relay socket，设置较大的缓冲区
    let relay_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    // 设置 OS 级别的 UDP 缓冲区 (2MB)
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = relay_socket.as_raw_fd();
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVBUF,
                &(2 * 1024 * 1024 as libc::c_int) as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_SNDBUF,
                &(2 * 1024 * 1024 as libc::c_int) as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
    }
    info!("[udp] session {key:?} created, relay={relay_addr}");

    // 3. 发送第一个包
    let pkt = build_socks5_udp_packet(key.dst, &first_payload);
    relay_socket.send_to(&pkt, relay_addr).await?;

    // 4. 启动响应接收任务
    let relay_clone = relay_socket.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            match tokio::time::timeout(SESSION_TIMEOUT, relay_clone.recv_from(&mut buf)).await {
                Ok(Ok((n, _from))) => {
                    // 剥离 SOCKS5 UDP 头部，取出 payload
                    match strip_socks5_udp_header(&buf[..n]) {
                        Ok(payload) => {
                            if inbound_tx.send((key, payload)).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("[udp] bad response from relay: {e}");
                        }
                    }
                }
                Ok(Err(e)) => {
                    warn!("[udp] relay recv error: {e}");
                    break;
                }
                Err(_) => {
                    debug!("[udp] session {key:?} recv timeout");
                    break;
                }
            }
        }
    });

    Ok(UdpSession {
        relay_socket,
        relay_addr,
        _control: control,
        last_active: Instant::now(),
    })
}

fn reap_idle(sessions: &mut HashMap<SessionKey, UdpSession>) {
    let now = Instant::now();
    let before = sessions.len();
    sessions.retain(|key, session| {
        let alive = now.duration_since(session.last_active) < SESSION_TIMEOUT;
        if !alive {
            debug!("[udp] reaped idle session {key:?}");
        }
        alive
    });
    let removed = before - sessions.len();
    if removed > 0 {
        debug!("[udp] reaped {removed} sessions, {} remaining", sessions.len());
    }
}

/// 构造 SOCKS5 UDP 请求头
/// [RSV 2B][FRAG 1B][ATYP 1B][DST.ADDR][DST.PORT][DATA]
fn build_socks5_udp_packet(dst: SocketAddr, data: &[u8]) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(10 + data.len());
    pkt.extend_from_slice(&[0x00, 0x00, 0x00]); // RSV + FRAG
    match dst {
        SocketAddr::V4(s) => {
            pkt.push(0x01);
            pkt.extend_from_slice(&s.ip().octets());
        }
        SocketAddr::V6(s) => {
            pkt.push(0x04);
            pkt.extend_from_slice(&s.ip().octets());
        }
    }
    pkt.extend_from_slice(&dst.port().to_be_bytes());
    pkt.extend_from_slice(data);
    pkt
}

/// 剥离 SOCKS5 UDP 响应头，返回 payload
/// [RSV 2B][FRAG 1B][ATYP 1B][BND.ADDR][BND.PORT][DATA]
fn strip_socks5_udp_header(buf: &[u8]) -> Result<Vec<u8>> {
    anyhow::ensure!(buf.len() >= 4, "header too short");
    anyhow::ensure!(buf[0] == 0 && buf[1] == 0, "RSV != 0");
    anyhow::ensure!(buf[2] == 0, "FRAG != 0");
    let payload_start = match buf[3] {
        0x01 => 10,  // IPv4: 3+1+1+4+2
        0x03 => {    // Domain: 3+1+1+1+len+2
            anyhow::ensure!(buf.len() >= 5, "domain len truncated");
            7 + buf[4] as usize
        }
        0x04 => 22,  // IPv6: 3+1+1+16+2
        t => anyhow::bail!("unknown ATYP: {t:#x}"),
    };
    anyhow::ensure!(buf.len() >= payload_start, "truncated");
    Ok(buf[payload_start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_socks5_udp_packet_v4() {
        let dst: SocketAddr = "1.2.3.4:53".parse().unwrap();
        let pkt = build_socks5_udp_packet(dst, b"hello");
        assert_eq!(pkt[3], 0x01);
        assert_eq!(&pkt[4..8], &[1, 2, 3, 4]);
        assert_eq!(&pkt[8..10], &[0x00, 0x35]);
        assert_eq!(&pkt[10..], b"hello");
    }

    #[test]
    fn test_build_socks5_udp_packet_v6() {
        let dst: SocketAddr = "[::1]:53".parse().unwrap();
        let pkt = build_socks5_udp_packet(dst, b"hello");
        assert_eq!(pkt[3], 0x04);
        assert_eq!(&pkt[4..20], &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(&pkt[20..22], &[0x00, 0x35]);
        assert_eq!(&pkt[22..], b"hello");
    }

    #[test]
    fn test_strip_socks5_udp_header_ipv4() {
        let mut pkt = vec![0, 0, 0, 0x01, 1, 2, 3, 4, 0x00, 0x35];
        pkt.extend_from_slice(b"response");
        let payload = strip_socks5_udp_header(&pkt).unwrap();
        assert_eq!(payload, b"response");
    }

    #[test]
    fn test_strip_socks5_udp_header_ipv6() {
        let mut pkt = vec![0, 0, 0, 0x04];
        pkt.extend_from_slice(&[0; 16]);
        pkt.extend_from_slice(&[0x00, 0x35]);
        pkt.extend_from_slice(b"response");
        let payload = strip_socks5_udp_header(&pkt).unwrap();
        assert_eq!(payload, b"response");
    }

    #[test]
    fn test_strip_fragment_rejected() {
        let pkt = [0, 0, 1, 0x01, 1, 2, 3, 4, 0, 53, 0, 0];
        assert!(strip_socks5_udp_header(&pkt).is_err());
    }
}
