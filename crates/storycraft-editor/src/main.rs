//! StoryCraft: an open-source e-learning authoring tool.

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod canvas;
mod draw;
mod history;
mod panels;
mod preview;
mod textures;
mod widgets;

fn main() -> eframe::Result {
    let initial = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("StoryCraft")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("StoryCraft", options, Box::new(move |_cc| Ok(Box::new(app::EditorApp::new(initial)))))
}
