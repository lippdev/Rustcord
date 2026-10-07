#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use std::time::Instant;
fn main() -> eframe::Result {
    let started = Instant::now();
    let smoke = std::env::args().any(|a| a == "--smoke-test");
    let metrics = std::env::args().any(|a| a == "--metrics");
    eframe::run_native(
        "Rustcord — mock",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 800.0])
                .with_min_inner_size([960.0, 600.0]),
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(ui::Rustcord::new(cc, started, smoke, metrics)))),
    )
}
