use crate::{Config, network::PhysicalRoute};
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

    /// Run `ip` command, log on error but don't fail (many commands are idempotent).
    async fn ip(args: &[&str]) -> Result<()> {
        let out = tokio::process::Command::new("ip")
            .args(args)
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("ip {}: {e}", args.join(" ")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            // "RTNETLINK answers: File exists" etc. are usually fine for idempotent ops
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

        // Ensure the TUN device exists (tun2socks creates it, but may not have started yet;
        // this is fine — tun_up is called before tun2socks. We just pre-configure routing.)
        // Actually: bring up after tun2socks starts. So we call tun_up after the brief delay.

        // 1. Bring the interface up (tun2socks will have created it)
        ip(&["link", "set", tun, "up"]).await?;

        // 2. Assign virtual IP
        ip(&["addr", "flush", "dev", tun]).await?;
        ip_strict(&["addr", "add", &format!("{tun_ip}/{prefix}"), "dev", tun]).await?;

        // 3. Server bypass route — server IP always walks the physical interface.
        //    This is the crucial anti-loop route.
        if let Some(ref sip) = server_ip {
            let gw = phys.gateway.to_string();
            let dev = &phys.iface;
            // Delete first (idempotent), then add
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

        // 4. Default routes through TUN (metric lower than physical)
        // IPv4
        ip(&["route", "del", "default", "dev", tun]).await?;
        ip_strict(&[
            "route", "add", "default", "dev", tun, "metric", "0", "proto", "static",
        ])
        .await?;

        // IPv6
        ip(&["-6", "route", "del", "default", "dev", tun]).await?;
        // metric 0 is silently bumped to 1024 for IPv6; use metric 1 instead
        ip(&[
            "-6", "route", "add", "::/0", "dev", tun, "metric", "1", "proto", "static",
        ])
        .await?;

        // 5. Assign TUN a gateway address (some tun2socks builds need it)
        ip(&["addr", "add", &format!("{tun_gw}/{prefix}"), "dev", tun]).await?;

        info!("[route] TUN routes configured (default → {tun})");
        Ok(())
    }

    pub async fn tun_down(cfg: &Config) {
        let tun = &cfg.tun_name;

        // Remove server bypass routes
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

        // Server bypass
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

        // Find TUN adapter (WireGuard Tunnel / Wintun)
        let find_idx = format!(
            r#"(Get-NetAdapter | Where-Object {{ $_.InterfaceAlias -eq '{tun}' -or $_.InterfaceDescription -like '*Wintun*' -or $_.InterfaceDescription -like '*WireGuard*' }} | Select-Object -First 1).InterfaceIndex"#
        );
        let out = tokio::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &find_idx])
            .output()
            .await?;
        let idx = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if idx.is_empty() {
            anyhow::bail!(
                "could not find TUN adapter '{tun}' — is wintun.dll present and tun2socks running?"
            );
        }

        // Disable auto-metric on TUN
        ps(&format!(
            "Set-NetIPInterface -InterfaceIndex {idx} -AutomaticMetric Disabled -InterfaceMetric 1"
        ))
        .await?;

        // Assign IP
        ps(&format!(
            "New-NetIPAddress -InterfaceIndex {idx} -IPAddress '{tun_ip}' -PrefixLength {prefix} -DefaultGateway '{tun_gw}' -ErrorAction SilentlyContinue"
        )).await?;

        // Server bypass route
        if let Some(ref sip) = cfg.server_ip_hint().await {
            let gw = phys.gateway.to_string();
            let dev = &phys.iface;
            ps(&format!(
                "Remove-NetRoute -DestinationPrefix '{sip}/32' -ErrorAction SilentlyContinue; \
                 New-NetRoute -InterfaceAlias '{dev}' -DestinationPrefix '{sip}/32' -NextHop '{gw}' -RouteMetric 1"
            )).await?;
        }

        // Default routes through TUN
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
    /// Attempt to resolve the server hostname to an IP for the bypass route.
    /// Returns None if the server field is already an IP or resolution fails.
    pub async fn server_ip_hint(&self) -> Option<String> {
        let host = extract_host(&self.server)?;
        // Already an IP?
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Some(host);
        }
        // Resolve
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
        // "host" or "host:port"
        Some(server.split(':').next()?.to_string())
    }
}
