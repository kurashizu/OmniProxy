use crate::{network::PhysicalRoute, Config};
use anyhow::Result;
use tracing::{debug, info, warn};

// ── Public API ────────────────────────────────────────────────────────────────

pub async fn tun_up(cfg: &Config, phys: &PhysicalRoute) -> Result<()> {
    imp::tun_up(cfg, phys).await
}

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
            .map_err(|e| anyhow::anyhow!("ip {}: {e}", args.join(" ")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("ip {}: {}", args.join(" "), stderr.trim());
        }
        Ok(())
    }

    pub async fn tun_up(cfg: &Config, phys: &PhysicalRoute) -> Result<()> {
        let tun = &cfg.tun_name;
        let tun_ip = &cfg.tun_ip;
        let prefix = cfg.tun_prefix;
        let tun_gw = &cfg.tun_gw;
        let server_ip = cfg.server_ip_hint().await;

        ip(&["link", "set", tun, "up"]).await?;
        ip(&["addr", "flush", "dev", tun]).await?;
        ip_strict(&["addr", "add", &format!("{tun_ip}/{prefix}"), "dev", tun]).await?;

        if let Some(ref sip) = server_ip {
            let gw = phys.gateway.to_string();
            let dev = &phys.iface;
            ip(&["route", "del", &format!("{sip}/32")]).await?;
            ip_strict(&[
                "route",
                "add",
                &format!("{sip}/32"),
                "via",
                &gw,
                "dev",
                dev,
                "metric",
                "1",
            ])
            .await?;
            info!("[route] server {sip}/32 → via {gw} dev {dev}");
        }

        ip(&["route", "del", "default", "dev", tun]).await?;
        ip_strict(&[
            "route", "add", "default", "dev", tun, "metric", "0", "proto", "static",
        ])
        .await?;

        ip(&["-6", "route", "del", "default", "dev", tun]).await?;
        ip(&[
            "-6", "route", "add", "::/0", "dev", tun, "metric", "1", "proto", "static",
        ])
        .await?;

        ip(&["addr", "add", &format!("{tun_gw}/{prefix}"), "dev", tun]).await?;

        info!("[route] TUN routes configured (default → {tun})");
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        let tun = &cfg.tun_name;
        if let Some(ref sip) = cfg.server_ip_hint().await {
            ip(&["route", "del", &format!("{sip}/32")]).await.ok();
        }
        ip(&["route", "del", "default", "dev", tun]).await.ok();
        ip(&["-6", "route", "del", "default", "dev", tun])
            .await
            .ok();
        ip(&["link", "set", tun, "down"]).await.ok();
        info!("[route] TUN routes removed");
    }
}

// ── macOS ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    async fn run(prog: &str, args: &[&str]) -> Result<()> {
        let out = tokio::process::Command::new(prog)
            .args(args)
            .output()
            .await?;
        if !out.status.success() {
            let e = String::from_utf8_lossy(&out.stderr);
            debug!("{prog} {} → {e}", args.join(" "));
        }
        Ok(())
    }

    pub async fn tun_up(cfg: &Config, phys: &PhysicalRoute) -> Result<()> {
        let tun = &cfg.tun_name;
        let tun_ip = &cfg.tun_ip;
        let tun_gw = &cfg.tun_gw;
        let prefix = cfg.tun_prefix;

        run(
            "ifconfig",
            &[tun, &format!("{tun_ip}/{prefix}"), tun_gw, "up"],
        )
        .await?;

        if let Some(ref sip) = cfg.server_ip_hint().await {
            let gw = phys.gateway.to_string();
            run("route", &["-n", "add", sip, &gw]).await?;
        }

        run("route", &["-n", "add", "-net", "0.0.0.0/1", tun_gw]).await?;
        run("route", &["-n", "add", "-net", "128.0.0.0/1", tun_gw]).await?;

        info!("[route] TUN routes configured (macOS)");
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        let tun_gw = &cfg.tun_gw;
        if let Some(ref sip) = cfg.server_ip_hint().await {
            tokio::process::Command::new("route")
                .args(["-n", "delete", sip])
                .output()
                .await
                .ok();
        }
        tokio::process::Command::new("route")
            .args(["-n", "delete", "-net", "0.0.0.0/1", tun_gw])
            .output()
            .await
            .ok();
        tokio::process::Command::new("route")
            .args(["-n", "delete", "-net", "128.0.0.0/1", tun_gw])
            .output()
            .await
            .ok();
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

        // 引入轮询等待机制（最多等10秒，每500ms拉取一次）
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

        // 禁用 TUN 上的自动跃点
        ps(&format!(
            "Set-NetIPInterface -InterfaceIndex {idx} -AutomaticMetric Disabled -InterfaceMetric 1"
        ))
        .await?;

        // 绑定虚体 IP
        ps(&format!(
            "New-NetIPAddress -InterfaceIndex {idx} -IPAddress '{tun_ip}' -PrefixLength {prefix} -DefaultGateway '{tun_gw}' -ErrorAction SilentlyContinue"
        )).await?;

        // 服务器旁路静态路由
        if let Some(ref sip) = cfg.server_ip_hint().await {
            let gw = phys.gateway.to_string();
            let dev = &phys.iface;
            ps(&format!(
                "Remove-NetRoute -DestinationPrefix '{sip}/32' -ErrorAction SilentlyContinue; \
                 New-NetRoute -InterfaceAlias '{dev}' -DestinationPrefix '{sip}/32' -NextHop '{gw}' -RouteMetric 1"
            )).await?;
        }

        // 接管系统全局默认路由
        ps(&format!(
            "Remove-NetRoute -InterfaceIndex {idx} -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue; \
             New-NetRoute -InterfaceIndex {idx} -DestinationPrefix '0.0.0.0/0' -NextHop '{tun_gw}' -RouteMetric 1"
        )).await?;
        ps(&format!(
            "Remove-NetRoute -InterfaceIndex {idx} -DestinationPrefix '::/0' -ErrorAction SilentlyContinue; \
             New-NetRoute -InterfaceIndex {idx} -DestinationPrefix '::/0' -NextHop '::' -RouteMetric 1"
        )).await?;

        info!("[route] TUN routes configured (Windows, idx={idx})");
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        let tun = &cfg.tun_name;
        if let Some(ref sip) = cfg.server_ip_hint().await {
            tokio::process::Command::new("powershell")
                .args(["-NoProfile", "-Command",
                    &format!("Remove-NetRoute -DestinationPrefix '{sip}/32' -ErrorAction SilentlyContinue")])
                .output().await.ok();
        }
        tokio::process::Command::new("powershell")
            .args(["-NoProfile", "-Command",
                &format!("Remove-NetRoute -InterfaceAlias '{tun}' -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue")])
            .output().await.ok();
        info!("[route] TUN routes removed (Windows)");
    }
}

// ── Config helper: resolve server IP at startup ───────────────────────────────

impl Config {
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
        Some(server.split(':').next()?.to_string())
    }
}
