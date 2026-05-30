#![cfg(not(target_arch = "wasm32"))]

use std::io::Read;
use std::process::{Child, ChildStderr, Command, Stdio};

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
        let stderr = child.stderr.take().map(set_non_blocking);
        Some(Self { child, stderr })
    }

    pub(crate) fn start_sudo(proxy_bin: &str, config_path: &str) -> Option<Self> {
        #[cfg(target_os = "macos")]
        {
            let script = format!(
                "do shell script \"{} -c {}\" with administrator privileges",
                proxy_bin.replace('"', "\\\""),
                config_path.replace('"', "\\\"")
            );
            let mut child = Command::new("osascript")
                .args(["-e", &script])
                .stderr(Stdio::piped())
                .spawn()
                .ok()?;
            let stderr = child.stderr.take().map(set_non_blocking);
            return Some(Self { child, stderr });
        }

        #[cfg(not(target_os = "macos"))]
        {
            let mut child = Command::new("sudo")
                .args([proxy_bin, "-c", config_path])
                .stderr(Stdio::piped())
                .spawn()
                .ok()?;
            let stderr = child.stderr.take().map(set_non_blocking);
            Some(Self { child, stderr })
        }
    }

    pub(crate) fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub(crate) fn try_read_stderr(&mut self) -> String {
        let mut buf = String::new();
        if let Some(ref mut stderr) = self.stderr {
            let mut tmp = [0u8; 4096];
            loop {
                match stderr.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => buf.push_str(&String::from_utf8_lossy(&tmp[..n])),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
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

#[cfg(unix)]
fn set_non_blocking(stderr: ChildStderr) -> ChildStderr {
    use std::os::unix::io::AsRawFd;
    let fd = stderr.as_raw_fd();
    unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) };
    stderr
}

#[cfg(not(unix))]
fn set_non_blocking(stderr: ChildStderr) -> ChildStderr {
    stderr
}
