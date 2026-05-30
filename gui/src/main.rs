#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use eframe::egui;

#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::JsCast;

mod app;
mod config;
mod native;
mod pages;
mod paths;
mod widgets;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let wgpu_options = eframe::egui_wgpu::WgpuConfiguration {
            #[cfg(target_os = "windows")]
            supported_backends: eframe::wgpu::Backends::DX12,
            #[cfg(not(target_os = "windows"))]
            supported_backends: eframe::wgpu::Backends::all(),
            present_mode: eframe::wgpu::PresentMode::AutoVsync,
            ..Default::default()
        };

        eframe::run_native(
            "OmniProxy",
            eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 500.0]),
                renderer: eframe::Renderer::Wgpu,
                wgpu_options,
                ..Default::default()
            },
            Box::new(|cc| Ok(Box::new(app::DashboardApp::new(cc)))),
        )
        .expect("failed to start eframe");
    }

    #[cfg(target_arch = "wasm32")]
    {
        eframe::WebLogger::init(log::LevelFilter::Debug).ok();
        wasm_bindgen_futures::spawn_local(async {
            let document = eframe::web_sys::window().unwrap().document().unwrap();
            let canvas: eframe::web_sys::HtmlCanvasElement = document
                .get_element_by_id("the_canvas_id")
                .unwrap()
                .dyn_into()
                .unwrap();
            eframe::WebRunner::new()
                .start(
                    canvas,
                    eframe::WebOptions::default(),
                    Box::new(|cc| Ok(Box::new(app::DashboardApp::new(cc)))),
                )
                .await
                .expect("failed to start eframe");
        });
    }
}
