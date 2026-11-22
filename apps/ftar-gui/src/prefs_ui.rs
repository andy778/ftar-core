//! Edit > Preferences window: monochrome mode and the drawing colours.

use eframe::egui;
use ftar_render::{Preferences, Symbol};

use crate::app::App;

fn edit_colour(ui: &mut egui::Ui, label: &str, c: &mut [u8; 3]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let r = ui.color_edit_button_srgb(c);
        changed = r.changed();
        // the swatch is the control; name it for assistive tools
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ColorWell, true, label));
        ui.label(label);
    });
    changed
}

impl App {
    pub(crate) fn prefs_window(&mut self, ctx: &egui::Context) {
        if !self.prefs_open {
            return;
        }
        let mut open = true;
        let mut p = self.prefs.clone();
        egui::Window::new("Preferences")
            .open(&mut open)
            .default_size([360.0, 480.0])
            .show(ctx, |ui| {
                ui.group(|ui| {
                    ui.strong("View");
                    ui.radio_value(&mut p.monochrome, false, "Colour");
                    ui.radio_value(&mut p.monochrome, true, "Monochrome (for printing)");
                });
                ui.add_enabled_ui(!p.monochrome, |ui| {
                    ui.group(|ui| {
                        ui.strong("Pen colours");
                        edit_colour(ui, "Pen", &mut p.pen);
                        edit_colour(ui, "Background", &mut p.background);
                        edit_colour(ui, "Highlight", &mut p.highlight);
                    });
                    ui.group(|ui| {
                        ui.strong("Symbol colours");
                        for s in Symbol::ALL {
                            let mut c = p.fill_of(s);
                            if edit_colour(ui, s.label(), &mut c) {
                                p.set_fill(s, c);
                            }
                        }
                    });
                });
                if ui.button("Reset to defaults").clicked() {
                    p = Preferences {
                        monochrome: p.monochrome,
                        ..Preferences::default()
                    };
                }
            });
        if p != self.prefs {
            self.set_prefs(p);
        }
        if !open {
            self.prefs_open = false;
        }
    }
}
