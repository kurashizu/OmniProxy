use crate::config::GuiConfig;
use crate::pages::Page;

pub(crate) struct RouteInfo {
    pub(crate) destination: String,
    pub(crate) gateway: String,
    pub(crate) interface: String,
}

pub(crate) struct DashboardApp {
    pub(crate) current_page: Page,

    pub(crate) tun_name: String,
    pub(crate) tun_ip: String,
    pub(crate) ws_connected: bool,
    pub(crate) client_uptime: f64,
    pub(crate) proxy_uptime: f64,
    pub(crate) client_pid: u32,
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

    pub(crate) latency_ms: f64,
    pub(crate) server_resolved_ip: String,
    pub(crate) server_egress_ip4: String,
    pub(crate) server_egress_ip6: String,
    pub(crate) connections_raw: Vec<serde_json::Value>,

    pub(crate) config_path: String,
    pub(crate) config: GuiConfig,
    pub(crate) show_token: bool,
    pub(crate) exe_dir: std::path::PathBuf,
    pub(crate) proxy_handle: Option<crate::native::ProxyHandle>,

    pub(crate) last_poll: web_time::Instant,
}
