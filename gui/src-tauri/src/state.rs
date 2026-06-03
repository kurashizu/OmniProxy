use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

use crate::config::schema::GuiConfig;

/// Handle for a running proxy process. The actual `tokio::process::Child` is
/// owned by the waiter task; this struct only carries the bits other code
/// needs to interact with it (signal it to stop, read its last stderr line).
pub struct ProxyProcess {
    /// Monotonic id assigned at spawn time. The waiter uses this to detect
    /// that its slot has been replaced by a newer start_proxy and must
    /// therefore not touch the state machine.
    pub id: u64,
    #[allow(dead_code)]
    pub started_at: std::time::Instant,
    /// Last stderr line written by the child. Mirrored from the log reader
    /// so the waiter can surface a meaningful error message after exit.
    pub last_error: Arc<Mutex<Option<String>>>,
    /// Signal channel: stop_proxy (or the window-close handler) sends
    /// `true` to ask the waiter to terminate the child.
    pub stop_tx: watch::Sender<bool>,
}

/// Snapshot of the proxy's lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyStateKind {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProxyState {
    pub state: ProxyStateKind,
    pub pid: u32,
    pub exit_code: Option<i32>,
    pub message: Option<String>,
}

impl Default for ProxyState {
    fn default() -> Self {
        Self {
            state: ProxyStateKind::Stopped,
            pid: 0,
            exit_code: None,
            message: None,
        }
    }
}

/// Shared application state.
pub struct AppState {
    /// Currently-running proxy handle, if any. Holds the stop signal and
    /// last_error, but NOT the `Child` itself (the waiter task owns that).
    pub child: Arc<Mutex<Option<ProxyProcess>>>,
    /// Path to the GUI's config.yaml.
    pub config_path: PathBuf,
    /// Cached last-known config (helps avoid repeated disk reads).
    pub config: Arc<Mutex<GuiConfig>>,
    /// Current lifecycle state.
    pub proxy_state: Arc<Mutex<ProxyState>>,
    /// Resolved path to the `proxy` binary.
    pub proxy_binary: Arc<Mutex<Option<PathBuf>>>,
    /// Monotonic counter incremented on every successful spawn. The
    /// waiter records the value at spawn time and only updates state /
    /// clears the slot if the counter still matches.
    pub next_proxy_id: Arc<AtomicU64>,
}

impl AppState {
    pub fn new(config_path: PathBuf, config: GuiConfig) -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            config_path,
            config: Arc::new(Mutex::new(config)),
            proxy_state: Arc::new(Mutex::new(ProxyState::default())),
            proxy_binary: Arc::new(Mutex::new(None)),
            next_proxy_id: Arc::new(AtomicU64::new(0)),
        }
    }
}
