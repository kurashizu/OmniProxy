#![cfg(not(target_arch = "wasm32"))]

use std::io::{Read, Write};
use std::process::{Child, Command};
use std::time::Duration;

pub(crate) struct ProxyHandle {
    child: Child,
}

impl ProxyHandle {
    pub(crate) fn start(proxy_bin: &str, config_path: &str) -> Option<Self> {
        Command::new(proxy_bin)
            .args(["-c", config_path])
            .spawn()
            .ok()
            .map(|child| Self { child })
    }

    pub(crate) fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub(crate) fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn fetch_json(host: &str, port: u16, path: &str) -> Option<serde_json::Value> {
    let addr = format!("{host}:{port}");
    let mut stream = std::net::TcpStream::connect_timeout(&addr.parse().ok()?, Duration::from_secs(2)).ok()?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n").as_bytes(),
        )
        .ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    let body = String::from_utf8_lossy(&buf);
    let body = body.split("\r\n\r\n").nth(1).unwrap_or(&body);
    serde_json::from_str(&body).ok()
}
