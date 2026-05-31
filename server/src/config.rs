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

impl Config {
    fn from_cli(cli: Cli) -> Result<Self> {
        if let Some(path) = cli.config {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config file: {path}"))?;
            return serde_yaml::from_str(&text)
                .with_context(|| format!("Failed to parse config file: {path}"));
        }

        let addr = cli.addr
            .or_else(|| std::env::var("SERVER_ADDR").ok())
            .unwrap_or_else(default_addr);

        let port = cli.port
            .or_else(|| std::env::var("SERVER_PORT").ok().and_then(|p| p.parse().ok()))
            .unwrap_or_else(default_port);

        let token = cli.token
            .or_else(|| std::env::var("SERVER_TOKEN").ok())
            .unwrap_or_default();

        Ok(Config { addr, port, token })
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
