use axum::extract::ws::{Message, WebSocket};
use bytes::Bytes;
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use protocol::{
    decode_frame, decode_udp_payload, TYPE_TCP_CONNECT, TYPE_TCP_DATA, TYPE_TCP_FIN, TYPE_UDP_DATA,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::tcp;
use crate::udp;

pub(crate) async fn handle_socket(socket: WebSocket) {
    let (ws_tx, mut ws_rx) = socket.split();

    let (frame_tx, mut frame_rx) = mpsc::channel::<Bytes>(1024);

    let mut ws_tx = ws_tx;
    tokio::spawn(async move {
        while let Some(frame) = frame_rx.recv().await {
            if ws_tx.send(Message::Binary(frame.into())).await.is_err() {
                break;
            }
        }
        ws_tx.close().await.ok();
    });

    let stream_map: Arc<DashMap<u32, mpsc::Sender<Bytes>>> = Arc::new(DashMap::new());

    let udp_sock = match udp::bind_socket().await {
        Some(s) => s,
        None => return,
    };
    udp::spawn_recv_task(udp_sock.clone(), frame_tx.clone());

    while let Some(msg) = ws_rx.next().await {
        match msg {
            Ok(Message::Binary(data)) => {
                let (stream_id, typ, payload) = match decode_frame(&data) {
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

                        let (up_tx, up_rx) = mpsc::channel::<Bytes>(64);
                        stream_map.insert(stream_id, up_tx);

                        let ftx = frame_tx.clone();
                        let sm = stream_map.clone();
                        tokio::spawn(async move {
                            tcp::run(stream_id, target, up_rx, ftx).await;
                            sm.remove(&stream_id);
                        });
                    }

                    TYPE_TCP_DATA => {
                        if let Some(tx) = stream_map.get(&stream_id) {
                            tx.send(payload).await.ok();
                        }
                    }

                    TYPE_TCP_FIN => {
                        stream_map.remove(&stream_id);
                    }

                    TYPE_UDP_DATA => match decode_udp_payload(&payload) {
                        Ok((host, port, data)) => {
                            let target = format!("{host}:{port}");
                            let sock = udp_sock.clone();
                            tokio::spawn(async move {
                                if let Err(e) = sock.send_to(&data, &target).await {
                                    warn!("[udp] send to {target}: {e}");
                                } else {
                                    debug!("[UDP→] {target} {}B", data.len());
                                }
                            });
                        }
                        Err(e) => warn!("[udp] decode: {e}"),
                    },

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
    info!("[ws] session closed");
}
