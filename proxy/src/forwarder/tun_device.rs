// TUN device creation and OS route management.

use crate::config::Config;
use anyhow::Result;
use tracing::{debug, info};

// ── Public TUN handle ────────────────────────────────────────────────────────

pub struct TunDevice {
    dev: Option<tun_rs::AsyncDevice>,
}

impl TunDevice {
    pub fn take_device(&mut self) -> Option<tun_rs::AsyncDevice> {
        self.dev.take()
    }
}

pub fn tun_up(cfg: &Config) -> Result<TunDevice> {
    info!("[tun] creating TUN '{}'", cfg.tun_name);

    let dev = tun_rs::DeviceBuilder::new()
        .name(&cfg.tun_name)
        .build_async()?;

    info!(
        "[tun] created '{}' (ifindex={})",
        dev.name().unwrap_or_default(),
        dev.if_index().unwrap_or(0)
    );

    routes::configure(cfg)?;
    Ok(TunDevice { dev: Some(dev) })
}

pub fn tun_down(cfg: &Config) {
    routes::remove(cfg);
}

// ── OS route management ──────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod routes {
    use super::*;

    pub fn configure(cfg: &Config) -> Result<()> {
        let tun = &cfg.tun_name;

        let out = std::process::Command::new("ip")
            .args(["link", "set", "dev", tun, "up"])
            .output()
            .map_err(|e| anyhow::anyhow!("ip link set: {e}"))?;
        if !out.status.success() {
            anyhow::bail!(
                "ip link set failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }

        let out = std::process::Command::new("ip")
            .args([
                "addr",
                "add",
                &format!("{}/{}", cfg.tun_ip, cfg.tun_prefix),
                "dev",
                tun,
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("ip addr add: {e}"))?;
        if !out.status.success() {
            debug!(
                "ip addr add: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }

        let out = std::process::Command::new("ip")
            .args([
                "-6",
                "addr",
                "add",
                &format!("{}/{}", cfg.tun_ip6, cfg.tun_prefix6),
                "dev",
                tun,
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("ip -6 addr add: {e}"))?;
        if !out.status.success() {
            debug!(
                "ip -6 addr add: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }

        let out = std::process::Command::new("ip")
            .args(["route", "add", "default", "dev", tun, "metric", "1"])
            .output()
            .map_err(|e| anyhow::anyhow!("ip route add: {e}"))?;
        if !out.status.success() {
            debug!(
                "ip route add: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }

        let out = std::process::Command::new("ip")
            .args(["-6", "route", "add", "default", "dev", tun, "metric", "1"])
            .output()
            .map_err(|e| anyhow::anyhow!("ip -6 route add: {e}"))?;
        if !out.status.success() {
            debug!(
                "ip -6 route add: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }

        info!("[tun] TUN routes configured (Linux)");
        Ok(())
    }

    pub fn remove(cfg: &Config) {
        let _ = std::process::Command::new("ip")
            .args(["route", "del", "default", "dev", &cfg.tun_name])
            .output();
        let _ = std::process::Command::new("ip")
            .args(["-6", "route", "del", "default", "dev", &cfg.tun_name])
            .output();
        let _ = std::process::Command::new("ip")
            .args(["link", "set", "dev", &cfg.tun_name, "down"])
            .output();
        info!("[tun] TUN routes removed (Linux)");
    }
}

#[cfg(target_os = "macos")]
mod routes {
    use super::*;
    use anyhow::Context;
    use tracing::warn;

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

    pub fn configure(cfg: &Config) -> Result<()> {
        info!("[tun] Configuring TUN routes (macOS)...");

        let out = std::process::Command::new("ifconfig")
            .args([&cfg.tun_name, &cfg.tun_ip, &cfg.tun_ip, "up"])
            .output()?;
        if !out.status.success() {
            debug!("ifconfig: {}", String::from_utf8_lossy(&out.stderr).trim());
        }

        let out = std::process::Command::new("ifconfig")
            .args([
                &cfg.tun_name,
                "inet6",
                &format!("{}/{}", cfg.tun_ip6, cfg.tun_prefix6),
                "up",
            ])
            .output()?;
        if !out.status.success() {
            debug!(
                "ifconfig inet6: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }

        if let Ok((gw, phys_iface)) = detect_physical_gateway() {
            info!(gateway = %gw, iface = %phys_iface, "[tun] adding scoped route for IP_BOUND_IF");
            let out = std::process::Command::new("route")
                .args([
                    "-n",
                    "add",
                    "-net",
                    "0.0.0.0/0",
                    &gw.to_string(),
                    "-ifscope",
                    &phys_iface,
                ])
                .output();
            match out {
                Ok(o) if !o.status.success() => {
                    debug!(
                        "route add -ifscope: {}",
                        String::from_utf8_lossy(&o.stderr).trim()
                    );
                }
                Err(e) => debug!("route add -ifscope failed: {e}"),
                _ => {}
            }
        } else {
            warn!(
                "[tun] could not detect physical gateway, skipping scoped route (IP_BOUND_IF may fail)"
            );
        }

        for prefix in ["0.0.0.0/1", "128.0.0.0/1"] {
            let out = std::process::Command::new("route")
                .args(["-n", "add", "-net", prefix, &cfg.tun_ip])
                .output()?;
            if !out.status.success() {
                debug!(
                    "route add -net {prefix}: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }
        for prefix in ["::/1", "8000::/1"] {
            let out = std::process::Command::new("route")
                .args(["-n", "add", "-inet6", "-net", prefix, &cfg.tun_ip6])
                .output()?;
            if !out.status.success() {
                debug!(
                    "route add -inet6 {prefix}: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }

        info!("[tun] TUN routes configured (macOS)");
        Ok(())
    }

    pub fn remove(cfg: &Config) {
        if let Ok((gw, phys_iface)) = detect_physical_gateway() {
            let _ = std::process::Command::new("route")
                .args([
                    "-n",
                    "delete",
                    "-net",
                    "0.0.0.0/0",
                    &gw.to_string(),
                    "-ifscope",
                    &phys_iface,
                ])
                .output();
        }
        for prefix in ["0.0.0.0/1", "128.0.0.0/1"] {
            let _ = std::process::Command::new("route")
                .args(["-n", "delete", "-net", prefix])
                .output();
        }
        for prefix in ["::/1", "8000::/1"] {
            let _ = std::process::Command::new("route")
                .args(["-n", "delete", "-inet6", "-net", prefix])
                .output();
        }
        let _ = std::process::Command::new("ifconfig")
            .args([&cfg.tun_name, "down"])
            .output();
        info!("[tun] TUN routes removed (macOS)");
    }
}

#[cfg(windows)]
mod routes {
    use super::*;

    fn powershell(script: &str) -> Result<std::process::Output> {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .output()
            .map_err(|e| anyhow::anyhow!("powershell: {e}"))
    }

    pub fn configure(cfg: &Config) -> Result<()> {
        info!("[tun] Configuring TUN routes (Windows)...");
        let out = powershell(&format!(
            "(Get-NetIPInterface -InterfaceAlias '{}' -AddressFamily IPv4 -ErrorAction SilentlyContinue).InterfaceIndex",
            cfg.tun_name
        ))?;
        let idx = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if idx.is_empty() {
            anyhow::bail!("could not find InterfaceIndex for '{}'", cfg.tun_name);
        }
        info!("[tun] found InterfaceIndex: {}", idx);

        let out = powershell(&format!(
            "Set-NetIPInterface -InterfaceIndex {idx} -AutomaticMetric Disabled -InterfaceMetric 1"
        ))?;
        if !out.status.success() {
            debug!(
                "Set-NetIPInterface: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }

        let out = powershell(&format!(
            "New-NetIPAddress -InterfaceIndex {idx} -IPAddress '{}' -PrefixLength {} -DefaultGateway '{}' -ErrorAction SilentlyContinue",
            cfg.tun_ip, cfg.tun_prefix, cfg.tun_gw
        ))?;
        if !out.status.success() {
            debug!(
                "New-NetIPAddress: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }

        let out = powershell(&format!(
            "Remove-NetRoute -InterfaceIndex {idx} -DestinationPrefix '0.0.0.0/0' -Confirm:$false -ErrorAction SilentlyContinue; \
             New-NetRoute -InterfaceIndex {idx} -DestinationPrefix '0.0.0.0/0' -NextHop '{}' -RouteMetric 1",
            cfg.tun_gw
        ))?;
        if !out.status.success() {
            debug!(
                "default v4 route: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }

        let out = powershell(&format!(
            "Remove-NetRoute -InterfaceIndex {idx} -DestinationPrefix '::/0' -Confirm:$false -ErrorAction SilentlyContinue; \
             New-NetRoute -InterfaceIndex {idx} -DestinationPrefix '::/0' -NextHop 'fe80::1' -RouteMetric 1"
        ))?;
        if !out.status.success() {
            debug!(
                "default v6 route: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }

        info!("[tun] TUN routes configured (Windows, idx={})", idx);
        Ok(())
    }

    pub fn remove(cfg: &Config) {
        // Remove IP address first (New-NetIPAddress fails silently if IP already exists on restart)
        let _ = powershell(&format!(
            "Remove-NetIPAddress -InterfaceAlias '{}' -AddressFamily IPv4 -Confirm:$false -ErrorAction SilentlyContinue",
            cfg.tun_name
        ));
        let _ = powershell(&format!(
            "Remove-NetRoute -InterfaceAlias '{}' -DestinationPrefix '0.0.0.0/0' -Confirm:$false -ErrorAction SilentlyContinue",
            cfg.tun_name
        ));
        let _ = powershell(&format!(
            "Remove-NetRoute -InterfaceAlias '{}' -DestinationPrefix '::/0' -ErrorAction SilentlyContinue",
            cfg.tun_name
        ));
        info!("[tun] TUN routes and IP removed (Windows)");
    }
}
