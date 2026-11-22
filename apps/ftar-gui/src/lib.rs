//! ftar-gui: egui front end for ftar-core.
//!
//! * [`App`] holds the state and every user operation (headless, testable)
//! * `ui` draws it with egui; `layout` (from ftar-core) places the tree

mod app;
mod help;
pub use ftar_core::layout;
pub use help::HelpState;
mod ui;

use std::path::PathBuf;

pub use app::{ctime_text, App, Pending, Tab};
use eframe::egui;

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
            if let Some(p) = path {
                app.open(p);
            }
            Ok(Box::new(app))
        }),
    )
}
