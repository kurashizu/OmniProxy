use eframe::egui;
use crate::app::DashboardApp;

impl DashboardApp {
    pub(crate) fn connections_page(&mut self, ui: &mut egui::Ui) {
        let count = self.connections_raw.len();
        ui.heading(format!("Connections ({})", count));

        ui.horizontal(|ui| {
            stat_pill(ui, "TCP", self.active_tcp, egui::Color32::from_rgb(60, 160, 220));
            ui.add_space(8.0);
            stat_pill(ui, "UDP", self.active_udp, egui::Color32::from_rgb(220, 160, 60));
            ui.add_space(8.0);
            stat_pill(ui, "ICMP", self.active_icmp, egui::Color32::from_rgb(160, 220, 80));
        });

        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let total_w = ui.available_width();
                let weights = [0.08, 0.12, 0.30, 0.35, 0.15];

                egui::Grid::new("conn_grid")
                    .striped(true)
                    .num_columns(5)
                    .min_col_width(40.0)
                    .show(ui, |ui| {
                        let headers = ["ID", "Protocol", "Source", "Target", "Duration"];
                        for (i, h) in headers.iter().enumerate() {
                            let w = total_w * weights[i];
                            ui.add(egui::Label::new(egui::RichText::new(*h).strong()).truncate());
                            if i < headers.len() - 1 {
                                ui.add_space(w - 40.0);
                            }
                        }
                        ui.end_row();

                        for c in &self.connections_raw {
                            let id = c.get("id").and_then(|v| v.as_u64()).map(|v| v.to_string()).unwrap_or_default();
                            let proto = c.get("protocol").and_then(|v| v.as_str()).unwrap_or("");
                            let src = c.get("source").and_then(|v| v.as_str()).unwrap_or("-");
                            let tgt = c.get("target").and_then(|v| v.as_str()).unwrap_or("");
                            let dur = c.get("duration_secs").and_then(|v| v.as_f64()).unwrap_or(0.0);

                            ui.label(&id);
                            ui.label(proto);
                            ui.add(egui::Label::new(src).truncate());
                            ui.add(egui::Label::new(tgt).truncate());
                            ui.label(format!("{:.1}s", dur));
                            ui.end_row();
                        }
                    });
            });
    }
}

fn stat_pill(ui: &mut egui::Ui, label: &str, value: usize, color: egui::Color32) {
    ui.label(
        egui::RichText::new(label).color(color).size(14.0).strong(),
    );
    ui.colored_label(color, format!("{}", value));
}
