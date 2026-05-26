use crate::Config;
use anyhow::{Context, Result};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::info;

// ── Physical route info ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalRoute {
    pub iface: String,
    pub ip: IpAddr,
    pub gateway: IpAddr,
}

// ── Platform implementations ──────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::fs;
    use tracing::{debug, warn};

    /// Parse /proc/net/route to find default routes, skip `tun_name`.
    pub fn detect_auto(tun_name: &str) -> Result<PhysicalRoute> {
        let text = fs::read_to_string("/proc/net/route").context("read /proc/net/route")?;

        // Columns: Iface Dest Gateway Flags RefCnt Use Metric Mask MTU Window IRTT
        // Default route: Dest=00000000, Mask=00000000, Flags has RTF_UP(0x1)+RTF_GATEWAY(0x2)
        let mut best: Option<(u32, String, u32)> = None; // (metric, iface, gateway_le)

        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 11 {
                continue;
            }
            let iface = cols[0];
            let dest = u32::from_str_radix(cols[1], 16).unwrap_or(1);
            let gw_hex = u32::from_str_radix(cols[2], 16).unwrap_or(0);
            let flags = u32::from_str_radix(cols[3], 16).unwrap_or(0);
            let metric = u32::from_str_radix(cols[6], 10).unwrap_or(9999);
            let mask = u32::from_str_radix(cols[7], 16).unwrap_or(1);

            const RTF_UP: u32 = 0x0001;
            const RTF_GATEWAY: u32 = 0x0002;

            if dest != 0 || mask != 0 {
                continue;
            }
            if flags & (RTF_UP | RTF_GATEWAY) != (RTF_UP | RTF_GATEWAY) {
                continue;
            }
            // Skip TUN and loopback
            if iface == tun_name || iface.starts_with("tun") || iface.starts_with("lo") {
                continue;
            }

            if best.as_ref().map_or(true, |(m, _, _)| metric < *m) {
                best = Some((metric, iface.to_string(), gw_hex));
            }
        }

        let (_, iface, gw_le) =
            best.context("no physical default route found in /proc/net/route")?;
        let gw_bytes = gw_le.to_le_bytes();
        let gateway = IpAddr::V4(std::net::Ipv4Addr::from(gw_bytes));
        let ip = get_iface_ipv4(&iface)?;

        Ok(PhysicalRoute { iface, ip, gateway })
    }

    pub fn detect_for_iface(iface: &str) -> Result<PhysicalRoute> {
        let ip = get_iface_ipv4(iface)?;
        // Read gateway from /proc/net/route for this iface
        let text = std::fs::read_to_string("/proc/net/route")?;
        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 8 {
                continue;
            }
            if cols[0] != iface {
                continue;
            }
            let dest = u32::from_str_radix(cols[1], 16).unwrap_or(1);
            let mask = u32::from_str_radix(cols[7], 16).unwrap_or(1);
            if dest == 0 && mask == 0 {
                let gw_le = u32::from_str_radix(cols[2], 16).unwrap_or(0);
                let gateway = IpAddr::V4(std::net::Ipv4Addr::from(gw_le.to_le_bytes()));
                return Ok(PhysicalRoute {
                    iface: iface.to_string(),
                    ip,
                    gateway,
                });
            }
        }
        anyhow::bail!("no default route found for interface {iface}")
    }

    fn get_iface_ipv4(iface: &str) -> Result<IpAddr> {
        use std::ffi::CString;
        use std::mem;
        unsafe {
            let sock = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
            if sock < 0 {
                anyhow::bail!("socket: {}", std::io::Error::last_os_error());
            }
            let mut ifr: libc::ifreq = mem::zeroed();
            let name_c = CString::new(iface)?;
            let name_bytes = name_c.to_bytes_with_nul();
            let copy_len = name_bytes.len().min(libc::IFNAMSIZ);
            // SAFETY: ifr_name is [c_char; IFNAMSIZ]
            std::ptr::copy_nonoverlapping(
                name_bytes.as_ptr() as *const libc::c_char,
                ifr.ifr_name.as_mut_ptr(),
                copy_len,
            );
            let ret = libc::ioctl(sock, libc::SIOCGIFADDR as libc::Ioctl, &mut ifr as *mut _);
            libc::close(sock);
            if ret < 0 {
                anyhow::bail!(
                    "SIOCGIFADDR for {iface}: {}",
                    std::io::Error::last_os_error()
                );
            }
            let sa = &*((&ifr.ifr_ifru.ifru_addr) as *const _ as *const libc::sockaddr_in);
            let addr = std::net::Ipv4Addr::from(u32::from_be(sa.sin_addr.s_addr));
            Ok(IpAddr::V4(addr))
        }
    }

    /// Watch /proc/net/route for changes via netlink RTMGRP_IPV4_ROUTE.
    pub async fn watch_route_changes(tx: watch::Sender<()>) {
        use std::os::unix::io::FromRawFd;

        unsafe {
            let sock = libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                libc::NETLINK_ROUTE,
            );
            if sock < 0 {
                warn!(
                    "[netwatch] netlink socket failed: {}",
                    std::io::Error::last_os_error()
                );
                return;
            }

            let mut sa: libc::sockaddr_nl = std::mem::zeroed();
            sa.nl_family = libc::AF_NETLINK as u16;
            // RTMGRP_LINK | RTMGRP_IPV4_ROUTE | RTMGRP_IPV4_IFADDR
            sa.nl_groups = 0x1 | 0x4 | 0x10;

            let bind_ret = libc::bind(
                sock,
                &sa as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_nl>() as u32,
            );
            if bind_ret < 0 {
                warn!(
                    "[netwatch] netlink bind failed: {}",
                    std::io::Error::last_os_error()
                );
                libc::close(sock);
                return;
            }

            info!("[netwatch] listening on netlink RTMGRP_LINK|IPV4_ROUTE|IPV4_IFADDR");

            // Wrap in tokio async fd
            let std_sock = std::net::UdpSocket::from_raw_fd(sock); // just to get async
                                                                   // Actually use tokio's UnixDatagram workaround: wrap raw fd
            let async_fd = match tokio::io::unix::AsyncFd::new(
                std::os::unix::io::BorrowedFd::borrow_raw(sock),
            ) {
                Ok(f) => f,
                Err(e) => {
                    warn!("[netwatch] AsyncFd: {e}");
                    libc::close(sock);
                    drop(std_sock);
                    return;
                }
            };
            drop(std_sock); // avoid double-close; AsyncFd borrowed the fd

            let mut buf = [0u8; 4096];
            // Debounce: accumulate events and fire after 500ms quiet period
            let debounce = Duration::from_millis(500);
            loop {
                // Wait for readable
                let mut guard = match async_fd.readable().await {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[netwatch] readable: {e}");
                        break;
                    }
                };
                // Drain the socket
                loop {
                    let n = libc::recv(
                        sock,
                        buf.as_mut_ptr() as *mut _,
                        buf.len(),
                        libc::MSG_DONTWAIT,
                    );
                    if n < 0 {
                        let err = *libc::__errno_location();
                        if err == libc::EAGAIN || err == libc::EWOULDBLOCK {
                            break;
                        }
                        warn!("[netwatch] recv error: {}", std::io::Error::last_os_error());
                        break;
                    }
                }
                guard.retain_ready();

                // Debounce: wait for things to settle before notifying
                debug!(
                    "[netwatch] route/link change detected, debouncing {}ms...",
                    debounce.as_millis()
                );
                tokio::time::sleep(debounce).await;
                info!("[netwatch] network change detected");
                tx.send(()).ok();
            }
            libc::close(sock);
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    pub fn detect_auto(tun_name: &str) -> Result<PhysicalRoute> {
        // Use `netstat -rn` output parsing on macOS
        let out = std::process::Command::new("route")
            .args(["-n", "get", "default"])
            .output()
            .context("run route -n get default")?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut iface = String::new();
        let mut gateway = String::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("interface:") {
                iface = rest.trim().to_string();
            }
            if let Some(rest) = line.strip_prefix("gateway:") {
                gateway = rest.trim().to_string();
            }
        }
        if iface.is_empty() {
            anyhow::bail!("could not detect interface from `route -n get default`");
        }
        if iface == tun_name || iface.starts_with("utun") {
            anyhow::bail!("detected interface {iface} looks like a TUN, set phys_iface explicitly");
        }
        let ip = get_iface_ipv4(&iface)?;
        let gw: IpAddr = gateway.parse().context("parse gateway IP")?;
        Ok(PhysicalRoute {
            iface,
            ip,
            gateway: gw,
        })
    }

    pub fn detect_for_iface(iface: &str) -> Result<PhysicalRoute> {
        let ip = get_iface_ipv4(iface)?;
        let out = std::process::Command::new("route")
            .args(["-n", "get", "default"])
            .output()?;
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("gateway:") {
                let gw: IpAddr = rest.trim().parse().context("parse gw")?;
                return Ok(PhysicalRoute {
                    iface: iface.to_string(),
                    ip,
                    gateway: gw,
                });
            }
        }
        anyhow::bail!("no default gateway found")
    }

    fn get_iface_ipv4(iface: &str) -> Result<IpAddr> {
        let out = std::process::Command::new("ipconfig")
            .args(["getifaddr", iface])
            .output()
            .context("ipconfig getifaddr")?;
        let ip_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        ip_str.parse::<IpAddr>().context("parse IP from ipconfig")
    }

    pub async fn watch_route_changes(tx: watch::Sender<()>) {
        // Poll every 5 seconds on macOS (no easy async netlink equivalent without external crates)
        let mut last: Option<String> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let out = std::process::Command::new("route")
                .args(["-n", "get", "default"])
                .output();
            if let Ok(out) = out {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                if last.as_deref() != Some(&text) {
                    if last.is_some() {
                        info!("[netwatch] network change detected (macOS poll)");
                        tx.send(()).ok();
                    }
                    last = Some(text);
                }
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use tracing::debug;

    pub fn detect_auto(tun_name: &str) -> Result<PhysicalRoute> {
        detect_via_powershell(Some(tun_name))
    }

    pub fn detect_for_iface(iface: &str) -> Result<PhysicalRoute> {
        // Get IP and gateway for specific iface name via netsh/powershell
        detect_via_powershell_iface(iface)
    }

    fn detect_via_powershell(skip_iface: Option<&str>) -> Result<PhysicalRoute> {
        // PowerShell one-liner: find lowest-metric IPv4 default route, skip TUN
        let skip = skip_iface.unwrap_or("__none__");
        let script = format!(
            r#"$r = Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Where-Object {{ $_.InterfaceAlias -ne '{skip}' -and $_.InterfaceAlias -notmatch '^tun' }} | Sort-Object RouteMetric | Select-Object -First 1; $a = (Get-NetIPAddress -InterfaceIndex $r.InterfaceIndex -AddressFamily IPv4 | Select-Object -First 1).IPAddress; "$($r.InterfaceAlias)|$a|$($r.NextHop)""#
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .context("powershell route query")?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let parts: Vec<&str> = text.split('|').collect();
        if parts.len() < 3 {
            anyhow::bail!("unexpected powershell output: {text:?}");
        }
        Ok(PhysicalRoute {
            iface: parts[0].to_string(),
            ip: parts[1].parse().context("parse IP")?,
            gateway: parts[2].parse().context("parse GW")?,
        })
    }

    fn detect_via_powershell_iface(iface: &str) -> Result<PhysicalRoute> {
        let script = format!(
            r#"$a = (Get-NetIPAddress -InterfaceAlias '{iface}' -AddressFamily IPv4 | Select-Object -First 1).IPAddress; $gw = (Get-NetRoute -DestinationPrefix '0.0.0.0/0' -InterfaceAlias '{iface}' | Select-Object -First 1).NextHop; "$a|$gw""#
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let parts: Vec<&str> = text.split('|').collect();
        if parts.len() < 2 {
            anyhow::bail!("powershell output: {text:?}");
        }
        Ok(PhysicalRoute {
            iface: iface.to_string(),
            ip: parts[0].parse().context("parse IP")?,
            gateway: parts[1].parse().context("parse GW")?,
        })
    }

    pub async fn watch_route_changes(tx: watch::Sender<()>) {
        // Poll every 3 seconds on Windows
        // (NotifyIpInterfaceChange requires unsafe FFI; polling is reliable enough)
        let mut last: Option<PhysicalRoute> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            match detect_via_powershell(None) {
                Ok(route) => {
                    if last.as_ref() != Some(&route) {
                        if last.is_some() {
                            info!(
                                "[netwatch] network change detected (Windows poll): {:?}",
                                route
                            );
                            tx.send(()).ok();
                        }
                        last = Some(route);
                    }
                }
                Err(e) => {
                    debug!("[netwatch] poll error: {e:#}");
                }
            }
        }
    }
}

// Re-export platform watcher
use imp::{detect_auto, detect_for_iface, watch_route_changes};

pub fn detect_physical_route(cfg: &Config) -> Result<PhysicalRoute> {
    match cfg.phys_iface.as_deref() {
        Some(iface) if !iface.is_empty() => detect_for_iface(iface),
        _ => detect_auto(&cfg.tun_name),
    }
}

/// Spawn the network-change watcher.
/// Fires `tx` whenever a change is detected.
/// Also detects if the outbound IP drifted (DHCP renewal) and fires in that case too.
pub async fn watch_changes(cfg: Arc<Config>, tx: watch::Sender<()>) {
    // Spawn the low-level platform watcher on a separate task
    let (inner_tx, mut inner_rx) = watch::channel(());
    tokio::spawn(async move {
        watch_route_changes(inner_tx).await;
    });

    // Also poll the physical IP every 10s as a safety net for DHCP changes
    // that don't produce a route event (e.g. same-gateway IP renewal)
    let cfg2 = cfg.clone();
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let mut last_ip: Option<IpAddr> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            if let Ok(phys) = detect_physical_route(&cfg2) {
                if last_ip.map_or(false, |ip| ip != phys.ip) {
                    info!(
                        "[netwatch] IP changed ({:?} → {}), triggering restart",
                        last_ip, phys.ip
                    );
                    tx2.send(()).ok();
                }
                last_ip = Some(phys.ip);
            }
        }
    });

    // Forward inner events to outer tx
    loop {
        if inner_rx.changed().await.is_err() {
            break;
        }
        tx.send(()).ok();
    }
}
