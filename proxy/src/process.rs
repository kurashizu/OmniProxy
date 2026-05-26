use anyhow::Result;
use std::path::Path;
use tokio::process::{Child, Command};
use tracing::warn;

/// Spawn a child process, forwarding its stdout/stderr to the current process's streams.
pub fn spawn(bin: &Path, args: &[String], label: &str) -> Result<Child> {
    let child = Command::new(bin)
        .args(args)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!("spawn {label} ({bin:?}): {e}"))?;
    Ok(child)
}

/// Attempt a graceful kill then wait; swallow errors.
pub async fn kill_quiet(child: &mut Child) {
    // Try graceful first
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            // Give it 2 seconds to exit
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await;
        }
    }
    // Force kill if still running
    if let Err(e) = child.kill().await {
        // "process already exited" is fine
        if e.kind() != std::io::ErrorKind::InvalidInput {
            warn!("kill: {e}");
        }
    }
    child.wait().await.ok();
}

#[cfg(unix)]
extern crate libc;
