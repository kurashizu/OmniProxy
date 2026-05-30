use eframe::egui;

#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::JsCast;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 400.0]),
            ..Default::default()
        };
        eframe::run_native(
            "OmniProxy Dashboard",
            native_options,
            Box::new(|cc| Ok(Box::new(DashboardApp::new(cc)))),
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
                    Box::new(|cc| Ok(Box::new(DashboardApp::new(cc)))),
                )
                .await
                .expect("failed to start eframe");
        });
    }
}

struct DashboardApp {
}

impl DashboardApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_pixels_per_point(1.0);
        Self {}
    }
}

impl eframe::App for DashboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.heading("OmniProxy Dashboard");
            });
        });
    }
}
