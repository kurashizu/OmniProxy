// Forwarder: netstack-smoltcp based TUN-to-SOCKS5 transparent proxy.

mod icmp;
pub mod tun_device;
mod udp;

use anyhow::{Context, Result};
use bytes::BytesMut;
use futures::{SinkExt, StreamExt};
use netstack_smoltcp::{Stack, StackBuilder, TcpListener, UdpSocket};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tracing::{debug, info, warn};
use tun_device::TunDevice;
use tun_rs::async_framed::BytesCodec;

pub use tun_device::{tun_down, tun_up};

pub struct Forwarder {
    stack: Option<Stack>,
    tcp_listener: Option<TcpListener>,
    udp_socket: Option<UdpSocket>,
    tun_framed:
        Option<tun_rs::async_framed::DeviceFramed<BytesCodec, std::sync::Arc<tun_rs::AsyncDevice>>>,
    socks_port: u16,
}

impl Forwarder {
    pub fn new(mut tun: TunDevice, socks_port: u16) -> Result<Self> {
        let (stack, runner, udp_socket, tcp_listener) = StackBuilder::default()
            .stack_buffer_size(4096)
            .tcp_buffer_size(512 * 1024)
            .udp_buffer_size(4096)
            .mtu(1500)
            .enable_tcp(true)
            .enable_udp(true)
            .enable_icmp(false)
            .build()
            .context("build netstack")?;

        if let Some(runner) = runner {
            tokio::spawn(runner);
        }

        let tcp_listener = tcp_listener.expect("tcp enabled");
        let udp_socket = udp_socket.expect("udp enabled");
        let dev = tun
            .take_device()
            .ok_or_else(|| anyhow::anyhow!("AsyncDevice already taken"))?;
        let dev = std::sync::Arc::new(dev);
        let framed = tun_rs::async_framed::DeviceFramed::new(dev, BytesCodec::new());

        info!("[forwarder] ready, SOCKS5 port {}", socks_port);

        Ok(Self {
            stack: Some(stack),
            tcp_listener: Some(tcp_listener),
            udp_socket: Some(udp_socket),
            tun_framed: Some(framed),
            socks_port,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let stack = self.stack.take().expect("stack already taken");
        let tun_framed = self.tun_framed.take().expect("tun_framed already taken");
        let (mut stack_sink, mut stack_stream) = stack.split();
        let (mut tun_stream, mut tun_sink) = tun_framed.split();

        // Channels for writing packets back to TUN
        // TCP/UDP uses a larger channel (bursty), ICMP uses a small low-latency channel
        let (tun_write_tx, mut tun_write_rx) = mpsc::channel::<BytesMut>(8192);
        let (icmp_tun_tx, mut icmp_tun_rx) = mpsc::channel::<BytesMut>(256);

        // Shutdown broadcast: signals service tasks to exit when infra dies
        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

        // TUN writer task: merges TCP/UDP and ICMP reply streams
        let tun_writer = tokio::spawn(async move {
            loop {
                tokio::select! {
                    pkt = tun_write_rx.recv() => {
                        let Some(pkt) = pkt else { break };
                        if tun_sink.send(pkt).await.is_err() {
                            warn!("[forwarder] tun write failed");
                            break;
                        }
                    }
                    pkt = icmp_tun_rx.recv() => {
                        let Some(pkt) = pkt else { break };
                        if tun_sink.send(pkt).await.is_err() {
                            warn!("[forwarder] tun write failed");
                            break;
                        }
                    }
                }
            }
        });

        // ICMP handler (intercepts ICMP from TUN before netstack)
        let (icmp_handler, icmp_outbound_tx) = icmp::IcmpHandler::new(self.socks_port, icmp_tun_tx);

        // TUN -> demux (ICMP vs TCP/UDP)
        let tun_to_stack = tokio::spawn(async move {
            while let Some(pkt) = tun_stream.next().await {
                match pkt {
                    Ok(p) => {
                        // ICMP demux: check IP protocol before sending to netstack
                        let is_icmp = !p.is_empty()
                            && match (p[0] >> 4) & 0x0F {
                                4 if p.len() > 9 => p[9] == 1,
                                6 if p.len() > 6 => p[6] == 58,
                                _ => false,
                            };

                        if is_icmp {
                            if icmp_outbound_tx.send(p.to_vec()).await.is_err() {
                                warn!("[forwarder] icmp channel closed");
                                break;
                            }
                        } else {
                            if stack_sink.send(p.to_vec()).await.is_err() {
                                warn!("[forwarder] stack sink closed");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("[forwarder] tun read: {e}");
                        break;
                    }
                }
            }
        });

        // ICMP handler task: non-critical, exit does not trigger full restart
        let mut icmp_shutdown = shutdown_tx.subscribe();
        tokio::spawn(async move {
            tokio::select! {
                _ = icmp_handler.run() => {
                    warn!("[forwarder] icmp handler exited (non-critical)");
                }
                _ = icmp_shutdown.recv() => {}
            }
        });

        // stack -> TUN
        let stack_to_tun = tokio::spawn(async move {
            while let Some(pkt) = stack_stream.next().await {
                match pkt {
                    Ok(p) => {
                        if tun_write_tx.send(BytesMut::from(&p[..])).await.is_err() {
                            warn!("[forwarder] tun write channel closed");
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("[forwarder] stack read: {e}");
                        break;
                    }
                }
            }
        });

        // TCP accept task: non-critical, exit does not trigger full restart
        let socks_port = self.socks_port;
        let mut tcp_listener = self
            .tcp_listener
            .take()
            .expect("tcp_listener already taken");
        let mut tcp_shutdown = shutdown_tx.subscribe();
        tokio::spawn(async move {
            tokio::select! {
                _ = async {
                    while let Some((stream, _src, dst)) = tcp_listener.next().await {
                        let sp = socks_port;
                        tokio::spawn(async move {
                            if let Err(e) = handle_tcp_via_socks5(stream, dst, sp).await {
                                debug!("[session] tcp {dst}: {e}");
                            }
                        });
                    }
                    warn!("[forwarder] tcp listener closed");
                } => {}
                _ = tcp_shutdown.recv() => {}
            }
        });

        // UDP task: non-critical, exit does not trigger full restart
        let udp_socket = self.udp_socket.take().expect("udp_socket already taken");
        let (udp_read, udp_write) = udp_socket.split();
        let socks_port = self.socks_port;
        let mut udp_shutdown = shutdown_tx.subscribe();
        tokio::spawn(async move {
            tokio::select! {
                r = udp::run_udp_handler(udp_read, udp_write, socks_port) => {
                    warn!("[forwarder] udp handler exited: {:?}", r);
                }
                _ = udp_shutdown.recv() => {}
            }
        });

        info!("[forwarder] running");

        // Only infrastructure tasks trigger full restart.
        // Service tasks (tcp/udp/icmp) are fire-and-forget — they die on shutdown.
        tokio::select! {
            _ = tun_to_stack => warn!("[forwarder] tun->stack died"),
            _ = stack_to_tun => warn!("[forwarder] stack->tun died"),
            _ = tun_writer    => warn!("[forwarder] tun writer died"),
        }

        // Signal service tasks to exit
        let _ = shutdown_tx.send(());

        // Give service tasks a moment to drain
        tokio::time::sleep(Duration::from_secs(1)).await;

        Ok(())
    }

    pub fn shutdown(self) {
        info!("[forwarder] shutdown");
    }
}

async fn handle_tcp_via_socks5(
    netstream: netstack_smoltcp::TcpStream,
    dst: std::net::SocketAddr,
    socks_port: u16,
) -> Result<()> {
    let socks_addr = format!("127.0.0.1:{}", socks_port);
    let mut socks = TcpStream::connect(&socks_addr)
        .await
        .context("connect to socks5")?;

    // SOCKS5 greeting (no auth)
    socks.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut buf = [0u8; 2];
    socks.read_exact(&mut buf).await?;

    // SOCKS5 CONNECT
    let mut req = vec![0x05, 0x01, 0x00, 0x01];
    match dst.ip() {
        std::net::IpAddr::V4(ip) => req.extend_from_slice(&ip.octets()),
        std::net::IpAddr::V6(ip) => {
            req[3] = 0x04;
            req.extend_from_slice(&ip.octets());
        }
    }
    req.extend_from_slice(&dst.port().to_be_bytes());
    socks.write_all(&req).await?;

    let mut hdr = [0u8; 4];
    socks.read_exact(&mut hdr).await?;
    anyhow::ensure!(hdr[1] == 0x00, "SOCKS5 CONNECT failed: {}", hdr[1]);
    let skip = match hdr[3] {
        0x01 => 4 + 2,
        0x04 => 16 + 2,
        0x03 => {
            let mut len = [0u8; 1];
            socks.read_exact(&mut len).await?;
            len[0] as usize + 2
        }
        t => anyhow::bail!("unknown ATYP {t:#x}"),
    };
    let mut tail = vec![0u8; skip];
    socks.read_exact(&mut tail).await?;

    info!("[session] SOCKS5 CONNECT to {} ok", dst);

    let (net_r, net_w) = tokio::io::split(netstream);
    let (socks_r, socks_w) = socks.into_split();

    // Wrap in buffered readers/writers with 16KB buffers
    let mut net_r = tokio::io::BufReader::with_capacity(16 * 1024, net_r);
    let mut net_w = tokio::io::BufWriter::with_capacity(16 * 1024, net_w);
    let mut socks_r = tokio::io::BufReader::with_capacity(16 * 1024, socks_r);
    let mut socks_w = tokio::io::BufWriter::with_capacity(16 * 1024, socks_w);

    tokio::select! {
        r = tokio::io::copy(&mut socks_r, &mut net_w) => { r.ok(); }
        r = tokio::io::copy(&mut net_r, &mut socks_w) => { r.ok(); }
    }
    Ok(())
}
