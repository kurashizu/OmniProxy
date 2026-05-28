mod bootstrap;
mod config;
mod network;
mod route;
mod stack;

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = config::Cli::parse();
    let cfg = config::Config::load(&cli)?;
    bootstrap::run(cfg).await
}
