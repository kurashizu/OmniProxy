use anyhow::{Context, Result};
use bytes::BytesMut;
use protocol::decode_icmp_payload;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const SESSION_TIMEOUT: Duration = Duration::from_secs(120);
const CMD_ICMP: u8 = 0xA1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SessionKey {
    dst: IpAddr,
}

struct IcmpSession {
    writer: OwnedWriteHalf,
    last_active: Instant,
    original_pkt: Vec<u8>,
}

pub(crate) struct IcmpHandler {
    sessions: HashMap<SessionKey, IcmpSession>,
    socks_port: u16,
    inbound_rx: mpsc::Receiver<(IpAddr, IpAddr, Vec<u8>)>,
    inbound_tx: mpsc::Sender<(IpAddr, IpAddr, Vec<u8>)>,
    outbound_rx: mpsc::Receiver<Vec<u8>>,
    tun_write_tx: mpsc::Sender<BytesMut>,
}

impl IcmpHandler {
    /// Create a new ICMP handler.
    /// Returns `(handler, outbound_tx)` where `outbound_tx` is used to send raw IP ICMP packets
    /// from the TUN demux to this handler.
    pub(crate) fn new(
        socks_port: u16,
        tun_write_tx: mpsc::Sender<BytesMut>,
    ) -> (Self, mpsc::Sender<Vec<u8>>) {
        let (inbound_tx, inbound_rx) = mpsc::channel(256);
        let (outbound_tx, outbound_rx) = mpsc::channel(256);
        (
            Self {
                sessions: HashMap::new(),
                socks_port,
                inbound_rx,
                inbound_tx,
                outbound_rx,
                tun_write_tx,
            },
            outbound_tx,
        )
    }

    /// Run the ICMP event loop. Handles both directions:
    /// - Outbound: reads raw IP ICMP packets from TUN, strips IP header, sends to SOCKS5 client
    /// - Inbound: reads ICMP payloads from SOCKS5 clients, rebuilds IP packets, writes to TUN
    pub(crate) async fn run(mut self) {
        let mut reap_interval = tokio::time::interval(SESSION_TIMEOUT);
        reap_interval.tick().await;

        loop {
            tokio::select! {
                _ = reap_interval.tick() => {
                    self.reap_idle();
                }
                Some(raw_ip_pkt) = self.outbound_rx.recv() => {
                    if let Err(e) = self.handle_outbound(&raw_ip_pkt).await {
                        debug!("[icmp] outbound: {e}");
                    }
                }
                Some((dst, src_ip, icmp_payload)) = self.inbound_rx.recv() => {
                    if let Some(session) = self.sessions.get(&SessionKey { dst }) {
                        let pkt = rebuild_ip_reply(&session.original_pkt, &icmp_payload, &src_ip);
                        if !pkt.is_empty()
                            && let Err(e) = self.tun_write_tx.send(BytesMut::from(&pkt[..])).await
                        {
                            warn!("[icmp] tun write: {e}");
                            break;
                        }
                    }
                }
                else => break,
            }
        }
    }

    /// Process outbound ICMP packet from TUN: strip IP header, send to SOCKS5 client.
    async fn handle_outbound(&mut self, raw_ip_pkt: &[u8]) -> Result<()> {
        let (dst, icmp_payload, _version) = parse_ip_icmp(raw_ip_pkt)?;
        let key = SessionKey { dst };

        if let Some(session) = self.sessions.get_mut(&key) {
            session.last_active = Instant::now();
            session.original_pkt = raw_ip_pkt.to_vec();
            let len = (icmp_payload.len() as u32).to_be_bytes();
            session.writer.write_all(&len).await?;
            session.writer.write_all(&icmp_payload).await?;
        } else {
            self.create_session(key, &icmp_payload, raw_ip_pkt).await?;
        }
        Ok(())
    }

    async fn create_session(
        &mut self,
        key: SessionKey,
        first_icmp_data: &[u8],
        first_pkt: &[u8],
    ) -> Result<()> {
        let socks_addr = format!("127.0.0.1:{}", self.socks_port);
        let mut control = TcpStream::connect(&socks_addr)
            .await
            .context("connect socks5 for icmp")?;

        // SOCKS5 greeting (no auth)
        control.write_all(&[0x05, 0x01, 0x00]).await?;
        let mut buf = [0u8; 2];
        control.read_exact(&mut buf).await?;

        // SOCKS5 CMD=0xA1
        let mut req = vec![0x05, CMD_ICMP, 0x00];
        match key.dst {
            IpAddr::V4(ip) => {
                req.push(0x01);
                req.extend_from_slice(&ip.octets());
                req.extend_from_slice(&0u16.to_be_bytes());
            }
            IpAddr::V6(ip) => {
                req.push(0x04);
                req.extend_from_slice(&ip.octets());
                req.extend_from_slice(&0u16.to_be_bytes());
            }
        }
        control.write_all(&req).await?;

        let mut resp = [0u8; 10];
        control.read_exact(&mut resp).await?;
        anyhow::ensure!(resp[1] == 0x00, "SOCKS5 CMD=0xA1 failed: {}", resp[1]);

        info!("[icmp] session {:?} via SOCKS5", key);

        // Split: read half → reader task, write half → session
        let (reader, writer) = control.into_split();

        // Send first ICMP payload
        let mut writer = writer;
        let len = (first_icmp_data.len() as u32).to_be_bytes();
        writer.write_all(&len).await?;
        writer.write_all(first_icmp_data).await?;

        // Spawn reader: SOCKS5 TCP → inbound channel
        let inbound_tx = self.inbound_tx.clone();
        let session_dst = key.dst;
        tokio::spawn(async move {
            let mut reader = reader;
            let mut len_buf = [0u8; 4];
            let mut payload_buf = vec![0u8; 65535];
            loop {
                if reader.read_exact(&mut len_buf).await.is_err() {
                    break;
                }
                let len = u32::from_be_bytes(len_buf) as usize;
                if len == 0 || len > 65535 {
                    break;
                }
                if reader.read_exact(&mut payload_buf[..len]).await.is_err() {
                    break;
                }
                // Client sends encode_icmp_payload format: [2B ip_len][ip][icmp_data]
                if let Ok((src_ip, icmp_data)) = decode_icmp_payload(&payload_buf[..len]) {
                    let src: IpAddr = match src_ip.parse() {
                        Ok(ip) => ip,
                        Err(_) => continue,
                    };
                    if inbound_tx.send((session_dst, src, icmp_data.to_vec())).await.is_err() {
                        break;
                    }
                }
            }
        });

        self.sessions.insert(
            key,
            IcmpSession {
                writer,
                last_active: Instant::now(),
                original_pkt: first_pkt.to_vec(),
            },
        );

        Ok(())
    }

    fn reap_idle(&mut self) {
        let now = Instant::now();
        let before = self.sessions.len();
        self.sessions.retain(|key, session| {
            let alive = now.duration_since(session.last_active) < SESSION_TIMEOUT;
            if !alive {
                debug!("[icmp] reaped idle session {key:?}");
            }
            alive
        });
        let removed = before - self.sessions.len();
        if removed > 0 {
            debug!(
                "[icmp] reaped {removed} sessions, {} remaining",
                self.sessions.len()
            );
        }
    }
}

// ── IP parsing ───────────────────────────────────────────────────────────────

fn parse_ip_icmp(pkt: &[u8]) -> Result<(IpAddr, Vec<u8>, u8)> {
    if pkt.is_empty() {
        anyhow::bail!("empty packet");
    }
    let version = (pkt[0] >> 4) & 0x0F;
    match version {
        4 => {
            let ihl = ((pkt[0] & 0x0F) as usize) * 4;
            anyhow::ensure!(pkt.len() >= ihl, "IPv4 packet too short for header");
            anyhow::ensure!(pkt[9] == 1, "not ICMP (proto={})", pkt[9]);

            let dst = IpAddr::from(Ipv4Addr::from([pkt[16], pkt[17], pkt[18], pkt[19]]));
            let icmp = pkt[ihl..].to_vec();
            Ok((dst, icmp, 4))
        }
        6 => {
            anyhow::ensure!(pkt.len() >= 40, "IPv6 packet too short");
            let next_header = pkt[6];
            anyhow::ensure!(next_header == 58, "not ICMPv6 (next_header={})", next_header);

            let mut dst_bytes = [0u8; 16];
            dst_bytes.copy_from_slice(&pkt[24..40]);
            let dst = IpAddr::from(Ipv6Addr::from(dst_bytes));
            let icmp = pkt[40..].to_vec();
            Ok((dst, icmp, 6))
        }
        v => anyhow::bail!("unsupported IP version: {v}"),
    }
}

// ── IP reply reconstruction ──────────────────────────────────────────────────

fn rebuild_ip_reply(original_pkt: &[u8], icmp_payload: &[u8], src_ip: &IpAddr) -> Vec<u8> {
    let version = (original_pkt[0] >> 4) & 0x0F;
    match version {
        4 => rebuild_ipv4(original_pkt, icmp_payload, src_ip),
        6 => rebuild_ipv6(original_pkt, icmp_payload, src_ip),
        _ => {
            warn!("[icmp] unsupported IP version {version} in reply");
            Vec::new()
        }
    }
}

fn rebuild_ipv4(original_pkt: &[u8], icmp_payload: &[u8], src_ip: &IpAddr) -> Vec<u8> {
    let IpAddr::V4(v4_src) = src_ip else {
        return Vec::new();
    };
    let ihl = ((original_pkt[0] & 0x0F) as usize) * 4;
    let mut reply = vec![0u8; ihl + icmp_payload.len()];

    // Copy IP header
    reply[..ihl].copy_from_slice(&original_pkt[..ihl]);

    // src = remote server's IP (the actual echo reply source), dst = original sender
    reply[12..16].copy_from_slice(&v4_src.octets());
    reply[16..20].copy_from_slice(&original_pkt[12..16]);

    // Update total length
    let total_len = (ihl + icmp_payload.len()) as u16;
    reply[2..4].copy_from_slice(&total_len.to_be_bytes());

    // Zero checksum before recomputing
    reply[10..12].copy_from_slice(&[0, 0]);

    // ICMP payload
    reply[ihl..].copy_from_slice(icmp_payload);

    // IP header checksum
    let checksum = ip_checksum(&reply[..ihl]);
    reply[10..12].copy_from_slice(&checksum.to_be_bytes());

    reply
}

fn rebuild_ipv6(original_pkt: &[u8], icmp_payload: &[u8], src_ip: &IpAddr) -> Vec<u8> {
    let IpAddr::V6(v6_src) = src_ip else {
        return Vec::new();
    };
    let mut reply = vec![0u8; 40 + icmp_payload.len()];

    // Copy IPv6 header
    reply[..40].copy_from_slice(&original_pkt[..40]);

    // src = remote server's IP, dst = original sender
    reply[8..24].copy_from_slice(&v6_src.octets());
    reply[24..40].copy_from_slice(&original_pkt[8..24]);

    // Update payload length (bytes 4..6)
    let payload_len = icmp_payload.len() as u16;
    reply[4..6].copy_from_slice(&payload_len.to_be_bytes());

    // Hop limit
    reply[7] = 64;

    // ICMP payload
    reply[40..].copy_from_slice(icmp_payload);

    // ICMPv6 checksum: zero the existing checksum field (bytes 2..4 of ICMP header)
    // before computing, as it may contain a stale value from the original sender.
    reply[42..44].copy_from_slice(&[0, 0]);

    // ICMPv6 checksum (pseudo-header + ICMP payload)
    let checksum = icmpv6_checksum(&reply[..40], &reply[40..]);
    reply[42..44].copy_from_slice(&checksum.to_be_bytes());

    reply
}

// ── Checksums ────────────────────────────────────────────────────────────────

fn ip_checksum(header: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for chunk in header.chunks(2) {
        let word = if chunk.len() == 2 {
            u16::from_be_bytes([chunk[0], chunk[1]]) as u32
        } else {
            (chunk[0] as u32) << 8
        };
        sum += word;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}

fn icmpv6_checksum(ipv6_header: &[u8], icmp_payload: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    // Pseudo-header: src (16B) + dst (16B) + length (4B) + next_header (4B)
    for i in (8..24).step_by(2) {
        sum += u16::from_be_bytes([ipv6_header[i], ipv6_header[i + 1]]) as u32;
    }
    for i in (24..40).step_by(2) {
        sum += u16::from_be_bytes([ipv6_header[i], ipv6_header[i + 1]]) as u32;
    }
    // Payload length (bytes 4..6)
    let payload_len = u16::from_be_bytes([ipv6_header[4], ipv6_header[5]]) as u32;
    sum += payload_len;
    // Next header (byte 6)
    sum += ipv6_header[6] as u32;

    // ICMPv6 payload
    for chunk in icmp_payload.chunks(2) {
        let word = if chunk.len() == 2 {
            u16::from_be_bytes([chunk[0], chunk[1]]) as u32
        } else {
            (chunk[0] as u32) << 8
        };
        sum += word;
    }

    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}
