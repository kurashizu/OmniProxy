use eframe::egui;
use crate::app::DashboardApp;

impl eframe::App for DashboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl DashboardApp {
    pub(crate) fn ui(&mut self, ctx: &egui::Context) {
        // Non-blocking: drain all pending poll results
        while let Ok(result) = self.stats_rx.try_recv() {
            self.apply_poll_result(result);
        }

        // Debounced config save (500ms after last change)
        if let Some(t) = self.dirty_at {
            if t.elapsed().as_millis() > 500 && self.dirty {
                self.save_config();
                self.dirty = false;
                self.dirty_at = None;
            }
        }

        self.check_proxy_alive();
        self.top_bar(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(8.0, 8.0)))
            .show(ctx, |ui| self.show_page(ui));

        // Ensure egui repaints even when idle
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    fn apply_poll_result(&mut self, r: crate::app::PollResult) {
        let dt = 0.5_f64.max(0.001);
        let cur_tx = r.bytes_tx;
        let cur_rx = r.bytes_rx;
        self.speed_tx = (cur_tx as f64 - self.prev_bytes_tx as f64) / dt;
        self.speed_rx = (cur_rx as f64 - self.prev_bytes_rx as f64) / dt;
        self.prev_bytes_tx = cur_tx;
        self.prev_bytes_rx = cur_rx;
        self.bytes_tx = cur_tx;
        self.bytes_rx = cur_rx;

        self.ws_connected = r.ws_connected;
        self.client_uptime = r.client_uptime;
        self.proxy_uptime = r.proxy_uptime;
        self.reconnect_count = r.reconnect_count;
        self.active_tcp = r.active_tcp;
        self.active_udp = r.active_udp;
        self.active_icmp = r.active_icmp;
        self.socks5 = r.socks5;
        self.server = r.server;
        self.tun_name = r.tun_name;
        self.tun_ip = r.tun_ip;
        self.routes = r.routes;
        self.connections_raw = r.connections_raw;
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
