use crate::{network::PhysicalRoute, Config};
use anyhow::Result;
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
        let tun_gw = &cfg.tun_gw;
        let prefix = cfg.tun_prefix;

        ip_strict(&["link", "set", "dev", tun, "mtu", "1500", "up"]).await?;
        ip(&["addr", "add", &format!("{tun_ip}/{prefix}"), "dev", tun]).await?;

        if let Some(ref sip) = cfg.server_ip_hint().await {
            let gw = phys.gateway.to_string();
            ip(&["route", "add", sip, "via", &gw, "dev", &phys.iface]).await?;
        }

        ip(&[
            "route", "add", "default", "via", tun_gw, "dev", tun, "metric", "1",
        ])
        .await?;
        ip(&["route", "add", "default", "dev", tun, "metric", "1"]).await?;

        info!("[route] TUN routes configured (Linux)");
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        if let Some(ref sip) = cfg.server_ip_hint().await {
            ip(&["route", "del", sip]).await.ok();
        }
        ip(&["link", "del", "dev", &cfg.tun_name]).await.ok();
        info!("[route] TUN routes removed (Linux)");
    }
}

// ── macOS ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    // 针对 macOS 的网络接口与标准路由控制存根实现
    pub async fn tun_up(cfg: &Config, _phys: &PhysicalRoute) -> Result<()> {
        let tun = &cfg.tun_name;
        let tun_ip = &cfg.tun_ip;
        let tun_gw = &cfg.tun_gw;

        info!("[route] Configuring TUN routes (macOS stub)...");

        // 示例：在 macOS 下通过 ifconfig 调通设备层拓扑
        let out = tokio::process::Command::new("ifconfig")
            .args([tun, tun_ip, tun_gw, "up"])
            .output()
            .await?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            debug!("ifconfig failed: {}", stderr.trim());
        }

        info!("[route] TUN routes configured (macOS stub)");
        Ok(())
    }

    pub async fn tun_down(_cfg: &Config) {
        info!("[route] TUN routes removed (macOS stub)");
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

        if let Some(ref sip) = cfg.server_ip_hint().await {
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
        if let Some(ref sip) = cfg.server_ip_hint().await {
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
    /// Attempt to resolve the server hostname to an IP for the bypass route.
    pub async fn server_ip_hint(&self) -> Option<String> {
        let host = extract_host(&self.server)?;
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Some(host);
        }
        match tokio::net::lookup_host(format!("{host}:443")).await {
            Ok(mut addrs) => {
                let ip = addrs.next()?.ip().to_string();
                info!("[route] server {host} resolved to {ip} (bypass route)");
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
