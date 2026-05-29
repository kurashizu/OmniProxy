// TUN interface and routing management.

use crate::config::Config;
use anyhow::Result;
use tracing::{debug, info};

// Public API.
pub async fn tun_up(cfg: &Config) -> Result<()> {
    imp::tun_up(cfg).await
}

pub async fn tun_down(cfg: &Config) {
    imp::tun_down(cfg).await;
}

pub fn tun_exists(tun: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("ip")
            .args(["link", "show", tun])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("ifconfig")
            .args([tun])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(windows)]
    {
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "(Get-NetIPInterface -InterfaceAlias '{}' -AddressFamily IPv4 -ErrorAction SilentlyContinue).InterfaceIndex",
                    tun
                ),
            ])
            .output()
            .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
            .unwrap_or(false)
    }
}

// Linux implementation.
#[cfg(target_os = "linux")]
mod imp {
    use super::*;

    pub async fn tun_up(cfg: &Config) -> Result<()> {
        let tun = &cfg.tun_name;

        // Bring up TUN device
        let out = tokio::process::Command::new("ip")
            .args(["link", "set", "dev", tun, "up"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("ip link set: {e}"))?;
        if !out.status.success() {
            anyhow::bail!("ip link set failed: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // Assign IPv4 address to TUN
        let out = tokio::process::Command::new("ip")
            .args(["addr", "add", &format!("{}/{}", cfg.tun_ip, cfg.tun_prefix), "dev", tun])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("ip addr add: {e}"))?;
        if !out.status.success() {
            debug!("ip addr add: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // Assign IPv6 address to TUN
        let out = tokio::process::Command::new("ip")
            .args(["-6", "addr", "add", &format!("{}/{}", cfg.tun_ip6, cfg.tun_prefix6), "dev", tun])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("ip -6 addr add: {e}"))?;
        if !out.status.success() {
            debug!("ip -6 addr add: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // Default IPv4 route via TUN (metric 1 to prioritize over physical default)
        let out = tokio::process::Command::new("ip")
            .args(["route", "add", "default", "dev", tun, "metric", "1"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("ip route add: {e}"))?;
        if !out.status.success() {
            debug!("ip route add: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // Default IPv6 route via TUN
        let out = tokio::process::Command::new("ip")
            .args(["-6", "route", "add", "default", "dev", tun, "metric", "1"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("ip -6 route add: {e}"))?;
        if !out.status.success() {
            debug!("ip -6 route add: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        info!("[tun] TUN routes configured (Linux)");
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        // Remove default IPv4 route
        let _ = tokio::process::Command::new("ip")
            .args(["route", "del", "default", "dev", &cfg.tun_name])
            .output()
            .await;
        // Remove default IPv6 route
        let _ = tokio::process::Command::new("ip")
            .args(["-6", "route", "del", "default", "dev", &cfg.tun_name])
            .output()
            .await;
        // Bring down TUN device
        let _ = tokio::process::Command::new("ip")
            .args(["link", "set", "dev", &cfg.tun_name, "down"])
            .output()
            .await;
        info!("[tun] TUN routes removed (Linux)");
    }
}

// macOS implementation.
#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use anyhow::Context;
    use tracing::warn;

    /// Parse `route -n get default` to extract gateway IP and interface name.
    fn detect_physical_gateway() -> Result<(std::net::IpAddr, String)> {
        let out = std::process::Command::new("route")
            .args(["-n", "get", "default"])
            .output()
            .context("route -n get default")?;
        let text = String::from_utf8_lossy(&out.stdout);

        let mut gateway = None;
        let mut iface = None;

        for line in text.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("gateway:") {
                let val = val.trim().to_string();
                if !val.is_empty() {
                    gateway = Some(val);
                }
            }
            if let Some(val) = line.strip_prefix("interface:") {
                let val = val.trim().to_string();
                if !val.is_empty() {
                    iface = Some(val);
                }
            }
        }

        let gw: std::net::IpAddr = gateway
            .context("could not detect gateway from `route -n get default`")?
            .parse()
            .context("parse gateway IP")?;
        let iface = iface.context("could not detect interface from `route -n get default`")?;

        Ok((gw, iface))
    }

    pub async fn tun_up(cfg: &Config) -> Result<()> {
        info!("[tun] Configuring TUN routes (macOS)...");

        // Bring up TUN interface with IPv4 address (macOS requires local=destination for utun)
        let out = tokio::process::Command::new("ifconfig")
            .args([&cfg.tun_name, &cfg.tun_ip, &cfg.tun_ip, "up"])
            .output()
            .await?;
        if !out.status.success() {
            debug!("ifconfig: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // Assign IPv6 address to TUN
        let out = tokio::process::Command::new("ifconfig")
            .args([&cfg.tun_name, "inet6", &format!("{}/{}", cfg.tun_ip6, cfg.tun_prefix6), "up"])
            .output()
            .await?;
        if !out.status.success() {
            debug!("ifconfig inet6: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // Scoped route on physical interface so macOS validates IP_BOUND_IF connections
        // against the physical NIC's scope instead of killing them.
        if let Ok((gw, phys_iface)) = detect_physical_gateway() {
            info!(gateway = %gw, iface = %phys_iface, "[tun] adding scoped route for IP_BOUND_IF");
            let out = tokio::process::Command::new("route")
                .args(["-n", "add", "-net", "0.0.0.0/0", &gw.to_string(), "-ifscope", &phys_iface])
                .output()
                .await;
            match out {
                Ok(o) if !o.status.success() => {
                    debug!("route add -ifscope: {}", String::from_utf8_lossy(&o.stderr).trim());
                }
                Err(e) => debug!("route add -ifscope failed: {e}"),
                _ => {}
            }
        } else {
            warn!("[tun] could not detect physical gateway, skipping scoped route (IP_BOUND_IF may fail)");
        }

        // Split tunneling: /1 routes cover entire IPv4 space but don't conflict with default gateway
        // 0.0.0.0/1 matches 0.0.0.0 - 127.255.255.255
        // 128.0.0.0/1 matches 128.0.0.0 - 255.255.255.255
        let out = tokio::process::Command::new("route")
            .args(["-n", "add", "-net", "0.0.0.0/1", &cfg.tun_ip])
            .output()
            .await?;
        if !out.status.success() {
            debug!("route add -net 0.0.0.0/1: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        let out = tokio::process::Command::new("route")
            .args(["-n", "add", "-net", "128.0.0.0/1", &cfg.tun_ip])
            .output()
            .await?;
        if !out.status.success() {
            debug!("route add -net 128.0.0.0/1: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // Split tunneling for IPv6 (::/1 + 8000::/1)
        let out = tokio::process::Command::new("route")
            .args(["-n", "add", "-inet6", "-net", "::/1", &cfg.tun_ip6])
            .output()
            .await?;
        if !out.status.success() {
            debug!("route add -inet6 ::/1: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        let out = tokio::process::Command::new("route")
            .args(["-n", "add", "-inet6", "-net", "8000::/1", &cfg.tun_ip6])
            .output()
            .await?;
        if !out.status.success() {
            debug!("route add -inet6 8000::/1: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        info!("[tun] TUN routes configured (macOS)");
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        // Remove scoped route on physical interface (best-effort, ignore errors)
        if let Ok((gw, phys_iface)) = detect_physical_gateway() {
            let _ = tokio::process::Command::new("route")
                .args(["-n", "delete", "-net", "0.0.0.0/0", &gw.to_string(), "-ifscope", &phys_iface])
                .output()
                .await;
        }

        // Remove IPv4 split tunnel routes
        let _ = tokio::process::Command::new("route")
            .args(["-n", "delete", "-net", "0.0.0.0/1"])
            .output()
            .await;
        let _ = tokio::process::Command::new("route")
            .args(["-n", "delete", "-net", "128.0.0.0/1"])
            .output()
            .await;

        // Remove IPv6 split tunnel routes
        let _ = tokio::process::Command::new("route")
            .args(["-n", "delete", "-inet6", "-net", "::/1"])
            .output()
            .await;
        let _ = tokio::process::Command::new("route")
            .args(["-n", "delete", "-inet6", "-net", "8000::/1"])
            .output()
            .await;

        // Bring down TUN device
        let _ = tokio::process::Command::new("ifconfig")
            .args([&cfg.tun_name, "down"])
            .output()
            .await;

        info!("[tun] TUN routes removed (macOS)");
    }
}

// Windows implementation.
#[cfg(windows)]
mod imp {
    use super::*;

    pub async fn tun_up(cfg: &Config) -> Result<()> {
        info!("[tun] Configuring TUN routes (Windows)...");

        // Get InterfaceIndex from interface alias name
        let out = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "(Get-NetIPInterface -InterfaceAlias '{}' -AddressFamily IPv4 -ErrorAction SilentlyContinue).InterfaceIndex",
                    cfg.tun_name
                ),
            ])
            .output()
            .await?;
        let idx = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if idx.is_empty() {
            anyhow::bail!("could not find InterfaceIndex for '{}'", cfg.tun_name);
        }
        info!("[tun] found InterfaceIndex: {}", idx);

        // Disable automatic metric and set manual metric to 1 (highest priority)
        let out = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Set-NetIPInterface -InterfaceIndex {idx} -AutomaticMetric Disabled -InterfaceMetric 1"),
            ])
            .output()
            .await?;
        if !out.status.success() {
            debug!("Set-NetIPInterface: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // Assign IPv4 address and default gateway to TUN interface
        let out = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "New-NetIPAddress -InterfaceIndex {idx} -IPAddress '{}' -PrefixLength {} -DefaultGateway '{}' -ErrorAction SilentlyContinue",
                    cfg.tun_ip, cfg.tun_prefix, cfg.tun_gw
                ),
            ])
            .output()
            .await?;
        if !out.status.success() {
            debug!("New-NetIPAddress: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // Default IPv4 route via TUN
        let out = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Remove-NetRoute -InterfaceIndex {idx} -DestinationPrefix '0.0.0.0/0' -Confirm:$false -ErrorAction SilentlyContinue; \
                     New-NetRoute -InterfaceIndex {idx} -DestinationPrefix '0.0.0.0/0' -NextHop '{}' -RouteMetric 1",
                    cfg.tun_gw
                ),
            ])
            .output()
            .await?;
        if !out.status.success() {
            debug!("default v4 route: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        // Default IPv6 route via TUN (link-local gateway fe80::1)
        let out = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Remove-NetRoute -InterfaceIndex {idx} -DestinationPrefix '::/0' -Confirm:$false -ErrorAction SilentlyContinue; \
                     New-NetRoute -InterfaceIndex {idx} -DestinationPrefix '::/0' -NextHop 'fe80::1' -RouteMetric 1"
                ),
            ])
            .output()
            .await?;
        if !out.status.success() {
            debug!("default v6 route: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        info!("[tun] TUN routes configured (Windows, idx={})", idx);
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        // Remove default IPv4 route on TUN
        let _ = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Remove-NetRoute -InterfaceAlias '{}' -DestinationPrefix '0.0.0.0/0' -Confirm:$false -ErrorAction SilentlyContinue",
                    cfg.tun_name
                ),
            ])
            .output()
            .await;

        // Remove default IPv6 route on TUN
        let _ = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Remove-NetRoute -InterfaceAlias '{}' -DestinationPrefix '::/0' -Confirm:$false -ErrorAction SilentlyContinue",
                    cfg.tun_name
                ),
            ])
            .output()
            .await;

        info!("[tun] TUN routes removed (Windows)");
    }
}
