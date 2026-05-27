use anyhow::Result;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, info};

mod codec;
mod bootstrap;
mod config;
mod mux;
mod socks5;
mod ws;

use crate::{config::Config, mux::Mux, socks5::handle};

#[tokio::main]
async fn main() -> Result<()> {
    // 1) 基础运行时初始化。
    bootstrap::init();

    // 2) 读取 CLI / 配置文件。
    let cfg = Config::load()?;

    // 3) 先连上远端，建立 mux，并启动断线重连。
    let mux = Mux::connect_mux(&cfg).await?;

    // 4) 监听本地 SOCKS5，并把请求转发到 mux。
    run_client(cfg, mux).await
}

async fn run_client(cfg: Config, mux: Arc<Mux>) -> Result<()> {
    // 本地 SOCKS5 监听地址。
    let bind = format!("{}:{}", cfg.addr, cfg.port);
    info!("socks5 on {bind}  →  {}", cfg.server);

    // 接入本地客户端连接，每个连接单独处理。
    let listener = TcpListener::bind(&bind).await?;
    loop {
        let (stream, peer) = listener.accept().await?;
        debug!("[+] {peer}");
        let mux = mux.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, mux).await {
                debug!("[!] {peer}: {e:#}");
            }
        });
    }
}
