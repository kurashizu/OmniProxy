use crate::config::GuiConfig;
use crate::pages::Page;

pub(crate) struct RouteInfo {
    pub(crate) destination: String,
    pub(crate) gateway: String,
    pub(crate) interface: String,
}

pub(crate) struct PollResult {
    pub ws_connected: bool,
    pub client_uptime: f64,
    pub proxy_uptime: f64,
    pub reconnect_count: u64,
    pub active_tcp: usize,
    pub active_udp: usize,
    pub active_icmp: usize,
    pub bytes_tx: u64,
    pub bytes_rx: u64,
    pub socks5: String,
    pub server: String,
    pub tun_name: String,
    pub tun_ip: String,
    pub routes: Vec<RouteInfo>,
    pub connections_raw: Vec<serde_json::Value>,
}

pub(crate) struct DashboardApp {
    pub(crate) current_page: Page,

    pub(crate) tun_name: String,
    pub(crate) tun_ip: String,
    pub(crate) ws_connected: bool,
    pub(crate) client_uptime: f64,
    pub(crate) proxy_uptime: f64,
    pub(crate) reconnect_count: u64,
    pub(crate) active_tcp: usize,
    pub(crate) active_udp: usize,
    pub(crate) active_icmp: usize,
    pub(crate) bytes_tx: u64,
    pub(crate) bytes_rx: u64,
    pub(crate) prev_bytes_tx: u64,
    pub(crate) prev_bytes_rx: u64,
    pub(crate) speed_tx: f64,
    pub(crate) speed_rx: f64,
    pub(crate) socks5: String,
    pub(crate) server: String,
    pub(crate) routes: Vec<RouteInfo>,

    pub(crate) connections_raw: Vec<serde_json::Value>,

    pub(crate) config: GuiConfig,
    pub(crate) show_token: bool,
    pub(crate) exe_dir: std::path::PathBuf,
    pub(crate) proxy_handle: Option<crate::native::ProxyHandle>,
    pub(crate) error_msg: Option<(String, web_time::Instant)>,

    pub(crate) dirty: bool,
    pub(crate) dirty_at: Option<web_time::Instant>,

    pub(crate) stats_rx: std::sync::mpsc::Receiver<PollResult>,
}
