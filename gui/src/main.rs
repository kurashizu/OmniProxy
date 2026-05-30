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

        unsafe extern "system" {
            fn MessageBoxW(hWnd: *const core::ffi::c_void, lpText: *const u16, lpCaption: *const u16, uType: u32) -> i32;
        }
        let wide: Vec<u16> = full.encode_utf16().chain(std::iter::once(0)).collect();
        let title: Vec<u16> = "OmniProxy Error\0".encode_utf16().collect();
        unsafe {
            MessageBoxW(core::ptr::null(), wide.as_ptr(), title.as_ptr(), 0x00000010);
        }
    }));
}

fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../icon.png");
    let image = image::load_from_memory(bytes)
        .expect("failed to load icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

#[cfg(target_os = "linux")]
fn ensure_root() {
    if unsafe { libc::getuid() } == 0 {
        return;
    }

    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => {
            eprintln!("this program requires root privileges");
            std::process::exit(1);
        }
    };

    let args: Vec<String> = std::env::args().collect();

    if let Ok(output) = std::process::Command::new("which")
        .arg("pkexec")
        .output()
    {
        if output.status.success() {
            let status = std::process::Command::new("pkexec")
                .arg(&exe)
                .args(&args)
                .status();

            match status {
                Ok(s) => std::process::exit(s.code().unwrap_or(1)),
                Err(_) => {}
            }
        }
    }

    let msg = format!("requires root privileges\n\nPlease run:\nsudo {}", exe.display());
    let _ = std::process::Command::new("zenity")
        .args(["--error", "--title=OmniProxy", &format!("--text={}", msg)])
        .status()
        .or_else(|_| {
            std::process::Command::new("kdialog")
                .args(["--error", &msg, "--title", "OmniProxy"])
                .status()
        });

    eprintln!("this program requires root privileges");
    eprintln!("  sudo {}", exe.display());
    std::process::exit(1);
}

fn main() {
    #[cfg(all(not(debug_assertions), target_os = "windows"))]
    set_panic_hook();

    #[cfg(target_os = "linux")]
    ensure_root();

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
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([800.0, 500.0])
                    .with_resizable(false)
                    .with_maximize_button(false)
                    .with_icon(load_icon()),
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
