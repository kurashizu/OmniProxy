use crate::{network::PhysicalRoute, Config};
use anyhow::Result;
use std::net::IpAddr;
use tracing::{debug, info, warn};

// ── Public API ────────────────────────────────────────────────────────────────

/// Bring up TUN interface, assign addresses, configure routes.
pub async fn tun_up(cfg: &Config, phys: &PhysicalRoute) -> Result<()> {
    imp::tun_up(cfg, phys).await
}

/// Tear down TUN routes and interface (best-effort; errors are logged, not returned).
pub async fn tun_down(cfg: &Config) {
    imp::tun_down(cfg).await;
}

// ── Linux ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod imp {
    use super::*;

    async fn ip(args: &[&str]) -> Result<()> {
        let out = tokio::process::Command::new("ip")
            .args(args)
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("ip {}: {e}", args.join(" ")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            debug!("ip {} → {}: {}", args.join(" "), out.status, stderr.trim());
        }
        Ok(())
    }

    async fn ip_strict(args: &[&str]) -> Result<()> {
        let out = tokio::process::Command::new("ip")
            .args(args)
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("ip strict {}: {e}", args.join(" ")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("ip strict failed: {}", stderr.trim());
        }
        Ok(())
    }

    pub async fn tun_up(cfg: &Config, phys: &PhysicalRoute) -> Result<()> {
        let tun = &cfg.tun_name;
        let tun_ip = &cfg.tun_ip;
        let tun_ip6 = &cfg.tun_ip6;
        let prefix = cfg.tun_prefix;
        let prefix6 = cfg.tun_prefix6;

        ip_strict(&["link", "set", "dev", tun, "mtu", "1500", "up"]).await?;
        ip(&["addr", "add", &format!("{tun_ip}/{prefix}"), "dev", tun]).await?;

        // Bypass route for server
        if let Some(ref sip) = cfg.server_ip_hint_v4().await {
            let gw = phys.gateway.to_string();
            ip(&["route", "add", sip, "via", &gw, "dev", &phys.iface]).await?;
        }

        // IPv6 TUN addr
        ip(&[
            "-6",
            "addr",
            "add",
            &format!("{tun_ip6}/{prefix6}"),
            "dev",
            tun,
        ])
        .await?;

        // Default routes via TUN device (no gateway needed)
        ip(&["route", "add", "default", "dev", tun, "metric", "1"]).await?;
        ip(&["-6", "route", "add", "default", "dev", tun, "metric", "1"]).await?;

        // Bypass route for server IPv6
        if let Some((gw6, iface6)) = ipv6_default_gateway().await {
            if let Some(server_ip6) = cfg.server_ip_hint_v6().await {
                ip(&[
                    "-6",
                    "route",
                    "add",
                    &format!("{server_ip6}/128"),
                    "via",
                    &gw6,
                    "dev",
                    &iface6,
                ])
                .await?;
            }
        }

        info!("[route] TUN routes configured (Linux)");
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        if let Some(ref sip) = cfg.server_ip_hint_v4().await {
            ip(&["route", "del", sip]).await.ok();
        }
        if let Some(server_ip6) = cfg.server_ip_hint_v6().await {
            ip(&["-6", "route", "del", &format!("{server_ip6}/128")])
                .await
                .ok();
        }
        ip(&["link", "del", "dev", &cfg.tun_name]).await.ok();
        info!("[route] TUN routes removed (Linux)");
    }

    async fn ipv6_default_gateway() -> Option<(String, String)> {
        let out = tokio::process::Command::new("ip")
            .args(["-6", "route", "show", "default"])
            .output()
            .await
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let mut gw = None;
            let mut dev = None;
            let parts: Vec<&str> = line.split_whitespace().collect();
            let mut i = 0;
            while i < parts.len() {
                match parts[i] {
                    "via" if i + 1 < parts.len() => gw = Some(parts[i + 1].to_string()),
                    "dev" if i + 1 < parts.len() => dev = Some(parts[i + 1].to_string()),
                    _ => {}
                }
                i += 1;
            }
            if let (Some(gw), Some(dev)) = (gw, dev) {
                return Some((gw, dev));
            }
        }
        None
    }
}

// ── macOS ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    async fn route(args: &[&str]) {
        let out = tokio::process::Command::new("route")
            .args(args)
            .output()
            .await;
        if let Ok(out) = out {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                debug!("route {} → {}", args.join(" "), stderr.trim());
            }
        }
    }

    // 针对 macOS 的网络接口与标准路由控制实现
    pub async fn tun_up(cfg: &Config, _phys: &PhysicalRoute) -> Result<()> {
        let tun = &cfg.tun_name;
        let tun_ip = &cfg.tun_ip;
        let tun_ip6 = &cfg.tun_ip6;
        let prefix6 = cfg.tun_prefix6;

        info!("[route] Configuring TUN routes (macOS)...");

        // macOS requires local and destination to be the same for utun
        let out = tokio::process::Command::new("ifconfig")
            .args([tun, tun_ip, tun_ip, "up"])
            .output()
            .await?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            debug!("ifconfig failed: {}", stderr.trim());
        }

        // IPv6 address on TUN
        tokio::process::Command::new("ifconfig")
            .args([tun, "inet6", &format!("{tun_ip6}/{prefix6}"), "up"])
            .output()
            .await
            .ok();

        // Bypass route for server (use interface to support both IPv4/IPv6)
        if let Some(ref sip) = cfg.server_ip_hint_v4().await {
            route(&["-n", "add", "-host", sip, "-interface", &_phys.iface]).await;
        }
        if let Some(ref sip6) = cfg.server_ip_hint_v6().await {
            route(&["-n", "add", "-inet6", "-host", sip6, "-interface", &_phys.iface]).await;
        }

        // Split tunneling default routes (IPv4)
        route(&["-n", "add", "-net", "0.0.0.0/1", tun_ip]).await;
        route(&["-n", "add", "-net", "128.0.0.0/1", tun_ip]).await;

        // Split tunneling default routes (IPv6) - must use interface, not IPv4 address
        route(&["-n", "add", "-inet6", "-net", "::/1", "-interface", tun]).await;
        route(&["-n", "add", "-inet6", "-net", "8000::/1", "-interface", tun]).await;

        info!("[route] TUN routes configured (macOS)");
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        // Clean up split tunneling routes
        route(&["-n", "delete", "-net", "0.0.0.0/1"]).await;
        route(&["-n", "delete", "-net", "128.0.0.0/1"]).await;
        route(&["-n", "delete", "-inet6", "-net", "::/1"]).await;
        route(&["-n", "delete", "-inet6", "-net", "8000::/1"]).await;
        info!("[route] TUN routes removed (macOS)");
    }
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(windows)]
mod imp {
    use super::*;
    use std::time::Duration;

    async fn ps(script: &str) -> Result<()> {
        let out = tokio::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .output()
            .await?;
        if !out.status.success() {
            let e = String::from_utf8_lossy(&out.stderr);
            debug!("ps: {e}");
        }
        Ok(())
    }

    pub async fn tun_up(cfg: &Config, phys: &PhysicalRoute) -> Result<()> {
        let tun = &cfg.tun_name;
        let tun_ip = &cfg.tun_ip;
        let tun_gw = &cfg.tun_gw;
        let prefix = cfg.tun_prefix;

        let find_idx_script = format!(
            r#"(Get-NetAdapter | Where-Object {{ $_.InterfaceAlias -eq '{tun}' -or $_.InterfaceDescription -like '*Wintun*' -or $_.InterfaceDescription -like '*WireGuard*' }} | Select-Object -First 1).InterfaceIndex"#
        );

        let mut idx = String::new();
        info!("[route] Waiting for Wintun adapter to be created by tun2socks...");

        for attempt in 1..=20 {
            let out = tokio::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", &find_idx_script])
                .output()
                .await?;

            let res = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !res.is_empty() {
                idx = res;
                break;
            }

            debug!(
                "[route] Adapter not found yet (attempt {}/20), retrying...",
                attempt
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        if idx.is_empty() {
            anyhow::bail!(
                "could not find TUN adapter '{tun}' (timeout 10s) — is wintun.dll present and tun2socks running?"
            );
        }

        info!("[route] Found TUN adapter with InterfaceIndex: {}", idx);

        ps(&format!(
            "Set-NetIPInterface -InterfaceIndex {idx} -AutomaticMetric Disabled -InterfaceMetric 1"
        ))
        .await?;

        ps(&format!(
            "New-NetIPAddress -InterfaceIndex {idx} -IPAddress '{tun_ip}' -PrefixLength {prefix} -DefaultGateway '{tun_gw}' -ErrorAction SilentlyContinue"
        )).await?;

        if let Some(ref sip) = cfg.server_ip_hint_v4().await {
            let gw = phys.gateway.to_string();
            let dev = &phys.iface;
            ps(&format!(
                "Remove-NetRoute -DestinationPrefix '{sip}/32' -Confirm:$false -ErrorAction SilentlyContinue; \
                 New-NetRoute -InterfaceAlias '{dev}' -DestinationPrefix '{sip}/32' -NextHop '{gw}' -RouteMetric 1"
            )).await?;
        }

        ps(&format!(
            "Remove-NetRoute -InterfaceIndex {idx} -DestinationPrefix '0.0.0.0/0' -Confirm:$false -ErrorAction SilentlyContinue; \
             New-NetRoute -InterfaceIndex {idx} -DestinationPrefix '0.0.0.0/0' -NextHop '{tun_gw}' -RouteMetric 1"
        )).await?;

        ps(&format!(
            "Remove-NetRoute -InterfaceIndex {idx} -DestinationPrefix '::/0' -Confirm:$false -ErrorAction SilentlyContinue; \
             New-NetRoute -InterfaceIndex {idx} -DestinationPrefix '::/0' -NextHop 'fe80::1' -RouteMetric 1"
        )).await?;

        info!("[route] TUN routes configured (Windows, idx={idx})");
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        let tun = &cfg.tun_name;
        if let Some(ref sip) = cfg.server_ip_hint_v4().await {
            tokio::process::Command::new("powershell")
                .args(["-NoProfile", "-Command",
                    &format!("Remove-NetRoute -DestinationPrefix '{sip}/32' -Confirm:$false -ErrorAction SilentlyContinue")])
                .output().await.ok();
        }
        tokio::process::Command::new("powershell")
            .args(["-NoProfile", "-Command",
                &format!("Remove-NetRoute -InterfaceAlias '{tun}' -DestinationPrefix '0.0.0.0/0' -Confirm:$false -ErrorAction SilentlyContinue")])
            .output().await.ok();
        tokio::process::Command::new("powershell")
            .args(["-NoProfile", "-Command",
                &format!("Remove-NetRoute -InterfaceAlias '{tun}' -DestinationPrefix '::/0' -Confirm:$false -ErrorAction SilentlyContinue")])
            .output().await.ok();
        info!("[route] TUN routes removed (Windows)");
    }
}

// ── Config helper: 全局公用方法 ───────────────────────────────────────────────

impl Config {
    /// Attempt to resolve the server hostname to an IPv4 for the bypass route.
    pub async fn server_ip_hint_v4(&self) -> Option<String> {
        let host = extract_host(&self.server)?;
        if host.parse::<std::net::Ipv4Addr>().is_ok() {
            return Some(host);
        }
        match tokio::net::lookup_host(format!("{host}:443")).await {
            Ok(mut addrs) => {
                let ip = addrs
                    .find(|a| matches!(a.ip(), IpAddr::V4(_)))?
                    .ip()
                    .to_string();
                info!("[route] server {host} resolved to {ip} (v4 bypass route)");
                Some(ip)
            }
            Err(e) => {
                warn!("[route] server DNS resolve failed: {e:#}");
                None
            }
        }
    }

    /// Attempt to resolve the server hostname to an IPv6 for the bypass route.
    pub async fn server_ip_hint_v6(&self) -> Option<String> {
        let host = extract_host(&self.server)?;
        if host.parse::<std::net::Ipv6Addr>().is_ok() {
            return Some(host);
        }
        match tokio::net::lookup_host(format!("{host}:443")).await {
            Ok(mut addrs) => {
                let ip = addrs
                    .find(|a| matches!(a.ip(), IpAddr::V6(_)))?
                    .ip()
                    .to_string();
                info!("[route] server {host} resolved to {ip} (v6 bypass route)");
                Some(ip)
            }
            Err(e) => {
                warn!("[route] server DNS resolve failed: {e:#}");
                None
            }
        }
    }
}

fn extract_host(server: &str) -> Option<String> {
    if server.contains("://") {
        let url = url::Url::parse(server).ok()?;
        Some(url.host_str()?.to_string())
    } else {
        let host_port: Vec<&str> = server.split(':').collect();
        Some(host_port[0].to_string())
    }
}
