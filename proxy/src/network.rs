use crate::config::Config;
use anyhow::{Context, Result};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, info};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalRoute {
    pub iface: String,
    pub ip: IpAddr,
    pub gateway: IpAddr,
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::fs;

    pub fn detect_auto(tun_name: &str) -> Result<PhysicalRoute> {
        let text = fs::read_to_string("/proc/net/route").context("read /proc/net/route")?;

        let mut best: Option<(u32, String, u32)> = None;

        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 11 {
                continue;
            }
            let iface = cols[0];
            let dest = u32::from_str_radix(cols[1], 16).context("parse dest")?;
            let gw_hex = u32::from_str_radix(cols[2], 16).context("parse gateway")?;
            let flags = u32::from_str_radix(cols[3], 16).context("parse flags")?;
            let metric = cols[6].parse::<u32>().context("parse metric")?;
            let mask = u32::from_str_radix(cols[7], 16).context("parse mask")?;

            const RTF_UP: u32 = 0x0001;
            const RTF_GATEWAY: u32 = 0x0002;

            if dest != 0 || mask != 0 {
                continue;
            }
            if flags & (RTF_UP | RTF_GATEWAY) != (RTF_UP | RTF_GATEWAY) {
                continue;
            }
            if iface == tun_name || iface.starts_with("tun") || iface.starts_with("lo") {
                continue;
            }

            if best.as_ref().is_none_or(|(m, _, _)| metric < *m) {
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
        let text = std::fs::read_to_string("/proc/net/route")?;
        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 8 {
                continue;
            }
            if cols[0] != iface {
                continue;
            }
            let dest = u32::from_str_radix(cols[1], 16).context("parse dest")?;
            let mask = u32::from_str_radix(cols[7], 16).context("parse mask")?;
            if dest == 0 && mask == 0 {
                let gw_le = u32::from_str_radix(cols[2], 16).context("parse gateway")?;
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

    pub async fn watch_route_changes(tx: watch::Sender<()>) {
        unsafe {
            let sock = libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                libc::NETLINK_ROUTE,
            );
            if sock < 0 {
                debug!(
                    "[netwatch] netlink socket failed: {}",
                    std::io::Error::last_os_error()
                );
                return;
            }

            let mut sa: libc::sockaddr_nl = std::mem::zeroed();
            sa.nl_family = libc::AF_NETLINK as u16;
            sa.nl_groups = 0x1 | 0x4 | 0x10;

            let bind_ret = libc::bind(
                sock,
                &sa as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_nl>() as u32,
            );
            if bind_ret < 0 {
                debug!(
                    "[netwatch] netlink bind failed: {}",
                    std::io::Error::last_os_error()
                );
                libc::close(sock);
                return;
            }

            info!("[netwatch] listening on netlink RTMGRP_LINK|IPV4_ROUTE|IPV4_IFADDR");

            let async_fd = match tokio::io::unix::AsyncFd::new(
                std::os::unix::io::BorrowedFd::borrow_raw(sock),
            ) {
                Ok(f) => f,
                Err(e) => {
                    debug!("[netwatch] AsyncFd: {e}");
                    libc::close(sock);
                    return;
                }
            };

            let mut buf = [0u8; 4096];
            let debounce = Duration::from_millis(500);
            loop {
                let mut guard = match async_fd.readable().await {
                    Ok(g) => g,
                    Err(e) => {
                        debug!("[netwatch] readable: {e}");
                        break;
                    }
                };
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
                        debug!("[netwatch] recv error: {}", std::io::Error::last_os_error());
                        break;
                    }
                }
                guard.retain_ready();

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

    pub async fn detect_auto(tun_name: &str) -> Result<PhysicalRoute> {
        let out = tokio::process::Command::new("route")
            .args(["-n", "get", "default"])
            .output()
            .await
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

    pub async fn detect_for_iface(iface: &str) -> Result<PhysicalRoute> {
        let ip = get_iface_ipv4(iface)?;
        let out = tokio::process::Command::new("route")
            .args(["-n", "get", "default"])
            .output()
            .await
            .context("run route -n get default")?;
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
        let mut last: Option<String> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let out = tokio::process::Command::new("route")
                .args(["-n", "get", "default"])
                .output()
                .await;
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

    pub fn detect_auto(tun_name: &str) -> Result<PhysicalRoute> {
        detect_via_powershell(Some(tun_name))
    }

    pub fn detect_for_iface(iface: &str) -> Result<PhysicalRoute> {
        detect_via_powershell_iface(iface)
    }

    fn detect_via_powershell(skip_iface: Option<&str>) -> Result<PhysicalRoute> {
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
            .output()
            .context("powershell")?;
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
        let mut last: Option<PhysicalRoute> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            match detect_via_powershell(None) {
                Ok(route) => {
                    if last.as_ref() != Some(&route) {
                        if last.is_some() {
                            info!(
                                "[netwatch] network change detected (Windows poll): {route:?}"
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

use imp::{detect_auto, detect_for_iface, watch_route_changes};

pub fn detect_physical_route(cfg: &Config) -> Result<PhysicalRoute> {
    match cfg.phys_iface.as_deref() {
        Some(iface) if !iface.is_empty() => detect_for_iface(iface),
        _ => detect_auto(&cfg.tun_name),
    }
}

pub async fn watch_changes(cfg: Arc<Config>, tx: watch::Sender<()>) {
    let (inner_tx, mut inner_rx) = watch::channel(());
    tokio::spawn(async move {
        watch_route_changes(inner_tx).await;
    });

    let cfg2 = cfg.clone();
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let mut last_ip: Option<IpAddr> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            if let Ok(phys) = detect_physical_route(&cfg2) {
                if last_ip.is_some_and(|ip| ip != phys.ip) {
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

    loop {
        if inner_rx.changed().await.is_err() {
            break;
        }
        tx.send(()).ok();
    }
}
