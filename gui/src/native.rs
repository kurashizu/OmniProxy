#![cfg(not(target_arch = "wasm32"))]

use std::io::{Read, Write};
use std::process::{Child, ChildStderr, Command, Stdio};
use std::time::Duration;

pub(crate) struct ProxyHandle {
    child: Child,
    stderr: Option<ChildStderr>,
}

impl ProxyHandle {
    pub(crate) fn start(proxy_bin: &str, config_path: &str) -> Option<Self> {
        let mut child = Command::new(proxy_bin)
            .args(["-c", config_path])
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;
        let stderr = child.stderr.take();
        Some(Self { child, stderr })
    }

    pub(crate) fn start_sudo(proxy_bin: &str, config_path: &str) -> Option<Self> {
        let mut child = Command::new("sudo")
            .args([proxy_bin, "-c", config_path])
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;
        let stderr = child.stderr.take();
        Some(Self { child, stderr })
    }

    pub(crate) fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub(crate) fn try_read_stderr(&mut self) -> String {
        let mut buf = String::new();
        if let Some(ref mut stderr) = self.stderr {
            use std::io::Read;
            let mut tmp = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut tmp) {
                if n == 0 { break; }
                buf.push_str(&String::from_utf8_lossy(&tmp[..n]));
            }
        }
        buf
    }

    pub(crate) fn wait_status(&mut self) -> Option<std::process::ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    pub(crate) fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
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
