use eframe::egui;
use crate::app::DashboardApp;
use crate::pages::Page;

impl DashboardApp {
    fn nav_button(&self, ui: &mut egui::Ui, page: Page, label: &str, width: f32) -> egui::Response {
        let selected = self.current_page == page;
        ui.add(
            egui::Button::new(
                egui::RichText::new(label).size(13.0)
                    .color(if selected { egui::Color32::WHITE } else { egui::Color32::GRAY }),
            )
            .min_size(egui::vec2(width, 28.0))
            .fill(if selected {
                egui::Color32::from_rgba_premultiplied(128, 128, 128, 30)
            } else {
                egui::Color32::TRANSPARENT
            }),
        )
    }

    pub(crate) fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .resizable(false)
            .min_height(36.0)
            .frame(egui::Frame::none().inner_margin(egui::Margin { left: 12.0, right: 12.0, top: 8.0, bottom: 4.0 }))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let heading_color = if self.ws_connected {
                        egui::Color32::GREEN
                    } else {
                        egui::Color32::from_rgb(180, 100, 100)
                    };
                    ui.heading(egui::RichText::new("OmniProxy").size(18.0).color(heading_color));
                    ui.add_space(8.0);
                    ui.separator();

                    if self.nav_button(ui, Page::Overview, "\u{1F3E0}  Overview", 130.0).clicked() {
                        self.current_page = Page::Overview;
                    }
                    if self.nav_button(ui, Page::Connections, "\u{1F517}  Connections", 130.0).clicked() {
                        self.current_page = Page::Connections;
                    }
                    if self.nav_button(ui, Page::Settings, "\u{2699}  Settings", 90.0).clicked() {
                        self.current_page = Page::Settings;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(egui::RichText::new("⏹  Stop").size(13.0)).min_size(egui::vec2(80.0, 28.0))).clicked() {
                        }
                    });
                });
            });
    }
}
