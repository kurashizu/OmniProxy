use eframe::egui;
use crate::app::DashboardApp;

impl eframe::App for DashboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl DashboardApp {
    pub(crate) fn ui(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.last_poll.elapsed() > std::time::Duration::from_millis(500) {
            let elapsed = self.last_poll.elapsed();
            self.last_poll = web_time::Instant::now();
            self.poll_connections(elapsed);
            self.check_proxy_alive();
        }

        self.top_bar(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(8.0, 8.0)))
            .show(ctx, |ui| self.show_page(ui));
    }

    fn show_page(&mut self, ui: &mut egui::Ui) {
        use crate::pages::Page;
        match self.current_page {
            Page::Overview => self.overview_page(ui),
            Page::Connections => self.connections_page(ui),
            Page::Settings => self.settings_page(ui),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_connections(&mut self, elapsed: std::time::Duration) {
        use std::io::{Read, Write};
        use std::time::Duration as StdDuration;

        // ── Client admin (10990) ───────────────────────────────────────
        let addr = format!("127.0.0.1:{}", self.config.admin_port.saturating_sub(1));
        if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
            &addr.parse().unwrap_or_else(|_| "127.0.0.1:10990".parse().unwrap()),
            StdDuration::from_secs(1),
        ) {
            let req = format!("GET /stats HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
            let _ = stream.write_all(req.as_bytes());
            stream.set_read_timeout(Some(StdDuration::from_secs(1))).ok();
            let mut buf = Vec::new();
            if stream.read_to_end(&mut buf).is_ok() {
                let body = String::from_utf8_lossy(&buf);
                let body = body.split("\r\n\r\n").nth(1).unwrap_or(&body);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                    self.connections_raw = v.get("connections")
                        .and_then(|c| c.as_array().cloned())
                        .unwrap_or_default();

                    self.ws_connected = v.get("connected").and_then(|b| b.as_bool()).unwrap_or(false);
                    self.client_uptime = v.get("uptime_secs").and_then(|f| f.as_f64()).unwrap_or(0.0);
                    self.reconnect_count = v.get("reconnect_count").and_then(|n| n.as_u64()).unwrap_or(0);
                    self.server = v.get("server").and_then(|s| s.as_str()).unwrap_or("").to_owned();
                    self.socks5 = v.get("socks5").and_then(|s| s.as_str()).unwrap_or("").to_owned();

                    if let Some(active) = v.get("active") {
                        self.active_tcp = active.get("tcp").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
                        self.active_udp = active.get("udp").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
                        self.active_icmp = active.get("icmp").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
                    }

                    if let Some(bytes) = v.get("bytes") {
                        let cur_tx = bytes.get("tx").and_then(|n| n.as_u64()).unwrap_or(0);
                        let cur_rx = bytes.get("rx").and_then(|n| n.as_u64()).unwrap_or(0);
                        let dt = elapsed.as_secs_f64().max(0.001);
                        self.speed_tx = (cur_tx as f64 - self.prev_bytes_tx as f64) / dt;
                        self.speed_rx = (cur_rx as f64 - self.prev_bytes_rx as f64) / dt;
                        self.bytes_tx = cur_tx;
                        self.bytes_rx = cur_rx;
                        self.prev_bytes_tx = cur_tx;
                        self.prev_bytes_rx = cur_rx;
                    }
                }
            }
        }

        // ── Proxy admin (10991) ────────────────────────────────────────
        let proxy_addr = format!("127.0.0.1:{}", self.config.admin_port);
        if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
            &proxy_addr.parse().unwrap_or_else(|_| "127.0.0.1:10991".parse().unwrap()),
            StdDuration::from_secs(1),
        ) {
            // /stats
            let req = "GET /stats HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(req.as_bytes());
            stream.set_read_timeout(Some(StdDuration::from_secs(1))).ok();
            let mut buf = Vec::new();
            if stream.read_to_end(&mut buf).is_ok() {
                let body = String::from_utf8_lossy(&buf);
                let body = body.split("\r\n\r\n").nth(1).unwrap_or(&body);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                    self.proxy_uptime = v.get("uptime_secs").and_then(|f| f.as_f64()).unwrap_or(0.0);

                    if let Some(tun) = v.get("tun") {
                        self.tun_name = tun.get("name").and_then(|s| s.as_str()).unwrap_or("").to_owned();
                        self.tun_ip = tun.get("ip").and_then(|s| s.as_str()).unwrap_or("").to_owned();
                    }
                }
            }
        }

        // ── Proxy /routes ─────────────────────────────────────────────
        if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
            &proxy_addr.parse().unwrap_or_else(|_| "127.0.0.1:10991".parse().unwrap()),
            StdDuration::from_secs(1),
        ) {
            let req = "GET /routes HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(req.as_bytes());
            stream.set_read_timeout(Some(StdDuration::from_secs(1))).ok();
            let mut buf = Vec::new();
            if stream.read_to_end(&mut buf).is_ok() {
                let body = String::from_utf8_lossy(&buf);
                let body = body.split("\r\n\r\n").nth(1).unwrap_or(&body);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                    if let Some(arr) = v.get("routes").and_then(|r| r.as_array()) {
                        self.routes = arr.iter().map(|r| {
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

    #[cfg(not(target_arch = "wasm32"))]
    fn check_proxy_alive(&mut self) {
        if let Some(ref mut h) = self.proxy_handle {
            if !h.is_alive() {
                let stderr = h.try_read_stderr();
                let status = h.wait_status();
                let code = status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
                let msg = if !stderr.trim().is_empty() {
                    format!("proxy exited (code {}): {}", code, stderr.trim())
                } else {
                    format!("proxy exited with code {}", code)
                };
                self.set_error(msg);
                self.proxy_handle = None;
                self.ws_connected = false;
            }
        }
    }
}
