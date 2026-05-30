use anyhow::{bail, Result};
use protocol::encode_icmp_payload;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tracing::{debug, warn};

use crate::mux::Mux;

// ── SOCKS5 constants ──────────────────────────────────────────────────────────

const SOCKS5_VERSION: u8 = 0x05;
const METHOD_NO_AUTH: u8 = 0x00;
const METHOD_NONE_ACCEPTABLE: u8 = 0xFF;

const CMD_CONNECT: u8 = 0x01;
const CMD_BIND: u8 = 0x02;
const CMD_UDP_ASSOCIATE: u8 = 0x03;
const CMD_ICMP: u8 = 0xA1;

const ATYP_IPV4: u8 = 0x01;
const ATYP_DOMAIN: u8 = 0x03;
const ATYP_IPV6: u8 = 0x04;

const REP_SUCCESS: u8 = 0x00;
const REP_GENERAL_FAILURE: u8 = 0x01;
const REP_COMMAND_NOT_SUPPORTED: u8 = 0x07;

// ── Types ─────────────────────────────────────────────────────────────────────

pub(crate) enum Socks5Cmd {
    Connect,
    Bind,
    UdpAssociate,
    Custom(u8),
}

pub(crate) enum TargetAddr {
    Ip(SocketAddr),
    Domain(String, u16),
}

// ── Protocol helpers ──────────────────────────────────────────────────────────

async fn read_socks5_handshake(stream: &mut TcpStream) -> Result<()> {
    // Client greeting: [VER][NMETHODS][METHODS...]
    let mut hdr = [0u8; 2];
    stream.read_exact(&mut hdr).await?;
    if hdr[0] != SOCKS5_VERSION {
        bail!("invalid socks5 version: {}", hdr[0]);
    }
    let nmethods = hdr[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream.read_exact(&mut methods).await?;

    // Only support No Auth
    if !methods.contains(&METHOD_NO_AUTH) {
        stream
            .write_all(&[SOCKS5_VERSION, METHOD_NONE_ACCEPTABLE])
            .await?;
        bail!("no acceptable auth method");
    }

    // Server selection
    stream
        .write_all(&[SOCKS5_VERSION, METHOD_NO_AUTH])
        .await?;
    Ok(())
}

async fn read_socks5_command(stream: &mut TcpStream) -> Result<(Socks5Cmd, TargetAddr)> {
    // [VER][CMD][RSV][ATYP]
    let mut hdr = [0u8; 4];
    stream.read_exact(&mut hdr).await?;
    if hdr[0] != SOCKS5_VERSION {
        bail!("invalid socks5 version: {}", hdr[0]);
    }
    if hdr[2] != 0x00 {
        bail!("RSV must be 0, got {}", hdr[2]);
    }

    let cmd = match hdr[1] {
        CMD_CONNECT => Socks5Cmd::Connect,
        CMD_BIND => Socks5Cmd::Bind,
        CMD_UDP_ASSOCIATE => Socks5Cmd::UdpAssociate,
        other => Socks5Cmd::Custom(other),
    };

    let target_addr = read_address(stream, hdr[3]).await?;
    Ok((cmd, target_addr))
}

async fn read_address(stream: &mut TcpStream, atyp: u8) -> Result<TargetAddr> {
    match atyp {
        ATYP_IPV4 => {
            let mut buf = [0u8; 6]; // 4 IP + 2 port
            stream.read_exact(&mut buf).await?;
            let ip = Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]);
            let port = u16::from_be_bytes([buf[4], buf[5]]);
            Ok(TargetAddr::Ip(SocketAddrV4::new(ip, port).into()))
        }
        ATYP_DOMAIN => {
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await?;
            let len = len_buf[0] as usize;
            let mut domain_buf = vec![0u8; len + 2]; // domain + 2 port
            stream.read_exact(&mut domain_buf).await?;
            let domain = String::from_utf8_lossy(&domain_buf[..len]).to_string();
            let port = u16::from_be_bytes([domain_buf[len], domain_buf[len + 1]]);
            Ok(TargetAddr::Domain(domain, port))
        }
        ATYP_IPV6 => {
            let mut buf = [0u8; 18]; // 16 IP + 2 port
            stream.read_exact(&mut buf).await?;
            let mut ip = [0u8; 16];
            ip.copy_from_slice(&buf[..16]);
            let port = u16::from_be_bytes([buf[16], buf[17]]);
            Ok(TargetAddr::Ip(
                SocketAddrV6::new(Ipv6Addr::from(ip), port, 0, 0).into(),
            ))
        }
        other => bail!("unknown ATYP: {other:#x}"),
    }
}

async fn write_socks5_reply(
    stream: &mut TcpStream,
    rep: u8,
    bind_addr: &SocketAddr,
) -> Result<()> {
    let mut resp = vec![SOCKS5_VERSION, rep, 0x00]; // VER, REP, RSV
    match bind_addr {
        SocketAddr::V4(addr) => {
            resp.push(ATYP_IPV4);
            resp.extend_from_slice(&addr.ip().octets());
            resp.extend_from_slice(&addr.port().to_be_bytes());
        }
        SocketAddr::V6(addr) => {
            resp.push(ATYP_IPV6);
            resp.extend_from_slice(&addr.ip().octets());
            resp.extend_from_slice(&addr.port().to_be_bytes());
        }
    }
    stream.write_all(&resp).await?;
    Ok(())
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn handle(mut stream: TcpStream, mux: Arc<Mux>) -> Result<()> {
    read_socks5_handshake(&mut stream).await?;
    let (cmd, target_addr) = read_socks5_command(&mut stream).await?;

    match cmd {
        Socks5Cmd::Connect => handle_tcp(stream, target_addr, mux).await,
        Socks5Cmd::UdpAssociate => handle_udp(stream, mux).await,
        Socks5Cmd::Custom(CMD_ICMP) => handle_icmp(stream, target_addr, mux).await,
        Socks5Cmd::Custom(c) => {
            let dummy: SocketAddr = "0.0.0.0:0".parse().unwrap();
            write_socks5_reply(&mut stream, REP_COMMAND_NOT_SUPPORTED, &dummy)
                .await
                .ok();
            bail!("unsupported cmd: {c:#x}")
        }
        _ => {
            let dummy: SocketAddr = "0.0.0.0:0".parse().unwrap();
            write_socks5_reply(&mut stream, REP_COMMAND_NOT_SUPPORTED, &dummy)
                .await
                .ok();
            bail!("unsupported cmd")
        }
    }
}

// ── TCP CONNECT ───────────────────────────────────────────────────────────────

async fn handle_tcp(
    mut stream: TcpStream,
    target_addr: TargetAddr,
    mux: Arc<Mux>,
) -> Result<()> {
    let target = target_to_string(&target_addr);
    debug!(target, "socks5 tcp connect");

    let (stream_id, mut data_rx, conn_rx) = match mux.tcp_connect(&target).await {
        Ok(v) => v,
        Err(e) => {
            let dummy: SocketAddr = "0.0.0.0:0".parse().unwrap();
            write_socks5_reply(&mut stream, REP_GENERAL_FAILURE, &dummy)
                .await
                .ok();
            return Err(e);
        }
    };

    match conn_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let dummy: SocketAddr = "0.0.0.0:0".parse().unwrap();
            write_socks5_reply(&mut stream, REP_GENERAL_FAILURE, &dummy)
                .await
                .ok();
            bail!("server connect failed: {e}");
        }
        Err(_) => {
            let dummy: SocketAddr = "0.0.0.0:0".parse().unwrap();
            write_socks5_reply(&mut stream, REP_GENERAL_FAILURE, &dummy)
                .await
                .ok();
            bail!("mux closed before connect ack");
        }
    }

    let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
    write_socks5_reply(&mut stream, REP_SUCCESS, &bind_addr).await?;

    let (mut lr, mut lw) = tokio::io::split(stream);
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

// ── UDP ASSOCIATE ─────────────────────────────────────────────────────────────

async fn handle_udp(mut stream: TcpStream, mux: Arc<Mux>) -> Result<()> {
    debug!("socks5 udp associate");

    let udp = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let local_addr = udp.local_addr()?;
    write_socks5_reply(&mut stream, REP_SUCCESS, &local_addr).await?;

    let (stream_id, mut udp_rx) = mux.register_udp().await;
    let client_addr: Arc<OnceLock<SocketAddr>> = Arc::new(OnceLock::new());

    let udp_recv = udp.clone();
    let mux_up = mux.clone();
    let ca_up = client_addr.clone();

    let local_to_mux = async move {
        let mut buf = vec![0u8; 65535];
        loop {
            let (n, src) = udp_recv.recv_from(&mut buf).await?;
            ca_up.get_or_init(|| src);
            let (host, port, data_offset) = match parse_socks5_udp_header(&buf[..n]) {
                Ok(v) => v,
                Err(e) => {
                    warn!(error = %e, "udp bad header");
                    continue;
                }
            };
            mux_up.udp_send(stream_id, &host, port, &buf[data_offset..n]).await?;
        }
        #[allow(unreachable_code)]
        anyhow::Ok(())
    };

    let udp_send = udp.clone();
    let ca_down = client_addr.clone();

    let mux_to_local = async move {
        while let Some((src_host, src_port, data)) = udp_rx.recv().await {
            let resp = build_socks5_udp_response(&src_host, src_port, &data);
            if let Some(a) = ca_down.get() {
                udp_send.send_to(&resp, a).await.ok();
            }
        }
        anyhow::Ok(())
    };

    let (mut tcp_rd, _) = tokio::io::split(stream);
    let tcp_watch = async move {
        let mut buf = [0u8; 1];
        let _ = tcp_rd.read(&mut buf).await;
    };

    tokio::select! {
        r = local_to_mux => { r.ok(); }
        r = mux_to_local => { r.ok(); }
        _ = tcp_watch    => { debug!("udp control connection closed"); }
    }

    mux.unregister_udp(stream_id).await;
    Ok(())
}

// ── ICMP ──────────────────────────────────────────────────────────────────────

async fn handle_icmp(
    mut stream: TcpStream,
    target_addr: TargetAddr,
    mux: Arc<Mux>,
) -> Result<()> {
    let target = target_to_string(&target_addr);
    debug!(target, "socks5 icmp");

    let (stream_id, mut icmp_rx, conn_rx) = match mux.icmp_register(&target).await {
        Ok(v) => v,
        Err(e) => {
            let dummy: SocketAddr = "0.0.0.0:0".parse().unwrap();
            write_socks5_reply(&mut stream, REP_GENERAL_FAILURE, &dummy)
                .await
                .ok();
            return Err(e);
        }
    };

    match conn_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let dummy: SocketAddr = "0.0.0.0:0".parse().unwrap();
            write_socks5_reply(&mut stream, REP_GENERAL_FAILURE, &dummy)
                .await
                .ok();
            bail!("server icmp connect failed: {e}");
        }
        Err(_) => {
            let dummy: SocketAddr = "0.0.0.0:0".parse().unwrap();
            write_socks5_reply(&mut stream, REP_GENERAL_FAILURE, &dummy)
                .await
                .ok();
            bail!("mux closed before icmp ack");
        }
    }

    let dummy: SocketAddr = "0.0.0.0:0".parse().unwrap();
    write_socks5_reply(&mut stream, REP_SUCCESS, &dummy).await?;

    let (mut tcp_rd, mut tcp_w) = tokio::io::split(stream);
    let mux_up = mux.clone();

    let up = async move {
        loop {
            let mut len_buf = [0u8; 4];
            if tcp_rd.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let len = u32::from_be_bytes(len_buf) as usize;
            if len == 0 || len > 65535 {
                break;
            }
            let mut icmp = vec![0u8; len];
            if tcp_rd.read_exact(&mut icmp).await.is_err() {
                break;
            }
            if mux_up
                .icmp_data(stream_id, bytes::Bytes::from(icmp))
                .await
                .is_err()
            {
                break;
            }
        }
        mux_up.icmp_unregister(stream_id).await;
        anyhow::Ok(())
    };

    let down = async move {
        while let Some((src_ip, data)) = icmp_rx.recv().await {
            let frame = encode_icmp_payload(&src_ip, &data);
            let len = (frame.len() as u32).to_be_bytes();
            tcp_w.write_all(&len).await?;
            tcp_w.write_all(&frame).await?;
        }
        anyhow::Ok(())
    };

    tokio::select! {
        r = up   => { r.ok(); }
        r = down => { r.ok(); }
    }
    Ok(())
}

// ── UDP header helpers ────────────────────────────────────────────────────────

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
    if let Ok(ip) = src_host.parse::<Ipv4Addr>() {
        resp.push(0x01);
        resp.extend_from_slice(&ip.octets());
    } else if let Ok(ip) = src_host.parse::<Ipv6Addr>() {
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
