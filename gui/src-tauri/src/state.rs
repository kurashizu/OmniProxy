use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::schema::GuiConfig;

/// Process handle for the spawned `proxy` binary. Wrapped to allow re-use
/// across command invocations.
pub struct ProxyProcess {
    pub child: tokio::process::Child,
    #[allow(dead_code)]
    pub started_at: std::time::Instant,
    /// Last line written to stderr before the process exited. Used to
    /// show a meaningful connection-error message (wrong server / token).
    pub last_error: Arc<tokio::sync::Mutex<Option<String>>>,
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
    /// Currently-spawned proxy process, if any.
    pub child: Arc<Mutex<Option<ProxyProcess>>>,
    /// Path to the GUI's config.yaml.
    pub config_path: PathBuf,
    /// Cached last-known config (helps avoid repeated disk reads).
    pub config: Arc<Mutex<GuiConfig>>,
    /// Current lifecycle state.
    pub proxy_state: Arc<Mutex<ProxyState>>,
    /// Resolved path to the `proxy` binary.
    pub proxy_binary: Arc<Mutex<Option<PathBuf>>>,
}

impl AppState {
    pub fn new(config_path: PathBuf, config: GuiConfig) -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            config_path,
            config: Arc::new(Mutex::new(config)),
            proxy_state: Arc::new(Mutex::new(ProxyState::default())),
            proxy_binary: Arc::new(Mutex::new(None)),
        }
    }
}
