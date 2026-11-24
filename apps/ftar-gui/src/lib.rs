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
    install_panic_hook();
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

/// Release builds have no console, so a panic would otherwise close the
/// window silently. Write the details to a crash log and tell the user.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let log = std::env::temp_dir().join("ftar-crash.log");
        let text = format!(
            "ftar {} crashed:
{info}

{}
",
            env!("CARGO_PKG_VERSION"),
            std::backtrace::Backtrace::force_capture()
        );
        let saved = std::fs::write(&log, &text).is_ok();
        let note = if saved {
            format!("Details were saved to {}.", log.display())
        } else {
            "The crash details could not be saved.".to_string()
        };
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("ftar crashed")
            .set_description(format!(
                "ftar hit an unexpected error and must close.

{info}

{note}"
            ))
            .show();
    }));
}
