use crate::app::DashboardApp;
use eframe::egui;

impl DashboardApp {
    pub(crate) fn logs_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Logs");
        ui.add_space(4.0);

        let count = self.log_lines.len();
        ui.label(format!("{} entries", count));
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.button("Clear").clicked() {
                self.log_lines.clear();
            }
        });
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.log_lines {
                    let color = if line.contains("[ERROR]") || line.contains("error") {
                        egui::Color32::from_rgb(255, 100, 100)
                    } else if line.contains("[WARN]") || line.contains("warn") {
                        egui::Color32::from_rgb(255, 200, 80)
                    } else {
                        egui::Color32::from_rgb(200, 200, 200)
                    };
                    ui.label(egui::RichText::new(line).size(12.0).color(color));
                }
            });
    }
}
