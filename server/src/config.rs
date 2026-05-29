use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "server", version)]
/// Command-line options for the server binary.
pub(crate) struct Cli {
    #[arg(long, short, default_value = "0.0.0.0")]
    /// Bind address for the WebSocket server.
    pub addr: String,
    #[arg(long, short, default_value = "9880")]
    /// Bind port for the WebSocket server.
    pub port: u16,
    #[arg(long, short, default_value = "")]
    /// Authentication token shared with clients.
    pub token: String,
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

impl Config {
    fn from_cli(cli: Cli) -> Result<Self> {
        if let Some(path) = cli.config {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config file: {path}"))?;
            return serde_yaml::from_str(&text)
                .with_context(|| format!("Failed to parse config file: {path}"));
        }
        Ok(Config {
            addr: cli.addr,
            port: cli.port,
            token: cli.token,
        })
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
