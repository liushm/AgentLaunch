#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fonts;
mod theme;

use app::AgentLaunchApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([840.0, 560.0])
            .with_min_inner_size([640.0, 420.0])
            .with_title("AgentLaunch"),
        ..Default::default()
    };

    eframe::run_native(
        "AgentLaunch",
        options,
        Box::new(|cc| Ok(Box::new(AgentLaunchApp::new(cc)))),
    )
}
