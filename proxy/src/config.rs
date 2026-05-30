use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// CLI arguments.
#[derive(Parser, Debug)]
#[command(name = "proxy", version, about = "Transparent proxy stack manager")]
pub struct Cli {
    /// Path to YAML config file
    #[arg(long, short = 'c')]
    pub config: Option<PathBuf>,

    /// Path to client binary
    #[cfg(target_os = "windows")]
    #[arg(long, short = 'C', default_value = r".\client.exe")]
    pub client: Option<PathBuf>,

    #[cfg(not(target_os = "windows"))]
    #[arg(long, short = 'C', default_value = "./client")]
    pub client: Option<PathBuf>,

    /// WebSocket server URL (e.g., example.com)
    #[arg(long, short = 's')]
    pub server: Option<String>,

    /// Auth token for server
    #[arg(long, short = 't')]
    pub token: Option<String>,

    /// Local SOCKS5 port
    #[arg(long, short = 'p', default_value = "1080")]
    pub socks_port: u16,

    /// TUN interface name
    #[arg(long, short = 'd', default_value_t = tun_name_default())]
    pub tun_name: String,

    /// TUN IPv4 address
    #[arg(long)]
    pub tun_ip: Option<String>,

    /// TUN IPv6 address
    #[arg(long)]
    pub tun_ip6: Option<String>,

    /// TUN IPv4 prefix length
    #[arg(long)]
    pub tun_prefix: Option<u8>,

    /// TUN IPv6 prefix length
    #[arg(long)]
    pub tun_prefix6: Option<u8>,

    /// TUN IPv4 gateway
    #[arg(long)]
    pub tun_gw: Option<String>,

    /// TUN IPv6 gateway
    #[arg(long)]
    pub tun_gw6: Option<String>,

    /// Manual: outbound IP for client binding
    #[arg(long, short = 'o')]
    pub phys_ip: Option<String>,

    /// Admin HTTP API port
    #[arg(long, default_value = "10991")]
    pub admin_port: u16,
}

// Config structure for YAML / CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub client: PathBuf,
    pub server: String,
    pub token: String,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default = "tun_name_default")]
    pub tun_name: String,
    #[serde(default = "default_tun_ip")]
    pub tun_ip: String,
    #[serde(default = "default_tun_ip6")]
    pub tun_ip6: String,
    #[serde(default = "default_tun_prefix")]
    pub tun_prefix: u8,
    #[serde(default = "default_tun_prefix6")]
    pub tun_prefix6: u8,
    #[serde(default = "default_tun_gw")]
    pub tun_gw: String,
    #[serde(default = "default_tun_gw6")]
    pub tun_gw6: String,
    pub phys_ip: Option<String>,
    #[serde(default = "default_admin_port")]
    pub admin_port: u16,
}

fn default_socks_port() -> u16 {
    1080
}

fn default_admin_port() -> u16 {
    10991
}

fn tun_name_default() -> String {
    if cfg!(target_os = "macos") {
        "utun99".into()
    } else {
        "tun0".into()
    }
}

fn default_tun_ip() -> String {
    "198.18.0.1".into()
}

fn default_tun_ip6() -> String {
    "fd00::1".into()
}

fn default_tun_prefix() -> u8 {
    16
}

fn default_tun_prefix6() -> u8 {
    64
}

fn default_tun_gw() -> String {
    "198.18.0.2".into()
}

fn default_tun_gw6() -> String {
    "fd00::2".into()
}

impl Config {
    // Load config: if config file is provided, use it directly.
    // Otherwise, build from CLI args with defaults.
    pub fn load(cli: &Cli) -> Result<Self> {
        // If config file is specified, ignore all other args
        if let Some(ref path) = cli.config {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read config: {}", path.display()))?;
            let mut cfg: Config = serde_yaml::from_str(&text)
                .with_context(|| format!("parse config: {}", path.display()))?;

            // Resolve relative paths to executable directory
            let self_dir = std::env::current_exe()
                .map(|p| p.parent().map(|parent| parent.to_path_buf()))
                .unwrap_or(None)
                .unwrap_or_else(|| PathBuf::from("."));

            if cfg.client.as_os_str().is_empty() {
                #[cfg(windows)]
                let ext = "client.exe";
                #[cfg(not(windows))]
                let ext = "client";
                cfg.client = self_dir.join(ext);
            }

            return Ok(cfg);
        }

        // No config file: build from CLI args
        let server = cli
            .server
            .clone()
            .context("--server is required (or use -c to specify config file)")?;

        let tun_ip = cli.tun_ip.clone().unwrap_or_else(default_tun_ip);
        let tun_ip6 = cli.tun_ip6.clone().unwrap_or_else(default_tun_ip6);
        let tun_prefix = cli.tun_prefix.unwrap_or(16);
        let tun_prefix6 = cli.tun_prefix6.unwrap_or(64);
        let tun_gw = cli.tun_gw.clone().unwrap_or_else(default_tun_gw);
        let tun_gw6 = cli.tun_gw6.clone().unwrap_or_else(default_tun_gw6);

        let client = cli.client.clone().unwrap();

        Ok(Config {
            client,
            server,
            token: cli.token.clone().unwrap_or_default(),
            socks_port: cli.socks_port,
            tun_name: cli.tun_name.clone(),
            tun_ip,
            tun_ip6,
            tun_prefix,
            tun_prefix6,
            tun_gw,
            tun_gw6,
            phys_ip: cli.phys_ip.clone(),
            admin_port: cli.admin_port,
        })
    }
}
