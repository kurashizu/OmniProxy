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

    pub(crate) fn set_error(&mut self, msg: impl Into<String>) {
        self.error_msg = Some((msg.into(), web_time::Instant::now()));
    }

    pub(crate) fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .resizable(false)
            .min_height(36.0)
            .frame(egui::Frame::none().inner_margin(egui::Margin { left: 12.0, right: 12.0, top: 8.0, bottom: 4.0 }))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let heading_color = if self.ws_connected && !self.tun_name.is_empty() {
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
                        let running = self.proxy_handle.as_mut().map_or(false, |h| h.is_alive());
                        if running {
                            if ui.add(egui::Button::new(
                                egui::RichText::new("⏹  Stop").size(13.0).color(egui::Color32::WHITE),
                            ).min_size(egui::vec2(80.0, 28.0))
                            .fill(egui::Color32::from_rgb(180, 60, 60))
                            ).clicked() {
                                if let Some(mut h) = self.proxy_handle.take() {
                                    h.stop();
                                }
                                self.ws_connected = false;
                            }
                        } else {
                            self.proxy_handle = None;
                            if ui.add(egui::Button::new(
                                egui::RichText::new("▶  Start").size(13.0).color(egui::Color32::WHITE),
                            ).min_size(egui::vec2(80.0, 28.0))
                            .fill(egui::Color32::from_rgb(60, 140, 60))
                            ).clicked() {
                                self.start_proxy();
                            }
                        }
                    });
                });
            });

        self.show_error_banner(ctx);
    }

    fn show_error_banner(&mut self, ctx: &egui::Context) {
        if let Some((ref msg, t)) = self.error_msg {
            if t.elapsed().as_secs() > 8 {
                self.error_msg = None;
                return;
            }
            let remaining = std::time::Duration::from_secs(8).saturating_sub(t.elapsed());
            ctx.request_repaint_after(remaining);
            let msg = msg.clone();
            egui::TopBottomPanel::top("error_banner")
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("⚠ {}", msg))
                                .size(13.0)
                                .color(egui::Color32::from_rgb(255, 100, 100)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                self.error_msg = None;
                            }
                        });
                    });
                });
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_proxy(&mut self) {
        let proxy_bin = self.exe_dir.join(&self.config.proxy);
        let config_path = self.exe_dir.join("config.yml");

        if !proxy_bin.exists() {
            self.set_error(format!("proxy binary not found: {}", proxy_bin.display()));
            return;
        }

        let handle = if cfg!(target_os = "windows") {
            crate::native::ProxyHandle::start(
                &proxy_bin.to_string_lossy(),
                &config_path.to_string_lossy(),
            )
        } else {
            crate::native::ProxyHandle::start_sudo(
                &proxy_bin.to_string_lossy(),
                &config_path.to_string_lossy(),
            )
        };

        match handle {
            Some(h) => {
                log::info!("proxy started (pid: {})", h.pid());
                self.proxy_handle = Some(h);
            }
            None => {
                self.set_error("failed to start proxy (need sudo/admin?)");
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn start_proxy(&mut self) {}
}
