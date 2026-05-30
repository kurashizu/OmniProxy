use bytes::Bytes;
use protocol::{TYPE_UDP_DATA, encode_frame_bytes, encode_udp_payload};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::warn;

pub(crate) async fn bind_socket() -> Option<Arc<UdpSocket>> {
    let sock = match UdpSocket::bind("[::]:0").await {
        Ok(s) => s,
        Err(_) => match UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(e) => {
                warn!("[udp] bind: {e}");
                return None;
            }
        },
    };

    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = sock.as_raw_fd();
        let size = (2 * 1024 * 1024) as libc::c_int;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVBUF,
                &size as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_SNDBUF,
                &size as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
    }

    Some(Arc::new(sock))
}

pub(crate) fn spawn_recv_task(sock: Arc<UdpSocket>, frame_tx: mpsc::Sender<Bytes>, stream_id: u32) {
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            let (n, src) = match sock.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(e) => {
                    warn!("[udp] recv: {e}");
                    break;
                }
            };
            let src_host = src.ip().to_string();
            let src_port = src.port();
            let payload = encode_udp_payload(&src_host, src_port, &buf[..n]);
            let frame = encode_frame_bytes(stream_id, TYPE_UDP_DATA, payload);
            if frame_tx.send(frame).await.is_err() {
                break;
            }
        }
    });
}
