mod app;
mod audio;
mod config;
mod contest;
mod messages;
mod station;
mod ui;

use app::ContestApp;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 320.0])
            .with_min_inner_size([400.0, 280.0]),
        ..Default::default()
    };

    eframe::run_native(
        "CW Contest Trainer",
        options,
        Box::new(|cc| Ok(Box::new(ContestApp::new(cc)))),
    )
}
