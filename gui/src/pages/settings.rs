use eframe::egui;
use crate::app::DashboardApp;

fn kv_edit(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    let mut changed = false;
    let half = ui.available_width() / 2.0;
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(label).sense(egui::Sense::hover()));
        ui.add_space(ui.available_width() - half);
        changed = ui.add_sized(
            egui::vec2(half - 24.0, 20.0),
            egui::TextEdit::singleline(value),
        ).changed();
    });
    ui.add_space(4.0);
    changed
}

fn kv_edit_u16(ui: &mut egui::Ui, label: &str, value: &mut u16) -> bool {
    let mut buf = value.to_string();
    let mut changed = false;
    let half = ui.available_width() / 2.0;
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(label).sense(egui::Sense::hover()));
        ui.add_space(ui.available_width() - half);
        if ui.add_sized(
            egui::vec2(half - 24.0, 20.0),
            egui::TextEdit::singleline(&mut buf),
        ).changed() {
            if let Ok(n) = buf.parse::<u16>() {
                *value = n;
                changed = true;
            }
        }
    });
    ui.add_space(4.0);
    changed
}

fn kv_edit_u8(ui: &mut egui::Ui, label: &str, value: &mut u8) -> bool {
    let mut buf = value.to_string();
    let mut changed = false;
    let half = ui.available_width() / 2.0;
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(label).sense(egui::Sense::hover()));
        ui.add_space(ui.available_width() - half);
        if ui.add_sized(
            egui::vec2(half - 24.0, 20.0),
            egui::TextEdit::singleline(&mut buf),
        ).changed() {
            if let Ok(n) = buf.parse::<u8>() {
                *value = n;
                changed = true;
            }
        }
    });
    ui.add_space(4.0);
    changed
}

fn kv_edit_opt(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) -> bool {
    let mut buf = value.clone().unwrap_or_default();
    let mut changed = false;
    let half = ui.available_width() / 2.0;
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(label).sense(egui::Sense::hover()));
        ui.add_space(ui.available_width() - half);
        if ui.add_sized(
            egui::vec2(half - 24.0, 20.0),
            egui::TextEdit::singleline(&mut buf),
        ).changed() {
            *value = if buf.is_empty() { None } else { Some(buf) };
            changed = true;
        }
    });
    ui.add_space(4.0);
    changed
}

impl DashboardApp {
    pub(crate) fn save_config(&self) {
        let path = self.exe_dir.join("config.yml");
        if let Ok(yaml) = serde_yaml::to_string(&self.config) {
            let _ = std::fs::write(&path, &yaml);
        }
    }

    pub(crate) fn settings_page(&mut self, ui: &mut egui::Ui) {
        let mut dirty = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Basic Settings");
            ui.add_space(4.0);
            dirty |= kv_edit(ui, "Server :", &mut self.config.server);
            ui.horizontal(|ui| {
                let half = ui.available_width() / 2.0;
                ui.add(egui::Label::new("Token :").sense(egui::Sense::hover()));
                ui.add_space(ui.available_width() - half);
                let btn_w = 44.0;
                let gap = 4.0;
                let input_w = half - 24.0 - btn_w - gap;
                let te = if self.show_token {
                    egui::TextEdit::singleline(&mut self.config.token)
                } else {
                    egui::TextEdit::singleline(&mut self.config.token).password(true)
                };
                dirty |= ui.add_sized(egui::vec2(input_w, 20.0), te).changed();
                ui.add_space(gap);
                if ui.add_sized(egui::vec2(btn_w, 20.0), egui::Button::new(if self.show_token { "Hide" } else { "Show" })).clicked() {
                    self.show_token = !self.show_token;
                }
            });
            ui.add_space(4.0);

            ui.separator();
            ui.heading("Advanced Settings");
            ui.add_space(4.0);
            dirty |= kv_edit(ui, "Client Path :", &mut self.config.client);
            dirty |= kv_edit(ui, "Proxy Path :", &mut self.config.proxy);
            dirty |= kv_edit(ui, "SOCKS Address :", &mut self.config.socks_addr);
            dirty |= kv_edit_u16(ui, "SOCKS Port :", &mut self.config.socks_port);
            dirty |= kv_edit_u16(ui, "Admin Port :", &mut self.config.admin_port);
            dirty |= kv_edit(ui, "TUN Name :", &mut self.config.tun_name);
            dirty |= kv_edit(ui, "TUN IP :", &mut self.config.tun_ip);
            dirty |= kv_edit_u8(ui, "TUN Prefix :", &mut self.config.tun_prefix);
            dirty |= kv_edit(ui, "TUN Gateway :", &mut self.config.tun_gw);
            dirty |= kv_edit(ui, "TUN IPv6 :", &mut self.config.tun_ip6);
            dirty |= kv_edit_u8(ui, "TUN Prefix6 :", &mut self.config.tun_prefix6);
            dirty |= kv_edit(ui, "TUN Gateway6 :", &mut self.config.tun_gw6);
            dirty |= kv_edit_opt(ui, "Socks Outbound IP :", &mut self.config.socks_outbound_ip);
            ui.add_space(4.0);

            ui.separator();
            ui.heading("About");
            ui.add_space(4.0);
            let half = ui.available_width() / 2.0;
            ui.horizontal(|ui| {
                ui.add(egui::Label::new("Version :").sense(egui::Sense::hover()));
                ui.add_space(ui.available_width() - half);
                ui.add(egui::Label::new(env!("CARGO_PKG_VERSION")).sense(egui::Sense::hover()));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add(egui::Label::new("GitHub :").sense(egui::Sense::hover()));
                ui.add_space(ui.available_width() - half);
                ui.hyperlink_to("github.com/kurashizu/OmniProxy", "https://github.com/kurashizu/OmniProxy");
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add(egui::Label::new("Author :").sense(egui::Sense::hover()));
                ui.add_space(ui.available_width() - half);
                ui.hyperlink_to("kurashizu", "https://blog.022025.xyz/");
            });
        });
        if dirty {
            self.dirty = true;
            self.dirty_at = Some(web_time::Instant::now());
        }
    }
}
