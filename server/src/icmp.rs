use anyhow::{Context, Result};
use bytes::Bytes;
use protocol::{encode_frame, encode_icmp_payload, TYPE_ICMP_DATA};
use std::net::SocketAddr;
use std::os::fd::FromRawFd;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Create a raw ICMP socket.
/// Uses SOCK_RAW so we can receive ICMP echo replies (SOCK_DGRAM only gets errors).
/// IPv4 and IPv6 are both supported.
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

/// Run bidirectional ICMP forwarding for one stream.
///
/// - `in_rx`: receives raw ICMP payloads from the mux (client sends)
/// - `frame_tx`: sends ICMP_DATA frames back to the mux (server replies)
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

    // recv raw ICMP → encode source IP → send to mux
    // With SOCK_RAW, recv returns the full IP packet. We need to strip the IP header.
    let recv_sock = sock.clone();
    let recv_tx = frame_tx.clone();
    let recv_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            match recv_sock.recv_from(&mut buf).await {
                Ok((n, _src)) => {
                    if n < 20 {
                        continue;
                    }
                    // Parse IP header to find ICMP payload
                    let ihl = ((buf[0] & 0x0F) as usize) * 4;
                    if n < ihl + 8 {
                        continue;
                    }
                    let icmp_type = buf[ihl];
                    // Only forward echo replies (type 0)
                    if icmp_type != 0 {
                        debug!(
                            "[icmp] skipping non-echo type={} from {}",
                            icmp_type,
                            _src.ip()
                        );
                        continue;
                    }
                    let src_ip = _src.ip().to_string();
                    let icmp_data = &buf[ihl..n];
                    let payload = encode_icmp_payload(&src_ip, icmp_data);
                    let frame = encode_frame(stream_id, TYPE_ICMP_DATA, &payload);
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

    // mux → raw ICMP send
    // Raw ICMP send: data is ICMP header + payload (kernel adds IP header)
    let send_task = tokio::spawn(async move {
        while let Some(data) = in_rx.recv().await {
            if let Err(e) = sock.send_to(&data, &target_addr).await {
                warn!("[icmp] send to {target_addr}: {e}");
            } else {
                debug!("[ICMP→] {target_addr} {}B", data.len());
            }
        }
    });

    tokio::select! {
        _ = recv_task => {},
        _ = send_task => {},
    }

    debug!("[icmp] stream {stream_id} closed");
}
