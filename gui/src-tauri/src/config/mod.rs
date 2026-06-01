pub mod schema;

pub use schema::{GuiConfig, NodeConfig};

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Resolve the configuration file path.
///
/// Production: `<gui-exe-dir>/config.yaml`.
/// Dev (`tauri dev`): `gui/config.yaml` (relative to project root).
pub fn default_config_path() -> PathBuf {
    if cfg!(debug_assertions) {
        // dev mode: prefer gui/config.yaml when running from the project root
        if let Ok(cwd) = std::env::current_dir() {
            let candidate = cwd.join("config.yaml");
            if candidate.exists() {
                return candidate;
            }
            let candidate = cwd.join("gui").join("config.yaml");
            if candidate.exists() {
                return candidate;
            }
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        return dir.join("config.yaml");
    }
    PathBuf::from("config.yaml")
}

/// Load the GUI config. If the file does not exist, write the default to
/// the resolved path and return it.
pub fn load_or_init(path: &Path) -> Result<GuiConfig> {
    if !path.exists() {
        let cfg = GuiConfig::default_with_node();
        write(path, &cfg).with_context(|| format!("write default config to {}", path.display()))?;
        return Ok(cfg);
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
    let cfg: GuiConfig =
        serde_yaml::from_str(&text).with_context(|| format!("parse config {}", path.display()))?;
    Ok(cfg)
}

pub fn write(path: &Path, cfg: &GuiConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let text = serde_yaml::to_string(cfg).context("serialize config to yaml")?;
    std::fs::write(path, text).with_context(|| format!("write config {}", path.display()))?;
    Ok(())
}

pub fn validate(node: &NodeConfig) -> Result<()> {
    if node.server.trim().is_empty() {
        anyhow::bail!("server is required");
    }
    if node.socks_port == 0 {
        anyhow::bail!("socks_port must be > 0");
    }
    if node.admin_port < 2 {
        anyhow::bail!("admin_port must be >= 2 (client admin uses admin_port - 1)");
    }
    if node.tun_prefix == 0 || node.tun_prefix > 32 {
        anyhow::bail!("tun_prefix must be 1..=32");
    }
    if node.tun_prefix6 == 0 || node.tun_prefix6 > 128 {
        anyhow::bail!("tun_prefix6 must be 1..=128");
    }
    if let Some(ip) = &node.phys_ip
        && !ip.is_empty()
        && ip.parse::<std::net::IpAddr>().is_err()
    {
        anyhow::bail!("phys_ip is not a valid IP address: {ip}");
    }
    Ok(())
}
