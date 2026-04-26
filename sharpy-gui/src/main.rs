#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod preview;
mod state;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Sharpy Demo"),
        ..Default::default()
    };

    eframe::run_native(
        "Sharpy Demo",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)) as Box<dyn eframe::App>)),
    )
}
