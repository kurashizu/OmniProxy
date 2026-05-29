use bytes::Bytes;
use protocol::{encode_frame, TYPE_TCP_CONNECTED, TYPE_TCP_DATA, TYPE_TCP_FIN};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::warn;

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
            match ur.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let frame = encode_frame(stream_id, TYPE_TCP_DATA, &buf[..n]);
                    if ftx.send(frame).await.is_err() {
                        break;
                    }
                }
            }
        }
        ftx.send(encode_frame(stream_id, TYPE_TCP_FIN, &[]))
            .await
            .ok();
    });

    let up = tokio::spawn(async move {
        while let Some(data) = up_rx.recv().await {
            if uw.write_all(&data).await.is_err() {
                break;
            }
        }
        uw.shutdown().await.ok();
    });

    tokio::select! {
        _ = down => {}
        _ = up   => {}
    }
}
