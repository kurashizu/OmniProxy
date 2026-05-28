use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

use crate::codec::{
    decode_frame, decode_udp_payload, encode_frame, encode_udp_payload, TYPE_TCP_CONNECT,
    TYPE_TCP_CONNECTED, TYPE_TCP_DATA, TYPE_TCP_FIN, TYPE_UDP_DATA, UDP_STREAM_ID,
};
use crate::config::Config;
use crate::ws::build_ws;

pub(crate) struct MuxInner {
    streams: HashMap<u32, mpsc::Sender<bytes::Bytes>>,
    connect_notify: HashMap<u32, oneshot::Sender<Result<()>>>,
    udp_tx: Option<mpsc::Sender<(String, u16, bytes::Bytes)>>,
    frame_tx: Option<mpsc::Sender<bytes::Bytes>>,
}

impl MuxInner {
    fn new() -> Self {
        MuxInner {
            streams: HashMap::new(),
            connect_notify: HashMap::new(),
            udp_tx: None,
            frame_tx: None,
        }
    }

    fn clear(&mut self) {
        self.streams.clear();
        self.connect_notify.clear();
        self.udp_tx = None;
        self.frame_tx = None;
    }
}

pub(crate) struct Mux {
    next_id: AtomicU32,
    inner: RwLock<MuxInner>,
    /// 当前配置（包含 server URL 和 outbound_ip）
    cfg: Config,
}

impl Mux {
    pub(crate) fn new(cfg: Config) -> Self {
        Mux {
            next_id: AtomicU32::new(1),
            inner: RwLock::new(MuxInner::new()),
            cfg,
        }
    }

    pub(crate) async fn connect_mux(cfg: &Config) -> Result<Arc<Self>> {
        let mux = Arc::new(Mux::new(cfg.clone()));
        let disc_rx = mux.connect().await?;
        spawn_reconnect_loop(mux.clone(), disc_rx);
        Ok(mux)
    }

    fn alloc_id(&self) -> u32 {
        loop {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            if id != UDP_STREAM_ID {
                return id;
            }
        }
    }

    async fn frame_tx(&self) -> Option<mpsc::Sender<bytes::Bytes>> {
        self.inner.read().await.frame_tx.clone()
    }

    async fn send_frame(&self, frame: bytes::Bytes) -> Result<()> {
        let tx = self.frame_tx().await.context("ws not connected")?;
        tx.send(frame)
            .await
            .map_err(|_| anyhow::anyhow!("ws writer closed"))
    }

    /// 建立 WS 连接。如果有 outbound_ip 但 bind 失败（网络切换导致 IP 失效），
    /// 则自动探测新出站 IP 并重试一次。
    pub(crate) async fn connect(self: &Arc<Self>) -> Result<oneshot::Receiver<()>> {
        let ws = build_ws(&self.cfg).await?;

        let (ws_tx, ws_rx) = ws.split();
        let (frame_tx, mut frame_rx) = mpsc::channel::<bytes::Bytes>(1024);

        let writer = tokio::spawn(async move {
            let mut ws_tx = ws_tx;
            let mut hb = tokio::time::interval(Duration::from_secs(20));
            hb.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    frame = frame_rx.recv() => match frame {
                        Some(f) => { if ws_tx.send(Message::Binary(f.into())).await.is_err() { break; } }
                        None    => break,
                    },
                    _ = hb.tick() => {
                        if ws_tx.send(Message::Ping(bytes::Bytes::new())).await.is_err() { break; }
                    }
                }
            }
            ws_tx.close().await.ok();
        });

        {
            self.inner.write().await.frame_tx = Some(frame_tx);
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
        mut ws_rx: impl StreamExt<Item = std::result::Result<Message, tokio_tungstenite::tungstenite::Error>>
            + Unpin
            + Send
            + 'static,
        writer: tokio::task::JoinHandle<()>,
    ) {
        loop {
            match ws_rx.next().await {
                Some(Ok(Message::Binary(data))) => {
                    let (id, typ, payload) = match decode_frame(&data) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("[mux] decode: {e}");
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
                            let inner = self.inner.read().await;
                            if let Some(tx) = inner.streams.get(&id) {
                                tx.send(payload).await.ok();
                            }
                        }
                        TYPE_TCP_FIN => {
                            self.inner.write().await.streams.remove(&id);
                        }
                        TYPE_UDP_DATA => {
                            if let Ok((host, port, data)) = decode_udp_payload(&payload) {
                                let inner = self.inner.read().await;
                                if let Some(tx) = &inner.udp_tx {
                                    tx.send((host, port, data)).await.ok();
                                }
                            }
                        }
                        _ => warn!("[mux] unknown type {typ:#x} sid={id}"),
                    }
                }
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                Some(Ok(Message::Close(_))) => {
                    info!("[mux] server closed ws");
                    break;
                }
                Some(Err(e)) => {
                    warn!("[mux] ws error: {e}");
                    break;
                }
                None => break,
                _ => {}
            }
        }
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
        let id = self.alloc_id();
        let (data_tx, data_rx) = mpsc::channel::<bytes::Bytes>(64);
        let (conn_tx, conn_rx) = oneshot::channel::<Result<()>>();
        {
            let mut inner = self.inner.write().await;
            inner.streams.insert(id, data_tx);
            inner.connect_notify.insert(id, conn_tx);
        }
        self.send_frame(encode_frame(id, TYPE_TCP_CONNECT, target.as_bytes()))
            .await?;
        Ok((id, data_rx, conn_rx))
    }

    pub(crate) async fn tcp_data(&self, id: u32, data: bytes::Bytes) -> Result<()> {
        self.send_frame(encode_frame(id, TYPE_TCP_DATA, &data))
            .await
    }

    pub(crate) async fn tcp_fin(&self, id: u32) {
        self.send_frame(encode_frame(id, TYPE_TCP_FIN, &[]))
            .await
            .ok();
        self.inner.write().await.streams.remove(&id);
    }

    pub(crate) async fn register_udp(&self) -> mpsc::Receiver<(String, u16, bytes::Bytes)> {
        let (tx, rx) = mpsc::channel(256);
        self.inner.write().await.udp_tx = Some(tx);
        rx
    }

    pub(crate) async fn udp_send(&self, host: &str, port: u16, data: &[u8]) -> Result<()> {
        let payload = encode_udp_payload(host, port, data);
        self.send_frame(encode_frame(UDP_STREAM_ID, TYPE_UDP_DATA, &payload))
            .await
    }
}

fn spawn_reconnect_loop(mux: Arc<Mux>, mut disc_rx: oneshot::Receiver<()>) {
    tokio::spawn(async move {
        let mut retry = 0u8;
        loop {
            let _ = disc_rx.await;
            if retry >= 5 {
                warn!("[mux] reconnect limit reached, stop retrying");
                break;
            }
            retry += 1;
            info!("[mux] disconnected, reconnecting in 3s...");
            tokio::time::sleep(Duration::from_secs(3)).await;
            loop {
                match mux.connect().await {
                    Ok(new_disc) => {
                        info!("[mux] reconnected");
                        disc_rx = new_disc;
                        break;
                    }
                    Err(e) => {
                        warn!("[mux] reconnect failed: {e:#}, retry in 5s");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }
    });
}
