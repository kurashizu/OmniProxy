use anyhow::Result;
use fast_socks5::server::{AuthMethodSuccessState, NoAuthentication, Socks5ServerProtocol};
use fast_socks5::util::target_addr::TargetAddr;
use fast_socks5::{ReplyError, Socks5Command};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::mux::Mux;

pub async fn handle(stream: TcpStream, mux: Arc<Mux>) -> Result<()> {
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

async fn handle_tcp(
    proto: fast_socks5::server::Socks5ServerProtocol<
        TcpStream,
        fast_socks5::server::states::CommandRead,
    >,
    target_addr: TargetAddr,
    mux: Arc<Mux>,
) -> Result<()> {
    let target = target_to_string(&target_addr);
    debug!(target, "socks5 tcp connect");

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
                .tcp_data(stream_id, bytes::Bytes::copy_from_slice(&buf[..n]))
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

async fn handle_udp(
    proto: fast_socks5::server::Socks5ServerProtocol<
        TcpStream,
        fast_socks5::server::states::CommandRead,
    >,
    mux: Arc<Mux>,
) -> Result<()> {
    debug!("socks5 udp associate");

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
                    warn!(error = %e, "udp bad header");
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
        _ = tcp_watch    => { debug!("udp control connection closed"); }
    }
    Ok(())
}

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
