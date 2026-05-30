use eframe::egui;

pub(crate) fn cfg_row(ui: &mut egui::Ui, label: &str, input_width: f32, edit: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(egui::RichText::new(label).color(egui::Color32::GRAY)).sense(egui::Sense::hover()));
        ui.add_space(ui.available_width() - input_width);
        edit(ui);
    });
}

pub(crate) fn format_bytes(b: u64) -> String {
    if b > 1_000_000_000_000 {
        format!("{:.2} TB", b as f64 / 1_000_000_000_000.0)
    } else if b > 1_000_000_000 {
        format!("{:.2} GB", b as f64 / 1_000_000_000.0)
    } else if b > 1_000_000 {
        format!("{:.2} MB", b as f64 / 1_000_000.0)
    } else if b > 1_000 {
        format!("{:.2} KB", b as f64 / 1_000.0)
    } else {
        format!("{b} B")
    }
}

pub(crate) fn format_speed(bps: f64) -> String {
    if bps > 1_000_000_000_000.0 {
        format!("{:.2} TB/s", bps / 1_000_000_000_000.0)
    } else if bps > 1_000_000_000.0 {
        format!("{:.2} GB/s", bps / 1_000_000_000.0)
    } else if bps > 1_000_000.0 {
        format!("{:.2} MB/s", bps / 1_000_000.0)
    } else if bps > 1_000.0 {
        format!("{:.2} KB/s", bps / 1_000.0)
    } else {
        format!("{:.1} B/s", bps)
    }
}
