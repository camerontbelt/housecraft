//! HouseCraft desktop (and web) application.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod canvas;
mod dialogs;
mod icons;
mod props;
mod snap;
mod tools;
mod util;
mod view3d;

/// MSAA sample count shared by eframe and the 3D pipeline (they must match).
pub const MSAA: u32 = 4;
/// Depth bits requested from eframe; the 3D pipeline uses the matching format.
pub const DEPTH_BITS: u8 = 24;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let path = std::env::args().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("HouseCraft")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([800.0, 500.0]),
        multisampling: MSAA as u16,
        depth_buffer: DEPTH_BITS,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "HouseCraft",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, path)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast as _;
    let options = eframe::WebOptions {
        depth_buffer: DEPTH_BITS,
        ..Default::default()
    };
    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("housecraft"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("a <canvas id=\"housecraft\"> element");
        eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(|cc| Ok(Box::new(app::App::new(cc, None)))),
            )
            .await
            .expect("failed to start HouseCraft");
    });
}
