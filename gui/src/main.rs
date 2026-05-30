use eframe::egui;

#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::JsCast;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let native_options = eframe::NativeOptions::default();
        eframe::run_native(
            "OmniProxy Dashboard",
            native_options,
            Box::new(|_cc| Ok(Box::new(DashboardApp::default()))),
        )
        .expect("failed to start eframe");
    }

    #[cfg(target_arch = "wasm32")]
    {
        eframe::WebLogger::init(log::LevelFilter::Debug).ok();
        wasm_bindgen_futures::spawn_local(async {
            let web_options = eframe::WebOptions::default();
            let document = eframe::web_sys::window().unwrap().document().unwrap();
            let canvas = document.get_element_by_id("the_canvas_id").unwrap();
            let canvas: eframe::web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
            eframe::WebRunner::new()
                .start(
                    canvas,
                    web_options,
                    Box::new(|_cc| Ok(Box::new(DashboardApp::default()))),
                )
                .await
                .expect("failed to start eframe");
        });
    }
}

struct Connection {
    id: u32,
    protocol: String,
    source: String,
    destination: String,
    bytes_sent: u64,
    bytes_recv: u64,
    status: String,
}

struct DashboardApp {
    connections: Vec<Connection>,
    next_id: u32,
    server_addr: String,
    proxy_port: String,
    stats_total_conns: u64,
    stats_active_conns: u32,
    stats_total_bytes: u64,
    stats_uptime_secs: u64,
    auto_refresh: bool,
}

impl Default for DashboardApp {
    fn default() -> Self {
        let mut app = Self {
            connections: Vec::new(),
            next_id: 1,
            server_addr: "wss://relay.omniproxy.example".to_owned(),
            proxy_port: "1080".to_owned(),
            stats_total_conns: 0,
            stats_active_conns: 0,
            stats_total_bytes: 0,
            stats_uptime_secs: 0,
            auto_refresh: true,
        };
        for _ in 0..4 {
            app.add_demo_connection();
        }
        app
    }
}

impl DashboardApp {
    fn add_demo_connection(&mut self) {
        let id = self.next_id;
        self.next_id += 1;
        let protocols = ["TCP", "UDP", "ICMP"];
        let statuses = ["established", "established", "closed", "establishing"];
        self.connections.push(Connection {
            id,
            protocol: protocols[id as usize % protocols.len()].to_owned(),
            source: format!("10.0.0.{}:{}", (id % 20) + 1, 40000 + id),
            destination: format!(
                "{}.{}.{}.{}:{}",
                (id % 223) + 1,
                (id * 3 % 255),
                (id * 7 % 255),
                (id * 11 % 255),
                80 + (id % 4) * 100
            ),
            bytes_sent: (id as u64 * 1234) % 99999,
            bytes_recv: (id as u64 * 5678) % 999999,
            status: statuses[id as usize % statuses.len()].to_owned(),
        });
        self.stats_total_conns += 1;
        self.stats_active_conns += 1;
    }
}

impl eframe::App for DashboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.auto_refresh {
            ctx.request_repaint();
            self.stats_uptime_secs += 1;
        }

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("OmniProxy");
                ui.separator();
                ui.label("Proxy & Relay Dashboard");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let color = if self.stats_active_conns > 0 {
                        egui::Color32::GREEN
                    } else {
                        egui::Color32::GRAY
                    };
                    ui.colored_label(color, "\u{25cf}");
                    ui.label("Connected");
                });
            });
        });

        egui::SidePanel::left("nav_panel")
            .resizable(false)
            .min_width(180.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Navigation");
                });
                ui.separator();
                ui.selectable_label(true, "Dashboard").clicked();
                ui.selectable_label(false, "Connections").clicked();
                ui.selectable_label(false, "Settings").clicked();
                ui.selectable_label(false, "Logs").clicked();
                ui.separator();
                if ui.button("+ New Connection").clicked() {
                    // placeholder
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.stats_section(ui);
                ui.separator();
                self.connections_section(ui);
            });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Server: {}", self.server_addr));
                ui.separator();
                ui.label(format!("Proxy Port: {}", self.proxy_port));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.auto_refresh, "Auto-refresh");
                    if ui.button("Refresh").clicked() {
                        // placeholder
                    }
                });
            });
        });
    }
}

impl DashboardApp {
    fn stats_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Statistics");
        egui::Grid::new("stats_grid")
            .striped(true)
            .min_col_width(120.0)
            .show(ui, |ui| {
                ui.label("Total Connections:");
                ui.label(format!("{}", self.stats_total_conns));
                ui.end_row();

                ui.label("Active Connections:");
                ui.label(format!("{}", self.stats_active_conns));
                ui.end_row();

                ui.label("Total Data Transferred:");
                let total = self.stats_total_bytes;
                if total > 1_000_000 {
                    ui.label(format!("{:.2} MB", total as f64 / 1_000_000.0));
                } else if total > 1_000 {
                    ui.label(format!("{:.2} KB", total as f64 / 1_000.0));
                } else {
                    ui.label(format!("{} B", total));
                }
                ui.end_row();

                ui.label("Uptime:");
                let h = self.stats_uptime_secs / 3600;
                let m = (self.stats_uptime_secs % 3600) / 60;
                let s = self.stats_uptime_secs % 60;
                ui.label(format!("{:02}:{:02}:{:02}", h, m, s));
                ui.end_row();
            });
    }

    fn connections_section(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Active Connections");
            if ui.button("Add Demo").clicked() {
                self.add_demo_connection();
            }
        });

        egui::ScrollArea::vertical()
            .max_height(400.0)
            .show(ui, |ui| {
                egui::Grid::new("conn_grid")
                    .striped(true)
                    .min_col_width(80.0)
                    .show(ui, |ui| {
                        ui.strong("ID");
                        ui.strong("Protocol");
                        ui.strong("Source");
                        ui.strong("Destination");
                        ui.strong("Sent");
                        ui.strong("Recv");
                        ui.strong("Status");
                        ui.strong("");
                        ui.end_row();

                        for conn in &self.connections {
                            ui.label(format!("{}", conn.id));
                            ui.label(&conn.protocol);
                            ui.label(&conn.source);
                            ui.label(&conn.destination);
                            ui.label(format!("{}", conn.bytes_sent));
                            ui.label(format!("{}", conn.bytes_recv));

                            let (color, text) = match conn.status.as_str() {
                                "established" => (egui::Color32::GREEN, "Established"),
                                "establishing" => (egui::Color32::YELLOW, "Establishing"),
                                "closed" => (egui::Color32::GRAY, "Closed"),
                                _ => (egui::Color32::WHITE, conn.status.as_str()),
                            };
                            ui.colored_label(color, text);
                            ui.label("\u{2715}");
                            ui.end_row();
                        }
                    });
            });
    }
}
