#![cfg(not(target_arch = "wasm32"))]

use std::io::Read;
use std::process::{Child, ChildStderr, Command, Stdio};
#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub(crate) struct ProxyHandle {
    child: Child,
    stderr: Option<ChildStderr>,
}

impl ProxyHandle {
    pub(crate) fn start(proxy_bin: &str, config_path: &str) -> Option<Self> {
        let mut cmd = Command::new(proxy_bin);
        cmd.args(["-c", config_path])
            .stderr(Stdio::piped())
            .stdout(Stdio::piped());
        #[cfg(unix)]
        cmd.process_group(0);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().ok()?;
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
            let mut cmd = Command::new("osascript");
            cmd.args(["-e", &script])
                .stderr(Stdio::piped());
            cmd.process_group(0);
            let mut child = cmd.spawn().ok()?;
            let stderr = child.stderr.take().map(set_non_blocking);
            return Some(Self { child, stderr });
        }

        #[cfg(not(target_os = "macos"))]
        {
            let mut cmd = Command::new("sudo");
            cmd.args([proxy_bin, "-c", config_path])
                .stderr(Stdio::piped());
            cmd.process_group(0);
            let mut child = cmd.spawn().ok()?;
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
        #[cfg(unix)]
        {
            let pid = self.child.id();
            if pid != 0 {
                let pgid = -(pid as i32);
                unsafe { libc::kill(pgid, libc::SIGTERM); }
                std::thread::sleep(std::time::Duration::from_millis(500));
                if self.is_alive() {
                    unsafe { libc::kill(pgid, libc::SIGKILL); }
                }
            }
        }
        #[cfg(windows)]
        {
            let pid = self.child.id();
            if pid != 0 {
                let _ = Command::new("taskkill")
                    .args(["/T", "/F", "/PID", &pid.to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
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
