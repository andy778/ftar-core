//! ftar-gui: egui front end for ftar-core.
//!
//! * [`App`] holds the state and every user operation (headless, testable)
//! * `ui` draws it with egui; the tree drawing comes from `ftar_render::scene`

mod app;
mod help;
pub use ftar_core::layout;
pub use help::HelpState;
mod ped;
mod prefs_ui;
mod preview;
mod toolbar;
mod ui;

use std::path::PathBuf;

pub use app::{ctime_text, App, Clip, Pending, Tab};
use eframe::egui;
pub use ped::PedWindow;
pub use preview::PrintPreview;
pub use ui::browser_command;

/// Start the desktop app, optionally opening `path`.
pub fn run(path: Option<PathBuf>) -> eframe::Result {
    eframe::run_native(
        "ftar - fault tree analysis",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
            ..Default::default()
        },
        Box::new(move |_cc| {
            let mut app = App::default();
            if let Some(p) = ftar_render::Preferences::default_path() {
                app.use_prefs_file(p);
            }
            if let Some(p) = path {
                app.open(p);
            }
            Ok(Box::new(app))
        }),
    )
}
