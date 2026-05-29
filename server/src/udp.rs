use bytes::Bytes;
use protocol::{encode_frame, encode_udp_payload, TYPE_UDP_DATA, UDP_STREAM_ID};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::warn;

pub(crate) async fn bind_socket() -> Option<Arc<UdpSocket>> {
    match UdpSocket::bind("[::]:0").await {
        Ok(s) => Some(Arc::new(s)),
        Err(_) => match UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => Some(Arc::new(s)),
            Err(e) => {
                warn!("[udp] bind: {e}");
                None
            }
        },
    }
}

pub(crate) fn spawn_recv_task(sock: Arc<UdpSocket>, frame_tx: mpsc::Sender<Bytes>) {
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
            let frame = encode_frame(UDP_STREAM_ID, TYPE_UDP_DATA, &payload);
            if frame_tx.send(frame).await.is_err() {
                break;
            }
        }
    });
}
