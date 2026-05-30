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

#[cfg(all(not(debug_assertions), target_os = "windows"))]
fn set_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = if let Some(s) = info.payload_as_str() {
            s.to_string()
        } else {
            "Unknown panic".to_string()
        };
        let location = info.location().map(|l| {
            format!("\n\nAt: {}:{}", l.file(), l.line())
        }).unwrap_or_default();
        let full = format!("OmniProxy crashed:{msg}{location}");

        extern "system" {
            fn MessageBoxW(hWnd: *const core::ffi::c_void, lpText: *const u16, lpCaption: *const u16, uType: u32) -> i32;
        }
        let wide: Vec<u16> = full.encode_utf16().chain(std::iter::once(0)).collect();
        let title: Vec<u16> = "OmniProxy Error\0".encode_utf16().collect();
        unsafe {
            MessageBoxW(core::ptr::null(), wide.as_ptr(), title.as_ptr(), 0x00000010);
        }
    }));
}

fn main() {
    #[cfg(all(not(debug_assertions), target_os = "windows"))]
    set_panic_hook();

    #[cfg(not(target_arch = "wasm32"))]
    {
        let wgpu_options = eframe::egui_wgpu::WgpuConfiguration {
            #[cfg(target_os = "windows")]
            supported_backends: eframe::wgpu::Backends::DX12
                | eframe::wgpu::Backends::VULKAN
                | eframe::wgpu::Backends::GL,
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
