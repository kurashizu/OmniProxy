use anyhow::{Context, Result};
use bytes::Bytes;
use protocol::{TYPE_ICMP_DATA, encode_frame_bytes, encode_icmp_payload};
use std::net::SocketAddr;
use std::os::fd::FromRawFd;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, warn};

pub(crate) async fn open_raw_icmp(target: &str) -> Result<Arc<UdpSocket>> {
    let addr: SocketAddr = target
        .parse()
        .context(format!("parse icmp target: {target}"))?;

    let fd = match addr {
        SocketAddr::V4(_) => unsafe {
            libc::socket(libc::AF_INET, libc::SOCK_RAW, libc::IPPROTO_ICMP)
        },
        SocketAddr::V6(_) => unsafe {
            libc::socket(libc::AF_INET6, libc::SOCK_RAW, libc::IPPROTO_ICMPV6)
        },
    };

    if fd < 0 {
        anyhow::bail!(
            "failed to create icmp raw socket for {target}: {}",
            std::io::Error::last_os_error()
        );
    }

    let std_sock = unsafe { std::net::UdpSocket::from_raw_fd(fd) };
    std_sock.set_nonblocking(true)?;
    Ok(Arc::new(UdpSocket::from_std(std_sock)?))
}

pub(crate) async fn run(
    stream_id: u32,
    target: String,
    mut in_rx: mpsc::Receiver<Bytes>,
    frame_tx: mpsc::Sender<Bytes>,
) {
    let sock = match open_raw_icmp(&target).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[icmp] open for {target}: {e}");
            return;
        }
    };

    let target_addr: SocketAddr = match target.parse() {
        Ok(a) => a,
        Err(e) => {
            warn!("[icmp] parse target {target}: {e}");
            return;
        }
    };

    // recv raw ICMP -> encode source IP -> send to mux
    // IPv4 SOCK_RAW: recv includes IP header, must strip it.
    // IPv6 SOCK_RAW: recv does NOT include IPv6 header, only ICMPv6 payload.
    let target_ip = target_addr.ip();
    let is_v6 = target_ip.is_ipv6();
    let recv_sock = sock.clone();
    let recv_tx = frame_tx.clone();
    let recv_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            match recv_sock.recv_from(&mut buf).await {
                Ok((n, src)) => {
                    if src.ip() != target_ip {
                        continue;
                    }
                    let (header_len, echo_reply_type) = if is_v6 {
                        (0, 129u8)
                    } else {
                        let ihl = ((buf[0] & 0x0F) as usize) * 4;
                        (ihl, 0)
                    };
                    if n < header_len + 8 {
                        continue;
                    }
                    let icmp_type = buf[header_len];
                    if icmp_type != echo_reply_type {
                        debug!(
                            "[icmp] skipping non-echo type={} from {}",
                            icmp_type,
                            src.ip()
                        );
                        continue;
                    }
                    let src_ip = src.ip().to_string();
                    let icmp_data = &buf[header_len..n];
                    let payload = encode_icmp_payload(&src_ip, icmp_data);
                    let frame = encode_frame_bytes(stream_id, TYPE_ICMP_DATA, payload);
                    if recv_tx.send(frame).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("[icmp] recv: {e}");
                    break;
                }
            }
        }
    });

    // mux -> raw ICMP send
    let send_task = tokio::spawn(async move {
        while let Some(data) = in_rx.recv().await {
            if let Err(e) = sock.send_to(&data, &target_addr).await {
                warn!("[icmp] send to {target_addr}: {e}");
            } else {
                debug!("[ICMP->] {target_addr} {}B", data.len());
            }
        }
    });

    tokio::select! {
        _ = recv_task => {},
        _ = send_task => {},
    }

    debug!("[icmp] stream {stream_id} closed");
}
