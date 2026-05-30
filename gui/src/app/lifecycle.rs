use crate::app::DashboardApp;
use crate::config::GuiConfig;
use crate::pages::Page;

impl DashboardApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_pixels_per_point(1.0);

        let config = GuiConfig::default();
        Self {
            current_page: Page::Overview,
            tun_name: String::new(),
            tun_ip: String::new(),
            ws_connected: false,
            client_uptime: 0.0,
            proxy_uptime: 0.0,
            client_pid: 0,
            reconnect_count: 0,
            active_tcp: 0,
            active_udp: 0,
            active_icmp: 0,
            bytes_tx: 0,
            bytes_rx: 0,
            prev_bytes_tx: 0,
            prev_bytes_rx: 0,
            speed_tx: 0.0,
            speed_rx: 0.0,
            socks5: String::new(),
            server: String::new(),
            routes: Vec::new(),
            latency_ms: 0.0,
            connections_raw: Vec::new(),
            server_resolved_ip: String::new(),
            server_egress_ip4: String::new(),
            server_egress_ip6: String::new(),
            config_path: String::new(),
            config,
            show_token: false,
            last_poll: web_time::Instant::now(),
        }
    }
}
