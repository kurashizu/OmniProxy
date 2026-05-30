use bytes::Bytes;
use tracing::warn;

pub(crate) async fn run(
    _stream_id: u32,
    target: String,
    _in_rx: tokio::sync::mpsc::Receiver<Bytes>,
    _frame_tx: tokio::sync::mpsc::Sender<Bytes>,
) {
    warn!("[icmp] ICMP passthrough not supported on this platform: {target}");
}
