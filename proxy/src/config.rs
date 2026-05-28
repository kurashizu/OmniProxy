use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "proxy", about = "Transparent proxy stack manager")]
pub struct Cli {
    #[arg(long, short = 'c')]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub client: Option<PathBuf>,
    #[arg(long)]
    pub tun2socks: Option<PathBuf>,
    #[arg(long)]
    pub server: Option<String>,
    #[arg(long)]
    pub token: Option<String>,
    #[arg(long)]
    pub socks_port: Option<u16>,
    #[arg(long)]
    pub tun_name: Option<String>,
    #[arg(long)]
    pub tun_ip: Option<String>,
    #[arg(long)]
    pub tun_ip6: Option<String>,
    #[arg(long)]
    pub tun_prefix: Option<u8>,
    #[arg(long)]
    pub tun_prefix6: Option<u8>,
    #[arg(long)]
    pub tun_gw: Option<String>,
    #[arg(long)]
    pub tun_gw6: Option<String>,
    #[arg(long)]
    pub phys_iface: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub client: PathBuf,
    #[serde(default)]
    pub tun2socks: PathBuf,
    pub server: String,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default = "default_tun_name")]
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
    #[serde(default)]
    pub phys_iface: Option<String>,
}

fn default_socks_port() -> u16 {
    1080
}

#[cfg(target_os = "macos")]
fn default_tun_name() -> String {
    "utun0".into()
}

#[cfg(not(target_os = "macos"))]
fn default_tun_name() -> String {
    "tun0".into()
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

macro_rules! override_if_some {
    ($cfg:expr, $cli:expr, $field:ident) => {
        if let Some(v) = $cli.$field.clone() {
            $cfg.$field = Some(v);
        }
    };
    ($cfg:expr, $cli:expr, $field:ident, non_option) => {
        if let Some(v) = $cli.$field.clone() {
            $cfg.$field = v;
        }
    };
}

impl Config {
    pub fn load(cli: &Cli) -> Result<Self> {
        let mut cfg: Config = if let Some(ref path) = cli.config {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read config: {}", path.display()))?;
            serde_yaml::from_str(&text)
                .with_context(|| format!("parse config: {}", path.display()))?
        } else {
            Config {
                client: cli.client.clone().unwrap_or_default(),
                tun2socks: cli.tun2socks.clone().unwrap_or_default(),
                server: cli
                    .server
                    .clone()
                    .context("--server or -c config is required")?,
                token: cli.token.clone().unwrap_or_default(),
                socks_port: cli.socks_port.unwrap_or_else(default_socks_port),
                tun_name: cli.tun_name.clone().unwrap_or_else(default_tun_name),
                tun_ip: cli.tun_ip.clone().unwrap_or_else(default_tun_ip),
                tun_ip6: cli.tun_ip6.clone().unwrap_or_else(default_tun_ip6),
                tun_prefix: cli.tun_prefix.unwrap_or_else(default_tun_prefix),
                tun_prefix6: cli.tun_prefix6.unwrap_or_else(default_tun_prefix6),
                tun_gw: cli.tun_gw.clone().unwrap_or_else(default_tun_gw),
                tun_gw6: cli.tun_gw6.clone().unwrap_or_else(default_tun_gw6),
                phys_iface: cli.phys_iface.clone(),
            }
        };

        override_if_some!(cfg, cli, client, non_option);
        override_if_some!(cfg, cli, tun2socks, non_option);
        override_if_some!(cfg, cli, server, non_option);
        override_if_some!(cfg, cli, token, non_option);
        if let Some(v) = cli.socks_port {
            cfg.socks_port = v;
        }
        override_if_some!(cfg, cli, tun_name, non_option);
        override_if_some!(cfg, cli, tun_ip, non_option);
        override_if_some!(cfg, cli, tun_ip6, non_option);
        if let Some(v) = cli.tun_prefix {
            cfg.tun_prefix = v;
        }
        if let Some(v) = cli.tun_prefix6 {
            cfg.tun_prefix6 = v;
        }
        override_if_some!(cfg, cli, tun_gw, non_option);
        override_if_some!(cfg, cli, tun_gw6, non_option);

        let self_dir = std::env::current_exe()
            .map(|p| p.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or(None)
            .unwrap_or_else(|| PathBuf::from("."));

        #[cfg(windows)]
        let (ext_client, ext_t2s) = ("client.exe", "tun2socks.exe");
        #[cfg(not(windows))]
        let (ext_client, ext_t2s) = ("client", "tun2socks");

        if cfg.client.as_os_str().is_empty() {
            cfg.client = self_dir.join(ext_client);
        }
        if cfg.tun2socks.as_os_str().is_empty() {
            cfg.tun2socks = self_dir.join(ext_t2s);
        }

        Ok(cfg)
    }
}
