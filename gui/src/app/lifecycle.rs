use crate::app::DashboardApp;
use crate::app::PollResult;
use crate::config::GuiConfig;
use crate::pages::Page;

fn do_poll_blocking(config: &crate::config::GuiConfig, prev_tx: u64, prev_rx: u64, _dt: f64) -> PollResult {
    use std::io::{Read, Write};
    use std::time::Duration;

    let mut result = PollResult {
        ws_connected: false,
        client_uptime: 0.0,
        proxy_uptime: 0.0,
        reconnect_count: 0,
        active_tcp: 0,
        active_udp: 0,
        active_icmp: 0,
        bytes_tx: prev_tx,
        bytes_rx: prev_rx,
        socks5: String::new(),
        server: String::new(),
        tun_name: String::new(),
        tun_ip: String::new(),
        routes: Vec::new(),
        connections_raw: Vec::new(),
    };

    // ── Client admin ──────────────────────────────────────────────
    let client_port = config.admin_port.saturating_sub(1);
    let client_addr = format!("127.0.0.1:{client_port}");
    if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
        &client_addr.parse().unwrap_or_else(|_| "127.0.0.1:10990".parse().unwrap()),
        Duration::from_secs(1),
    ) {
        let req = "GET /stats HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
        if stream.write_all(req.as_bytes()).is_ok() {
            stream.set_read_timeout(Some(Duration::from_secs(1))).ok();
            let mut buf = Vec::new();
            if stream.read_to_end(&mut buf).is_ok() {
                let body = String::from_utf8_lossy(&buf);
                let body = body.split("\r\n\r\n").nth(1).unwrap_or(&body);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                    result.connections_raw = v.get("connections")
                        .and_then(|c| c.as_array().cloned())
                        .unwrap_or_default();

                    result.ws_connected = v.get("connected").and_then(|b| b.as_bool()).unwrap_or(false);
                    result.client_uptime = v.get("uptime_secs").and_then(|f| f.as_f64()).unwrap_or(0.0);
                    result.reconnect_count = v.get("reconnect_count").and_then(|n| n.as_u64()).unwrap_or(0);
                    result.server = v.get("server").and_then(|s| s.as_str()).unwrap_or("").to_owned();
                    result.socks5 = v.get("socks5").and_then(|s| s.as_str()).unwrap_or("").to_owned();

                    if let Some(active) = v.get("active") {
                        result.active_tcp = active.get("tcp").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
                        result.active_udp = active.get("udp").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
                        result.active_icmp = active.get("icmp").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
                    }

                    if let Some(bytes) = v.get("bytes") {
                        result.bytes_tx = bytes.get("tx").and_then(|n| n.as_u64()).unwrap_or(prev_tx);
                        result.bytes_rx = bytes.get("rx").and_then(|n| n.as_u64()).unwrap_or(prev_rx);
                    }
                }
            }
        }
    }

    // ── Proxy /stats ──────────────────────────────────────────────
    let proxy_addr = format!("127.0.0.1:{}", config.admin_port);
    if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
        &proxy_addr.parse().unwrap_or_else(|_| "127.0.0.1:10991".parse().unwrap()),
        Duration::from_secs(1),
    ) {
        let req = "GET /stats HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
        if stream.write_all(req.as_bytes()).is_ok() {
            stream.set_read_timeout(Some(Duration::from_secs(1))).ok();
            let mut buf = Vec::new();
            if stream.read_to_end(&mut buf).is_ok() {
                let body = String::from_utf8_lossy(&buf);
                let body = body.split("\r\n\r\n").nth(1).unwrap_or(&body);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                    result.proxy_uptime = v.get("uptime_secs").and_then(|f| f.as_f64()).unwrap_or(0.0);
                    if let Some(tun) = v.get("tun") {
                        result.tun_name = tun.get("name").and_then(|s| s.as_str()).unwrap_or("").to_owned();
                        result.tun_ip = tun.get("ip").and_then(|s| s.as_str()).unwrap_or("").to_owned();
                    }
                }
            }
        }
    }

    // ── Proxy /routes ─────────────────────────────────────────────
    if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
        &proxy_addr.parse().unwrap_or_else(|_| "127.0.0.1:10991".parse().unwrap()),
        Duration::from_secs(1),
    ) {
        let req = "GET /routes HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
        if stream.write_all(req.as_bytes()).is_ok() {
            stream.set_read_timeout(Some(Duration::from_secs(1))).ok();
            let mut buf = Vec::new();
            if stream.read_to_end(&mut buf).is_ok() {
                let body = String::from_utf8_lossy(&buf);
                let body = body.split("\r\n\r\n").nth(1).unwrap_or(&body);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                    if let Some(arr) = v.get("routes").and_then(|r| r.as_array()) {
                        result.routes = arr.iter().map(|r| {
                            crate::app::RouteInfo {
                                destination: r.get("destination").and_then(|s| s.as_str()).unwrap_or("").to_owned(),
                                gateway: r.get("gateway").and_then(|s| s.as_str()).unwrap_or("").to_owned(),
                                interface: r.get("interface").and_then(|s| s.as_str()).unwrap_or("").to_owned(),
                            }
                        }).collect();
                    }
                }
            }
        }
    }

    result
}

impl DashboardApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_pixels_per_point(1.0);
        cc.egui_ctx.set_theme(eframe::egui::Theme::Dark);

        let exe_dir = crate::paths::base_dir();

        let config_path = exe_dir.join("config.yml");
        let config: GuiConfig = std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_yaml::from_str(&s).ok())
            .unwrap_or_default();

        let (tx, rx) = std::sync::mpsc::channel::<PollResult>();
        let config_clone = config.clone();

        std::thread::spawn(move || {
            let mut prev_tx: u64 = 0;
            let mut prev_rx: u64 = 0;
            loop {
                let dt = 0.5;
                let result = do_poll_blocking(&config_clone, prev_tx, prev_rx, dt);
                prev_tx = result.bytes_tx;
                prev_rx = result.bytes_rx;
                if tx.send(result).is_err() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        });

        Self {
            current_page: Page::Overview,
            tun_name: String::new(),
            tun_ip: String::new(),
            ws_connected: false,
            client_uptime: 0.0,
            proxy_uptime: 0.0,
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
            connections_raw: Vec::new(),
            config,
            show_token: false,
            exe_dir,
            proxy_handle: None,
            error_msg: None,
            dirty: false,
            dirty_at: None,
            stats_rx: rx,
        }
    }
}
