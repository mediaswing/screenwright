//! screenwright — a desktop screenplay writing app.
//!
//! Write in the [Fountain](https://fountain.io) plain-text format on the left
//! and see a live, industry-standard formatted preview on the right, with page
//! and scene statistics updated as you type.
//!
//! The screenplay engine (`parser`, `format`, `stats`) is plain `std` Rust;
//! only the UI layer depends on `eframe`/`egui`.

// Hide the console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod element;
mod export;
mod format;
mod gui;
mod parser;
mod stats;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 740.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title("screenwright"),
        ..Default::default()
    };

    eframe::run_native(
        "screenwright",
        options,
        Box::new(|cc| Ok(Box::new(gui::ScreenwrightApp::new(cc)))),
    )
}
