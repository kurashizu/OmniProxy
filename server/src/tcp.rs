use bytes::Bytes;
use protocol::{encode_frame, TYPE_TCP_CONNECTED, TYPE_TCP_DATA, TYPE_TCP_FIN};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tracing::{debug, warn};

const TCP_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) async fn run(
    stream_id: u32,
    target: String,
    mut up_rx: mpsc::Receiver<Bytes>,
    frame_tx: mpsc::Sender<Bytes>,
) {
    let upstream = match TcpStream::connect(&target).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[TCP] connect {target}: {e}");
            let msg = format!("{e}");
            frame_tx
                .send(encode_frame(stream_id, TYPE_TCP_CONNECTED, msg.as_bytes()))
                .await
                .ok();
            return;
        }
    };

    if frame_tx
        .send(encode_frame(stream_id, TYPE_TCP_CONNECTED, &[]))
        .await
        .is_err()
    {
        return;
    }

    let (mut ur, mut uw) = tokio::io::split(upstream);

    let ftx = frame_tx.clone();
    let down = tokio::spawn(async move {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            match tokio::time::timeout(TCP_IDLE_TIMEOUT, ur.read(&mut buf)).await {
                Ok(Ok(0)) | Ok(Err(_)) => break,
                Ok(Ok(n)) => {
                    let frame = encode_frame(stream_id, TYPE_TCP_DATA, &buf[..n]);
                    if ftx.send(frame).await.is_err() {
                        break;
                    }
                }
                Err(_) => {
                    debug!(stream_id, "[TCP] idle timeout (upstream read)");
                    break;
                }
            }
        }
        ftx.send(encode_frame(stream_id, TYPE_TCP_FIN, &[]))
            .await
            .ok();
    });

    let up = tokio::spawn(async move {
        loop {
            match tokio::time::timeout(TCP_IDLE_TIMEOUT, up_rx.recv()).await {
                Ok(Some(data)) => {
                    if uw.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    debug!(stream_id, "[TCP] idle timeout (mux read)");
                    break;
                }
            }
        }
        uw.shutdown().await.ok();
    });

    tokio::select! {
        _ = down => {}
        _ = up   => {}
    }
}
