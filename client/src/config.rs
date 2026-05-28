use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(name = "client", version)]
/// Command-line options for the client binary.
struct Cli {
    #[arg(long, short, default_value = "127.0.0.1")]
    /// Local bind address for the SOCKS5 server.
    addr: String,
    #[arg(long, short, default_value = "1080")]
    /// Local bind port for the SOCKS5 server.
    port: u16,
    #[arg(long, short, default_value = "")]
    /// Optional proxy token sent to the remote server.
    token: String,
    #[arg(long, short)]
    /// Remote WebSocket server URL.
    server: Option<String>,
    #[arg(long, short)]
    /// Optional outbound source IP for connecting to the remote server.
    outbound_ip: Option<String>,
    #[arg(long, short)]
    /// Load settings from a YAML config file.
    config: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_addr")]
    pub addr: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub outbound_ip: Option<String>,
}

fn default_addr() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    1080
}

impl Config {
    fn from_cli(cli: Cli) -> Result<Self> {
        match (cli.config, cli.server) {
            (Some(config_path), _) => {
                let config_str = std::fs::read_to_string(&config_path)
                    .with_context(|| format!("Failed to read config file: {}", config_path))?;
                serde_yaml::from_str(&config_str)
                    .with_context(|| format!("Failed to parse config file: {}", config_path))
            }
            (None, Some(server)) => Ok(Self {
                addr: cli.addr,
                port: cli.port,
                token: cli.token,
                server,
                outbound_ip: cli.outbound_ip,
            }),
            (None, None) => anyhow::bail!("Either --config or --server must be provided"),
        }
    }

    pub(crate) fn load() -> Result<Self> {
        let cli = Cli::parse();
        Self::from_cli(cli)
    }
}
