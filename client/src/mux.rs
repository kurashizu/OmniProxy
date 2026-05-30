use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::ws::build_ws;
use protocol::{
    decode_frame, decode_icmp_payload, decode_udp_payload, encode_frame, encode_udp_payload,
    TYPE_ICMP_DATA, TYPE_TCP_CONNECT, TYPE_TCP_CONNECTED, TYPE_TCP_DATA, TYPE_TCP_FIN, TYPE_UDP_DATA,
};

pub(crate) struct MuxInner {
    streams: HashMap<u32, mpsc::Sender<bytes::Bytes>>,
    connect_notify: HashMap<u32, oneshot::Sender<Result<()>>>,
    udp_txs: HashMap<u32, mpsc::Sender<(String, u16, bytes::Bytes)>>,
    icmp_txs: HashMap<u32, mpsc::Sender<(String, bytes::Bytes)>>,
    frame_tx: Option<mpsc::Sender<bytes::Bytes>>,
}

impl MuxInner {
    fn new() -> Self {
        MuxInner {
            streams: HashMap::new(),
            connect_notify: HashMap::new(),
            udp_txs: HashMap::new(),
            icmp_txs: HashMap::new(),
            frame_tx: None,
        }
    }

    fn clear(&mut self) {
        self.streams.clear();
        self.connect_notify.clear();
        self.udp_txs.clear();
        self.icmp_txs.clear();
        self.frame_tx = None;
    }
}

pub(crate) struct Mux {
    next_id: AtomicU32,
    inner: RwLock<MuxInner>,
    /// Current config (includes server URL and outbound_ip)
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
        self.next_id.fetch_add(1, Ordering::Relaxed)
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

    /// Establish WS connection. If outbound_ip bind fails (e.g. network change
    /// invalidated the IP), auto-detect the new outbound IP and retry once.
    pub(crate) async fn connect(self: &Arc<Self>) -> Result<oneshot::Receiver<()>> {
        debug!("establishing ws connection");
        let ws = build_ws(&self.cfg).await?;
        debug!("ws connected");

        let (ws_tx, ws_rx) = ws.split();
        let (frame_tx, mut frame_rx) = mpsc::channel::<bytes::Bytes>(1024);

        let writer = tokio::spawn(async move {
            let mut ws_tx = ws_tx;
            let mut hb = tokio::time::interval(Duration::from_secs(20));
            hb.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    frame = frame_rx.recv() => match frame {
                        Some(f) => { if ws_tx.send(Message::Binary(f)).await.is_err() { break; } }
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
                                warn!(id, "tcp data: stream receiver dropped");
                            }
                        }
                        TYPE_TCP_FIN => {
                            self.inner.write().await.streams.remove(&id);
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
                                warn!(id, "udp data: session receiver dropped");
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
                                warn!(id, "icmp data: session receiver dropped");
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
        debug!(id, target, "tcp connect");
        let (data_tx, data_rx) = mpsc::channel::<bytes::Bytes>(1024);
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
        debug!(id, "tcp fin");
        self.send_frame(encode_frame(id, TYPE_TCP_FIN, &[]))
            .await
            .ok();
        self.inner.write().await.streams.remove(&id);
    }

    pub(crate) async fn register_udp(&self) -> (u32, mpsc::Receiver<(String, u16, bytes::Bytes)>) {
        let id = self.alloc_id();
        let (tx, rx) = mpsc::channel(256);
        self.inner.write().await.udp_txs.insert(id, tx);
        (id, rx)
    }

    pub(crate) async fn unregister_udp(&self, id: u32) {
        self.inner.write().await.udp_txs.remove(&id);
    }

    pub(crate) async fn udp_send(&self, stream_id: u32, host: &str, port: u16, data: &[u8]) -> Result<()> {
        debug!(stream_id, host, port, "udp send");
        let payload = encode_udp_payload(host, port, data);
        self.send_frame(encode_frame(stream_id, TYPE_UDP_DATA, &payload))
            .await
    }

    pub(crate) async fn icmp_register(&self) -> (u32, mpsc::Receiver<(String, bytes::Bytes)>) {
        let id = self.alloc_id();
        let (tx, rx) = mpsc::channel(256);
        self.inner.write().await.icmp_txs.insert(id, tx);
        (id, rx)
    }

    pub(crate) async fn icmp_data(&self, id: u32, data: bytes::Bytes) -> Result<()> {
        self.send_frame(encode_frame(id, TYPE_ICMP_DATA, &data))
            .await
    }

    pub(crate) async fn icmp_unregister(&self, id: u32) {
        self.inner.write().await.icmp_txs.remove(&id);
    }
}

fn spawn_reconnect_loop(mux: Arc<Mux>, mut disc_rx: oneshot::Receiver<()>) {
    tokio::spawn(async move {
        let mut retry = 0u8;
        loop {
            let _ = disc_rx.await;
            if retry >= 5 {
                warn!("reconnect limit reached, stop retrying");
                break;
            }
            retry += 1;
            info!("disconnected, reconnecting in 3s...");
            tokio::time::sleep(Duration::from_secs(3)).await;
            loop {
                match mux.connect().await {
                    Ok(new_disc) => {
                        info!("reconnected");
                        disc_rx = new_disc;
                        break;
                    }
                    Err(e) => {
                        warn!(error = %e, "reconnect failed, retry in 5s");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }
    });
}
