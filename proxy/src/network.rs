// Physical network route detection.

use crate::config::Config;
use anyhow::{Context, Result};
use std::net::IpAddr;

pub fn detect_physical_route(cfg: &Config) -> Result<IpAddr> {
    if let Some(ip) = &cfg.phys_ip {
        return ip.parse().context("parse phys_ip");
    }

    detect_auto()
}

#[cfg(target_os = "linux")]
fn detect_auto() -> Result<IpAddr> {
    let text = std::process::Command::new("ip")
        .args(["route", "get", "1.1.1.1"])
        .output()
        .context("ip route get 1.1.1.1")?;

    if !text.status.success() {
        anyhow::bail!("ip route get failed");
    }

    let output = String::from_utf8_lossy(&text.stdout);
    let line = output.lines().next().unwrap_or_default();

    let mut src_ip = String::new();
    let parts: Vec<&str> = line.split_whitespace().collect();
    for i in 0..parts.len() {
        if parts[i] == "src" && i + 1 < parts.len() {
            src_ip = parts[i + 1].to_string();
            break;
        }
    }

    if src_ip.is_empty() {
        anyhow::bail!("failed to parse ip route output: {line}");
    }

    src_ip.parse().context("parse src IP")
}

#[cfg(target_os = "macos")]
fn detect_auto() -> Result<IpAddr> {
    fn get_iface_ipv4(iface: &str) -> Result<IpAddr> {
        let out = std::process::Command::new("ipconfig")
            .args(["getifaddr", iface])
            .output()
            .context("ipconfig getifaddr")?;
        let ip_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        ip_str.parse::<IpAddr>().context("parse IP from ipconfig")
    }

    let out = std::process::Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .context("run route -n get default")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut iface = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("interface:") {
            iface = rest.trim().to_string();
        }
    }
    if iface.is_empty() {
        anyhow::bail!("could not detect interface from `route -n get default`");
    }
    if iface.starts_with("utun") {
        anyhow::bail!("detected interface {iface} looks like a TUN, use manual config");
    }
    get_iface_ipv4(&iface)
}

#[cfg(windows)]
fn detect_auto() -> Result<IpAddr> {
    fn get_iface_ipv4(iface: &str) -> Result<IpAddr> {
        let script = format!(
            "(Get-NetIPAddress -InterfaceAlias '{}' -AddressFamily IPv4 -ErrorAction SilentlyContinue | Select-Object -First 1).IPAddress",
            iface
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .context("powershell get IP")?;
        let ip_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        ip_str.parse::<IpAddr>().context("parse IP")
    }

    let script = r#"Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Where-Object {
        $type = (Get-NetAdapter -InterfaceIndex $_.InterfaceIndex -ErrorAction SilentlyContinue).InterfaceDescription
        $type -notmatch 'TAP|TUN|Wintun|WireGuard|OpenVPN|VPN|Tunnel'
    } | Sort-Object RouteMetric | Select-Object -First 1 | ForEach-Object { $_.InterfaceAlias }"#;
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .context("powershell")?;
    let iface = String::from_utf8_lossy(&out.stdout).trim().to_string();

    if iface.is_empty() {
        anyhow::bail!("failed to detect physical interface from powershell output");
    }

    get_iface_ipv4(&iface)
}
