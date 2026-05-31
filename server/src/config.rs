use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "server", version)]
/// Command-line options for the server binary.
pub(crate) struct Cli {
    #[arg(long, short)]
    /// Bind address for the WebSocket server.
    pub addr: Option<String>,
    #[arg(long, short)]
    /// Bind port for the WebSocket server.
    pub port: Option<u16>,
    #[arg(long, short)]
    /// Authentication token shared with clients.
    pub token: Option<String>,
    #[arg(long, short)]
    /// Load settings from a YAML config file.
    pub config: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct Config {
    #[serde(default = "default_addr")]
    pub addr: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub token: String,
}

fn default_addr() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    9880
}

impl Default for Config {
    fn default() -> Self {
        Config {
            addr: default_addr(),
            port: default_port(),
            token: String::new(),
        }
    }
}

impl Config {
    fn from_cli(cli: Cli) -> Result<Self> {
        // load base config from file if specified
        let mut cfg = if let Some(path) = cli.config {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config file: {path}"))?;
            serde_yaml::from_str::<Config>(&text)
                .with_context(|| format!("Failed to parse config file: {path}"))?
        } else {
            Config::default()
        };

        // env overrides file
        if let Ok(addr) = std::env::var("SERVER_ADDR") {
            cfg.addr = addr;
        }
        if let Some(port_str) = std::env::var("SERVER_PORT").ok() {
            cfg.port = port_str.parse().context("invalid SERVER_PORT")?;
        }
        if let Ok(token) = std::env::var("SERVER_TOKEN") {
            cfg.token = token;
        }

        // CLI overrides env
        if let Some(addr) = cli.addr {
            cfg.addr = addr;
        }
        if let Some(port) = cli.port {
            cfg.port = port;
        }
        if let Some(token) = cli.token {
            cfg.token = token;
        }

        Ok(cfg)
    }

    pub(crate) fn load() -> Result<Self> {
        let cli = Cli::parse();
        Self::from_cli(cli)
    }
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub cfg: Arc<Config>,
}
