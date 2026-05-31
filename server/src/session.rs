use axum::extract::ws::{Message, WebSocket};
use bytes::Bytes;
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use protocol::{
    TYPE_ICMP_DATA, TYPE_TCP_CONNECT, TYPE_TCP_CONNECTED, TYPE_TCP_DATA, TYPE_TCP_FIN,
    TYPE_UDP_DATA, decode_frame, decode_udp_payload, encode_frame,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::icmp;
use crate::tcp;
use crate::udp;

pub(crate) async fn handle_socket(socket: WebSocket) {
    let (ws_tx, mut ws_rx) = socket.split();

    let (frame_tx, mut frame_rx) = mpsc::channel::<Bytes>(1024);

    let mut ws_tx = ws_tx;
    tokio::spawn(async move {
        while let Some(frame) = frame_rx.recv().await {
            if ws_tx.send(Message::Binary(frame)).await.is_err() {
                break;
            }
        }
        ws_tx.close().await.ok();
    });

    let stream_map: Arc<DashMap<u32, mpsc::Sender<Bytes>>> = Arc::new(DashMap::new());
    let udp_sockets: Arc<DashMap<u32, Arc<tokio::net::UdpSocket>>> = Arc::new(DashMap::new());
    let udp_targets: Arc<DashMap<u32, SocketAddr>> = Arc::new(DashMap::new());
    let icmp_streams: Arc<DashMap<u32, mpsc::Sender<Bytes>>> = Arc::new(DashMap::new());
    let max_streams = Arc::new(tokio::sync::Semaphore::new(4096));

    while let Some(msg) = ws_rx.next().await {
        match msg {
            Ok(Message::Binary(data)) => {
                let (stream_id, typ, payload) = match decode_frame(data) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("[mux] decode: {e}");
                        continue;
                    }
                };

                match typ {
                    TYPE_TCP_CONNECT => {
                        let target = String::from_utf8_lossy(&payload).to_string();
                        debug!("[TCP→] {target} sid={stream_id}");

                        let permit = match max_streams.clone().try_acquire_owned() {
                            Ok(p) => p,
                            Err(_) => {
                                warn!("[mux] max streams reached, rejecting sid={stream_id}");
                                continue;
                            }
                        };

                        let (up_tx, up_rx) = mpsc::channel::<Bytes>(1024);
                        stream_map.insert(stream_id, up_tx);

                        let ftx = frame_tx.clone();
                        let sm = stream_map.clone();
                        tokio::spawn(async move {
                            let _permit = permit;
                            tcp::run(stream_id, target, up_rx, ftx).await;
                            sm.remove(&stream_id);
                        });
                    }

                    TYPE_TCP_DATA => {
                        let tx = stream_map.get(&stream_id).map(|r| r.clone());
                        if let Some(tx) = tx {
                            match tokio::time::timeout(Duration::from_secs(5), tx.send(payload))
                                .await
                            {
                                Ok(Ok(())) => {}
                                Ok(Err(_)) => {
                                    warn!(stream_id, "tcp data: stream receiver dropped");
                                }
                                Err(_) => {
                                    warn!(stream_id, "tcp data: backpressure timeout, dropping");
                                    stream_map.remove(&stream_id);
                                }
                            }
                        }
                    }

                    TYPE_TCP_FIN => {
                        stream_map.remove(&stream_id);
                    }

                    TYPE_UDP_DATA => match decode_udp_payload(&payload) {
                        Ok((host, port, data)) => {
                            let sock = match udp_sockets.entry(stream_id) {
                                dashmap::mapref::entry::Entry::Occupied(e) => e.get().clone(),
                                dashmap::mapref::entry::Entry::Vacant(e) => {
                                    let s = match udp::bind_socket().await {
                                        Some(s) => s,
                                        None => {
                                            warn!("[udp] bind failed for sid={stream_id}");
                                            continue;
                                        }
                                    };
                                    udp::spawn_recv_task(s.clone(), frame_tx.clone(), stream_id);
                                    e.insert(s.clone());
                                    s
                                }
                            };
                            let addr = match udp_targets.get(&stream_id) {
                                Some(a) => *a,
                                None => match tokio::net::lookup_host((host.clone(), port)).await {
                                    Ok(mut addrs) => match addrs.next() {
                                        Some(a) => {
                                            udp_targets.insert(stream_id, a);
                                            a
                                        }
                                        None => {
                                            warn!("[udp] resolve {host}:{port}: no addresses");
                                            continue;
                                        }
                                    },
                                    Err(e) => {
                                        warn!("[udp] resolve {host}:{port}: {e}");
                                        continue;
                                    }
                                },
                            };
                            tokio::spawn(async move {
                                if let Err(e) = sock.send_to(&data, addr).await {
                                    warn!("[udp] send to {addr}: {e}");
                                }
                            });
                        }
                        Err(e) => warn!("[udp] decode: {e}"),
                    },

                    TYPE_ICMP_DATA => {
                        if let Some(tx) = icmp_streams.get(&stream_id) {
                            // Existing stream: forward raw payload directly
                            if tx.send(payload).await.is_err() {
                                warn!(stream_id, "icmp: handler receiver dropped");
                            }
                        } else {
                            // First frame: payload should be target IP string
                            let target_str = match std::str::from_utf8(&payload) {
                                Ok(s) => s.trim().to_string(),
                                Err(_) => {
                                    debug!(
                                        stream_id,
                                        len = payload.len(),
                                        "icmp: ignoring binary data for unknown stream"
                                    );
                                    continue;
                                }
                            };
                            if target_str.parse::<std::net::SocketAddr>().is_err() {
                                debug!(
                                    stream_id,
                                    "icmp: ignoring invalid target: {}",
                                    target_str.chars().take(20).collect::<String>()
                                );
                                continue;
                            }

                            debug!("[ICMP→] {target_str} sid={stream_id}");

                            let permit = match max_streams.clone().try_acquire_owned() {
                                Ok(p) => p,
                                Err(_) => {
                                    warn!(
                                        "[mux] max streams reached, rejecting icmp sid={stream_id}"
                                    );
                                    continue;
                                }
                            };

                            let (tx, rx) = mpsc::channel::<Bytes>(256);
                            let ftx = frame_tx.clone();
                            let sm = icmp_streams.clone();
                            tokio::spawn(async move {
                                let _permit = permit;
                                icmp::run(stream_id, target_str, rx, ftx).await;
                                sm.remove(&stream_id);
                            });
                            icmp_streams.insert(stream_id, tx.clone());

                            // Send ack so client can proceed with SOCKS5 reply
                            let ack = encode_frame(stream_id, TYPE_TCP_CONNECTED, &[]);
                            if frame_tx.send(ack).await.is_err() {
                                warn!(stream_id, "icmp: failed to send ack");
                            }
                        }
                    }

                    _ => warn!("[mux] unknown type {typ:#x} sid={stream_id}"),
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                warn!("[ws] {e}");
                break;
            }
            _ => {}
        }
    }

    stream_map.clear();
    udp_sockets.clear();
    udp_targets.clear();
    icmp_streams.clear();
    info!("[ws] session closed");
}
