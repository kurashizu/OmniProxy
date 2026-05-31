use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::ws::build_ws;
use protocol::{
    TYPE_ICMP_DATA, TYPE_TCP_CONNECT, TYPE_TCP_CONNECTED, TYPE_TCP_DATA, TYPE_TCP_FIN,
    TYPE_UDP_DATA, decode_frame, decode_icmp_payload, decode_udp_payload, encode_frame,
    encode_frame_bytes, encode_udp_payload,
};

const RATE_CHECK_INTERVAL: u64 = 16; // check every N frames
const RATE_HI_THRESHOLD: u64 = 100 * 1024; // 100 KB/s
const RATE_LO_THRESHOLD: u64 = 10 * 1024; // 10 KB/s

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamPriority {
    Mi, // low-volume, latency-sensitive
    Lo, // high-volume, bulk
}

struct RateTracker {
    bytes: AtomicU64,
    frames: AtomicU64,
    window_start: std::sync::Mutex<Instant>,
    priority: std::sync::Mutex<StreamPriority>,
}

impl RateTracker {
    fn new() -> Self {
        Self {
            bytes: AtomicU64::new(0),
            frames: AtomicU64::new(0),
            window_start: std::sync::Mutex::new(Instant::now()),
            priority: std::sync::Mutex::new(StreamPriority::Mi),
        }
    }

    fn record(&self, n: u64) {
        self.bytes.fetch_add(n, Ordering::Relaxed);
        let f = self.frames.fetch_add(1, Ordering::Relaxed) + 1;
        if f.is_multiple_of(RATE_CHECK_INTERVAL) {
            self.reclassify();
        }
    }

    fn reclassify(&self) {
        let mut start = self.window_start.lock().unwrap();
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if elapsed_ms == 0 {
            return;
        }
        let bytes = self.bytes.swap(0, Ordering::Relaxed);
        self.frames.store(0, Ordering::Relaxed);
        let rate = bytes * 1000 / elapsed_ms;
        let mut pri = self.priority.lock().unwrap();
        *pri = if rate >= RATE_HI_THRESHOLD {
            StreamPriority::Lo
        } else if rate <= RATE_LO_THRESHOLD {
            StreamPriority::Mi
        } else {
            *pri
        };
        *start = Instant::now();
    }

    fn priority(&self) -> StreamPriority {
        *self.priority.lock().unwrap()
    }
}

#[derive(Clone)]
pub(crate) struct StreamMeta {
    pub id: u32,
    pub protocol: &'static str,
    pub target: String,
    pub source: String,
    pub started_at: Instant,
}

impl StreamMeta {
    fn tcp(id: u32, target: String, source: String) -> Self {
        Self {
            id,
            protocol: "TCP",
            target,
            source,
            started_at: Instant::now(),
        }
    }

    fn udp(id: u32, source: String) -> Self {
        Self {
            id,
            protocol: "UDP",
            target: String::new(),
            source,
            started_at: Instant::now(),
        }
    }

    fn icmp(id: u32, target: String, source: String) -> Self {
        Self {
            id,
            protocol: "ICMP",
            target,
            source,
            started_at: Instant::now(),
        }
    }
}

pub(crate) struct Stats {
    pub started_at: Instant,
    pub ws_connected: AtomicBool,
    pub reconnect_count: AtomicU64,
    pub bytes_tx: AtomicU64,
    pub bytes_rx: AtomicU64,
}

impl Stats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started_at: Instant::now(),
            ws_connected: AtomicBool::new(false),
            reconnect_count: AtomicU64::new(0),
            bytes_tx: AtomicU64::new(0),
            bytes_rx: AtomicU64::new(0),
        })
    }
}

pub(crate) struct MuxInner {
    streams: HashMap<u32, mpsc::Sender<bytes::Bytes>>,
    connect_notify: HashMap<u32, oneshot::Sender<Result<()>>>,
    udp_txs: HashMap<u32, mpsc::Sender<(String, u16, bytes::Bytes)>>,
    icmp_txs: HashMap<u32, mpsc::Sender<(String, bytes::Bytes)>>,
    rate_trackers: HashMap<u32, Arc<RateTracker>>,
    frame_hi: Option<mpsc::Sender<bytes::Bytes>>,
    frame_mi: Option<mpsc::Sender<bytes::Bytes>>,
    frame_lo: Option<mpsc::Sender<bytes::Bytes>>,
    stream_info: HashMap<u32, StreamMeta>,
}

impl MuxInner {
    fn new() -> Self {
        MuxInner {
            streams: HashMap::new(),
            connect_notify: HashMap::new(),
            udp_txs: HashMap::new(),
            icmp_txs: HashMap::new(),
            rate_trackers: HashMap::new(),
            frame_hi: None,
            frame_mi: None,
            frame_lo: None,
            stream_info: HashMap::new(),
        }
    }

    fn clear(&mut self) {
        self.streams.clear();
        self.connect_notify.clear();
        self.udp_txs.clear();
        self.icmp_txs.clear();
        self.rate_trackers.clear();
        self.frame_hi = None;
        self.frame_mi = None;
        self.frame_lo = None;
        self.stream_info.clear();
    }

    pub(crate) fn stream_count(&self) -> usize {
        self.streams.len()
    }

    pub(crate) fn udp_count(&self) -> usize {
        self.udp_txs.len()
    }

    pub(crate) fn icmp_count(&self) -> usize {
        self.icmp_txs.len()
    }

    pub(crate) fn connections_snapshot(&self) -> Vec<StreamMeta> {
        let mut list: Vec<_> = self.stream_info.values().cloned().collect();
        list.sort_by_key(|s| std::cmp::Reverse(s.id));
        list
    }
}

pub(crate) struct Mux {
    next_id: AtomicU32,
    inner: RwLock<MuxInner>,
    /// Current config (includes server URL and outbound_ip)
    cfg: Config,
    stats: Arc<Stats>,
}

impl Mux {
    pub(crate) fn new(cfg: Config) -> Self {
        Mux {
            next_id: AtomicU32::new(1),
            inner: RwLock::new(MuxInner::new()),
            cfg,
            stats: Stats::new(),
        }
    }

    pub(crate) fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }

    pub(crate) fn inner(&self) -> &RwLock<MuxInner> {
        &self.inner
    }

    pub(crate) fn config(&self) -> &Config {
        &self.cfg
    }

    pub(crate) async fn connect_mux(cfg: &Config) -> Result<Arc<Self>> {
        let mux = Arc::new(Mux::new(cfg.clone()));

        const MAX_RETRIES: u32 = 2;
        const RETRY_DELAY: Duration = Duration::from_secs(5);
        let mut last_err = None;

        for attempt in 1..=MAX_RETRIES {
            match mux.connect().await {
                Ok(disc_rx) => {
                    spawn_reconnect_loop(mux.clone(), disc_rx);
                    return Ok(mux);
                }
                Err(e) => {
                    warn!(attempt, max = MAX_RETRIES, "connection failed: {e:#}");
                    last_err = Some(e);
                    if attempt < MAX_RETRIES {
                        tokio::time::sleep(RETRY_DELAY).await;
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("connection failed")))
    }

    fn alloc_id_locked(next: &AtomicU32, inner: &MuxInner) -> u32 {
        loop {
            let id = next.fetch_add(1, Ordering::Relaxed);
            if id == 0 {
                continue;
            }
            if !inner.streams.contains_key(&id)
                && !inner.connect_notify.contains_key(&id)
                && !inner.udp_txs.contains_key(&id)
                && !inner.icmp_txs.contains_key(&id)
                && !inner.rate_trackers.contains_key(&id)
            {
                return id;
            }
        }
    }

    async fn send_hi(&self, frame: bytes::Bytes) -> Result<()> {
        let len = frame.len() as u64;
        let tx = self
            .inner
            .read()
            .await
            .frame_hi
            .clone()
            .context("ws not connected")?;
        tx.send(frame)
            .await
            .map_err(|_| anyhow::anyhow!("ws writer closed"))?;
        self.stats.bytes_tx.fetch_add(len, Ordering::Relaxed);
        Ok(())
    }

    async fn send_mi(&self, frame: bytes::Bytes) -> Result<()> {
        let len = frame.len() as u64;
        let tx = self
            .inner
            .read()
            .await
            .frame_mi
            .clone()
            .context("ws not connected")?;
        tx.send(frame)
            .await
            .map_err(|_| anyhow::anyhow!("ws writer closed"))?;
        self.stats.bytes_tx.fetch_add(len, Ordering::Relaxed);
        Ok(())
    }

    async fn send_lo(&self, frame: bytes::Bytes) -> Result<()> {
        let len = frame.len() as u64;
        let tx = self
            .inner
            .read()
            .await
            .frame_lo
            .clone()
            .context("ws not connected")?;
        tx.send(frame)
            .await
            .map_err(|_| anyhow::anyhow!("ws writer closed"))?;
        self.stats.bytes_tx.fetch_add(len, Ordering::Relaxed);
        Ok(())
    }

    async fn send_data_with_rate(&self, id: u32, frame: bytes::Bytes) -> Result<()> {
        let tracker = {
            let inner = self.inner.read().await;
            inner.rate_trackers.get(&id).cloned()
        };
        match tracker {
            Some(tracker) => {
                let payload_len = frame.len() as u64;
                tracker.record(payload_len);
                match tracker.priority() {
                    StreamPriority::Mi => self.send_mi(frame).await,
                    StreamPriority::Lo => self.send_lo(frame).await,
                }
            }
            None => self.send_mi(frame).await,
        }
    }

    /// Establish WS connection. If outbound_ip bind fails (e.g. network change
    /// invalidated the IP), auto-detect the new outbound IP and retry once.
    pub(crate) async fn connect(self: &Arc<Self>) -> Result<oneshot::Receiver<()>> {
        debug!("establishing ws connection");
        let ws = build_ws(&self.cfg).await?;
        debug!("ws connected");

        let (ws_tx, ws_rx) = ws.split();
        // Three priority channels:
        //   hi: control (connect/fin/icmp register) — capacity 64
        //   mi: low-volume interactive data — capacity 256
        //   lo: high-volume bulk data — capacity 1024
        let (hi_tx, mut hi_rx) = mpsc::channel::<bytes::Bytes>(64);
        let (mi_tx, mut mi_rx) = mpsc::channel::<bytes::Bytes>(256);
        let (lo_tx, mut lo_rx) = mpsc::channel::<bytes::Bytes>(1024);

        let writer = tokio::spawn(async move {
            let mut ws_tx = ws_tx;
            let mut hb = tokio::time::interval(Duration::from_secs(20));
            hb.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            const MI_LIMIT: usize = 16;
            loop {
                // Drain hi completely before touching mi/lo
                while let Ok(f) = hi_rx.try_recv() {
                    if ws_tx.send(Message::Binary(f)).await.is_err() {
                        return;
                    }
                }
                // Drain mi with a limit so lo is not starved
                let mut mi_drained = 0usize;
                loop {
                    if mi_drained >= MI_LIMIT {
                        break;
                    }
                    match mi_rx.try_recv() {
                        Ok(f) => {
                            mi_drained += 1;
                            if ws_tx.send(Message::Binary(f)).await.is_err() {
                                return;
                            }
                        }
                        Err(_) => break,
                    }
                }
                // Drain lo
                while let Ok(f) = lo_rx.try_recv() {
                    if ws_tx.send(Message::Binary(f)).await.is_err() {
                        return;
                    }
                }
                // Wait for next event
                tokio::select! {
                    biased;
                    f = hi_rx.recv() => match f {
                        Some(f) => { if ws_tx.send(Message::Binary(f)).await.is_err() { break; } }
                        None    => break,
                    },
                    f = mi_rx.recv() => match f {
                        Some(f) => {
                            // Drain any hi that arrived while waiting
                            while let Ok(h) = hi_rx.try_recv() {
                                if ws_tx.send(Message::Binary(h)).await.is_err() { return; }
                            }
                            if ws_tx.send(Message::Binary(f)).await.is_err() { break; }
                        }
                        None => break,
                    },
                    f = lo_rx.recv() => match f {
                        Some(f) => {
                            while let Ok(h) = hi_rx.try_recv() {
                                if ws_tx.send(Message::Binary(h)).await.is_err() { return; }
                            }
                            if ws_tx.send(Message::Binary(f)).await.is_err() { break; }
                        }
                        None => break,
                    },
                    _ = hb.tick() => {
                        if ws_tx.send(Message::Ping(bytes::Bytes::new())).await.is_err() { break; }
                    }
                }
            }
            ws_tx.close().await.ok();
        });

        self.stats.ws_connected.store(true, Ordering::Relaxed);
        {
            let mut inner = self.inner.write().await;
            inner.frame_hi = Some(hi_tx);
            inner.frame_mi = Some(mi_tx);
            inner.frame_lo = Some(lo_tx);
        }

        let (disc_tx, disc_rx) = oneshot::channel::<()>();
        let mux = self.clone();
        tokio::spawn(async move {
            mux.dispatch(ws_rx, writer).await;
            disc_tx.send(()).ok();
        });

        Ok(disc_rx)
    }

    async fn dispatch(
        self: &Arc<Self>,
        mut ws_rx: impl StreamExt<
            Item = std::result::Result<Message, tokio_tungstenite::tungstenite::Error>,
        > + Unpin
        + Send
        + 'static,
        writer: tokio::task::JoinHandle<()>,
    ) {
        loop {
            match ws_rx.next().await {
                Some(Ok(Message::Binary(data))) => {
                    self.stats
                        .bytes_rx
                        .fetch_add(data.len() as u64, Ordering::Relaxed);
                    let (id, typ, payload) = match decode_frame(data) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!(error = %e, "decode error");
                            continue;
                        }
                    };
                    match typ {
                        TYPE_TCP_CONNECTED => {
                            let mut inner = self.inner.write().await;
                            if let Some(tx) = inner.connect_notify.remove(&id) {
                                let r: Result<()> = if payload.is_empty() {
                                    Ok(())
                                } else {
                                    Err(anyhow::anyhow!("{}", String::from_utf8_lossy(&payload)))
                                };
                                tx.send(r).ok();
                            }
                        }
                        TYPE_TCP_DATA => {
                            let tx = {
                                let inner = self.inner.read().await;
                                inner.streams.get(&id).cloned()
                            };
                            if let Some(tx) = tx
                                && tx.send(payload).await.is_err()
                            {
                                warn!(id, "tcp data: stream receiver dropped, sending FIN");
                                self.tcp_fin(id).await;
                            }
                        }
                        TYPE_TCP_FIN => {
                            let mut inner = self.inner.write().await;
                            inner.streams.remove(&id);
                            inner.stream_info.remove(&id);
                        }
                        TYPE_UDP_DATA => {
                            let tx = {
                                let inner = self.inner.read().await;
                                inner.udp_txs.get(&id).cloned()
                            };
                            if let Some(tx) = tx
                                && let Ok((host, port, data)) = decode_udp_payload(&payload)
                                && tx.send((host, port, data)).await.is_err()
                            {
                                warn!(id, "udp data: session receiver dropped, cleaning up");
                                let mut inner = self.inner.write().await;
                                inner.udp_txs.remove(&id);
                            }
                        }
                        TYPE_ICMP_DATA => {
                            let tx = {
                                let inner = self.inner.read().await;
                                inner.icmp_txs.get(&id).cloned()
                            };
                            if let Some(tx) = tx
                                && let Ok((ip, data)) = decode_icmp_payload(&payload)
                                && tx.send((ip, data)).await.is_err()
                            {
                                warn!(id, "icmp data: session receiver dropped, cleaning up");
                                let mut inner = self.inner.write().await;
                                inner.icmp_txs.remove(&id);
                            }
                        }
                        _ => warn!(typ = format!("{typ:#x}"), id, "unknown frame type"),
                    }
                }
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                Some(Ok(Message::Close(_))) => {
                    info!("server closed ws");
                    break;
                }
                Some(Err(e)) => {
                    warn!(error = %e, "ws error");
                    break;
                }
                None => break,
                _ => {}
            }
        }
        self.stats.ws_connected.store(false, Ordering::Relaxed);
        writer.abort();
        self.inner.write().await.clear();
    }

    pub(crate) async fn tcp_connect(
        &self,
        target: &str,
    ) -> Result<(
        u32,
        mpsc::Receiver<bytes::Bytes>,
        oneshot::Receiver<Result<()>>,
    )> {
        let (data_tx, data_rx) = mpsc::channel::<bytes::Bytes>(1024);
        let (conn_tx, conn_rx) = oneshot::channel::<Result<()>>();
        let id = {
            let mut inner = self.inner.write().await;
            let id = Self::alloc_id_locked(&self.next_id, &inner);
            inner.streams.insert(id, data_tx);
            inner.rate_trackers.insert(id, Arc::new(RateTracker::new()));
            inner
                .stream_info
                .insert(id, StreamMeta::tcp(id, target.to_owned(), String::new()));
            inner.connect_notify.insert(id, conn_tx);
            id
        };
        debug!(id, target, "tcp connect");
        self.send_hi(encode_frame(id, TYPE_TCP_CONNECT, target.as_bytes()))
            .await?;
        Ok((id, data_rx, conn_rx))
    }

    pub(crate) async fn tcp_data(&self, id: u32, data: bytes::Bytes) -> Result<()> {
        self.send_data_with_rate(id, encode_frame_bytes(id, TYPE_TCP_DATA, data))
            .await
    }

    pub(crate) async fn tcp_fin(&self, id: u32) {
        debug!(id, "tcp fin");
        self.send_hi(encode_frame(id, TYPE_TCP_FIN, &[])).await.ok();
        let mut inner = self.inner.write().await;
        inner.streams.remove(&id);
        inner.rate_trackers.remove(&id);
        inner.stream_info.remove(&id);
    }

    pub(crate) async fn register_udp(&self) -> (u32, mpsc::Receiver<(String, u16, bytes::Bytes)>) {
        let (tx, rx) = mpsc::channel(256);
        let mut inner = self.inner.write().await;
        let id = Self::alloc_id_locked(&self.next_id, &inner);
        inner.udp_txs.insert(id, tx);
        inner
            .stream_info
            .insert(id, StreamMeta::udp(id, String::new()));
        (id, rx)
    }

    pub(crate) async fn unregister_udp(&self, id: u32) {
        let mut inner = self.inner.write().await;
        inner.udp_txs.remove(&id);
        inner.stream_info.remove(&id);
    }

    pub(crate) async fn udp_send(
        &self,
        stream_id: u32,
        host: &str,
        port: u16,
        data: &[u8],
    ) -> Result<()> {
        debug!(stream_id, host, port, "udp send");
        let payload = encode_udp_payload(host, port, data);
        self.send_mi(encode_frame_bytes(stream_id, TYPE_UDP_DATA, payload))
            .await
    }

    pub(crate) async fn icmp_register(
        &self,
        target: &str,
    ) -> Result<(
        u32,
        mpsc::Receiver<(String, bytes::Bytes)>,
        oneshot::Receiver<Result<()>>,
    )> {
        let (tx, rx) = mpsc::channel(256);
        let (conn_tx, conn_rx) = oneshot::channel();
        let id = {
            let mut inner = self.inner.write().await;
            let id = Self::alloc_id_locked(&self.next_id, &inner);
            inner.icmp_txs.insert(id, tx);
            inner
                .stream_info
                .insert(id, StreamMeta::icmp(id, target.to_owned(), String::new()));
            inner.connect_notify.insert(id, conn_tx);
            id
        };
        self.send_hi(encode_frame(id, TYPE_ICMP_DATA, target.as_bytes()))
            .await?;
        Ok((id, rx, conn_rx))
    }

    pub(crate) async fn icmp_data(&self, id: u32, data: bytes::Bytes) -> Result<()> {
        self.send_mi(encode_frame(id, TYPE_ICMP_DATA, &data)).await
    }

    pub(crate) async fn icmp_unregister(&self, id: u32) {
        let mut inner = self.inner.write().await;
        inner.icmp_txs.remove(&id);
        inner.stream_info.remove(&id);
    }
}

fn spawn_reconnect_loop(mux: Arc<Mux>, mut disc_rx: oneshot::Receiver<()>) {
    tokio::spawn(async move {
        let mut delay = Duration::from_secs(1);
        let max_delay = Duration::from_secs(30);
        loop {
            let _ = disc_rx.await;
            info!("disconnected, reconnecting in {}s...", delay.as_secs());
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(max_delay);
            loop {
                match mux.connect().await {
                    Ok(new_disc) => {
                        info!("reconnected");
                        mux.stats.reconnect_count.fetch_add(1, Ordering::Relaxed);
                        disc_rx = new_disc;
                        delay = Duration::from_secs(1);
                        break;
                    }
                    Err(e) => {
                        warn!(error = %e, "reconnect failed, retry in {}s", delay.as_secs());
                        tokio::time::sleep(delay).await;
                        delay = (delay * 2).min(max_delay);
                    }
                }
            }
        }
    });
}
