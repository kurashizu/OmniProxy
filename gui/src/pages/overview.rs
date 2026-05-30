use crate::app::DashboardApp;
use crate::app::RouteInfo;
use crate::widgets::{format_bytes, format_speed};
use eframe::egui;

fn kv_table(ui: &mut egui::Ui, _id: &str, rows: &[(&str, String)]) {
    let half = ui.available_width() / 2.0;
    for (k, v) in rows {
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(*k).sense(egui::Sense::hover()));
            ui.add_space(ui.available_width() - half);
            ui.add(egui::Label::new(v).sense(egui::Sense::hover()));
        });
        ui.add_space(4.0);
    }
}

fn route_table(ui: &mut egui::Ui, routes: &[RouteInfo]) {
    let def: Vec<&RouteInfo> = routes
        .iter()
        .filter(|r| r.destination == "0.0.0.0/0" || r.destination == "::/0")
        .collect();
    let v4 = def.iter().find(|r| r.destination == "0.0.0.0/0");
    let v6 = def.iter().find(|r| r.destination == "::/0");
    let w = ui.available_width() / 3.0;
    egui::Grid::new("route_grid")
        .striped(true)
        .min_col_width(w)
        .show(ui, |ui| {
            ui.strong("Dest");
            ui.strong("Gateway");
            ui.strong("Iface");
            ui.end_row();
            if let Some(r) = v4 {
                ui.label(&r.destination);
                ui.label(&r.gateway);
                ui.label(&r.interface);
            } else {
                ui.weak("0.0.0.0/0");
                ui.weak("-");
                ui.weak("-");
            }
            ui.end_row();
            if let Some(r) = v6 {
                ui.label(&r.destination);
                ui.label(&r.gateway);
                ui.label(&r.interface);
            } else {
                ui.weak("::/0");
                ui.weak("-");
                ui.weak("-");
            }
            ui.end_row();
        });
}

impl DashboardApp {
    pub(crate) fn overview_page(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |cols| {
            cols[0].heading("Process Status");
            cols[0].add_space(4.0);
            kv_table(
                &mut cols[0],
                "proc_grid",
                &[
                    (
                        "Proxy Uptime :",
                        format!(
                            "{:02}:{:02}:{:02}",
                            self.proxy_uptime as u64 / 3600,
                            (self.proxy_uptime as u64 % 3600) / 60,
                            self.proxy_uptime as u64 % 60
                        ),
                    ),
                    (
                        "Client Uptime :",
                        format!(
                            "{:02}:{:02}:{:02}",
                            self.client_uptime as u64 / 3600,
                            (self.client_uptime as u64 % 3600) / 60,
                            self.client_uptime as u64 % 60
                        ),
                    ),
                    ("Reconnects :", format!("{}", self.reconnect_count)),
                ],
            );

            cols[0].separator();
            cols[0].heading("Default Routes");
            cols[0].add_space(4.0);
            route_table(&mut cols[0], &self.routes);

            cols[0].separator();
            cols[0].heading("Traffic Stats");
            cols[0].add_space(4.0);
            kv_table(
                &mut cols[0],
                "traffic_grid",
                &[
                    ("Bytes Sent :", format_bytes(self.bytes_tx)),
                    ("Bytes Received :", format_bytes(self.bytes_rx)),
                    ("Upload Speed :", format_speed(self.speed_tx)),
                    ("Download Speed :", format_speed(self.speed_rx)),
                ],
            );

            cols[1].heading("TUN Status");
            cols[1].add_space(4.0);
            let dev = if self.tun_name.is_empty() {
                self.config.tun_name.clone()
            } else {
                self.tun_name.clone()
            };
            let ip4 = if self.tun_ip.is_empty() {
                format!("{}/{}", self.config.tun_ip, self.config.tun_prefix)
            } else {
                self.tun_ip.clone()
            };
            kv_table(
                &mut cols[1],
                "tun_grid",
                &[
                    ("Device :", if dev.is_empty() { "-".into() } else { dev }),
                    ("IPv4 :", if ip4.is_empty() { "-".into() } else { ip4 }),
                    ("IPv4 Gateway :", self.config.tun_gw.clone()),
                    (
                        "IPv6 :",
                        format!("{}/{}", self.config.tun_ip6, self.config.tun_prefix6),
                    ),
                    ("IPv6 Gateway :", self.config.tun_gw6.clone()),
                ],
            );

            cols[1].separator();
            cols[1].heading("Server Info");
            cols[1].add_space(4.0);
            kv_table(
                &mut cols[1],
                "server_grid",
                &[
                    (
                        "Server :",
                        if self.server.is_empty() {
                            "-".into()
                        } else {
                            self.server.clone()
                        },
                    ),
                    (
                        "SOCKS5 :",
                        if self.socks5.is_empty() {
                            "-".into()
                        } else {
                            self.socks5.clone()
                        },
                    ),
                ],
            );
        });
    }
}
