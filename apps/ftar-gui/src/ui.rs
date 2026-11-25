//! egui front end: drawing and widgets only; all behaviour is in [`crate::App`].

use eframe::egui::{
    self, Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, Sense, Shape, Stroke, Vec2,
    ViewportCommand,
};
use ftar_core::montecarlo::Method;
use ftar_core::report::ReportKind;
use ftar_core::EventType;

use crate::app::{App, Pending, Tab};
use ftar_render::scene::{tree_scene, triangulate, Anchor, Prim, Rgb, Scene, SceneOptions};
use ftar_render::Preferences;

const GATES: &[EventType] = &[
    EventType::And,
    EventType::Or,
    EventType::Xor,
    EventType::PriorityAnd,
    EventType::Inhibit,
];
const EVENTS: &[EventType] = &[
    EventType::Basic,
    EventType::Undeveloped,
    EventType::External,
    EventType::Dormant,
    EventType::Intermediate,
    EventType::ConditionAnalysed,
    EventType::ConditionNotAnalysed,
    EventType::TransferIn,
    EventType::TransferOut,
];

fn accepts_children(k: EventType) -> bool {
    k.is_gate() || k == EventType::Intermediate
}

/// Shortcuts the menu bar acts on after drawing its menus.
#[allow(clippy::struct_excessive_bools)]
struct BarKeys {
    save: bool,
    undo: bool,
    redo: bool,
    del: bool,
    new: bool,
    open: bool,
    print: bool,
}

fn shortcut(ctx: &egui::Context, k: Key) -> String {
    ctx.format_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, k))
}

/// Cut, copy, paste and link. They act on the tree unless a text field has the keyboard;
/// backends send them as clipboard events, tests as key presses.
fn clipboard_keys(ctx: &egui::Context) -> (bool, bool, bool, bool) {
    if ctx.memory(|m| m.focused().is_some()) {
        return (false, false, false, false);
    }
    ctx.input_mut(|i| {
        let mut c = (
            i.consume_key(Modifiers::COMMAND, Key::X),
            i.consume_key(Modifiers::COMMAND, Key::C),
            i.consume_key(Modifiers::COMMAND, Key::V),
            i.consume_key(Modifiers::COMMAND, Key::I),
        );
        i.events.retain(|e| match e {
            egui::Event::Cut => {
                c.0 = true;
                false
            }
            egui::Event::Copy => {
                c.1 = true;
                false
            }
            egui::Event::Paste(_) => {
                c.2 = true;
                false
            }
            _ => true,
        });
        c
    })
}

/// OpenFTA's other shortcuts; a focused text field keeps Ctrl+A and the rest for itself.
fn openfta_keys(ctx: &egui::Context) -> (bool, bool, bool, bool) {
    if ctx.egui_wants_keyboard_input() {
        return (false, false, false, false);
    }
    ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::COMMAND, Key::A),
            i.consume_key(Modifiers::COMMAND, Key::D),
            i.consume_key(Modifiers::COMMAND, Key::L),
            i.consume_key(Modifiers::COMMAND, Key::R),
        )
    })
}

impl App {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let keys = self.handle_shortcuts(&ctx);
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let has_doc = self.doc.is_some();
                ui.menu_button("File", |ui| self.file_menu(ui, has_doc));
                ui.menu_button("Edit", |ui| self.edit_menu(ui, has_doc));
                ui.menu_button("View", |ui| self.view_menu(ui, has_doc));
                ui.menu_button("Analysis", |ui| self.analysis_menu(ui, has_doc));
                ui.menu_button("Report", |ui| self.report_menu(ui));
                ui.menu_button("Help", |ui| self.help_menu(ui));
                ui.separator();
                if keys.undo && self.history.can_undo() {
                    self.undo();
                }
                if keys.redo && self.history.can_redo() {
                    self.redo();
                }
                ui.label(self.title());
                if has_doc {
                    self.database_field(ui);
                }
                if keys.save && has_doc {
                    self.save_or_save_as();
                }
                if keys.new {
                    self.request_new();
                }
                if keys.open {
                    self.open_dialog();
                }
                if keys.print && has_doc {
                    self.print_now();
                }
                if keys.del && self.selected.as_ref().is_some_and(|p| !p.is_empty()) {
                    self.delete_selected();
                }
            });
        });
    }

    /// Keyboard shortcuts. Most act at once; the ones the menu bar acts on after drawing its
    /// menus are returned, so they still run in that order.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) -> BarKeys {
        let (zoom_in, zoom_out) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::Plus)
                    || i.consume_key(Modifiers::COMMAND, Key::Equals),
                i.consume_key(Modifiers::COMMAND, Key::Minus),
            )
        });
        if self.doc.is_some() {
            if zoom_in {
                self.zoom_in();
            } else if zoom_out {
                self.zoom_out();
            }
        }
        let keys = ctx.input_mut(|i| BarKeys {
            save: i.consume_key(Modifiers::COMMAND, Key::S),
            undo: i.consume_key(Modifiers::COMMAND, Key::Z),
            redo: i.consume_key(Modifiers::COMMAND, Key::Y),
            del: i.consume_key(Modifiers::NONE, Key::Delete),
            new: i.consume_key(Modifiers::COMMAND, Key::N),
            open: i.consume_key(Modifiers::COMMAND, Key::O),
            print: i.consume_key(Modifiers::COMMAND, Key::P),
        });
        let (cut, copy, paste, link) = clipboard_keys(ctx);
        let (save_as, delete_key, shift_left, shift_right) = openfta_keys(ctx);
        let quit_key = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Q));
        let can_shift_now = self.selected.as_ref().is_some_and(|p| !p.is_empty());
        if save_as && self.doc.is_some() {
            self.save_as_dialog();
        }
        if delete_key && can_shift_now {
            self.delete_selected();
        }
        if shift_left && can_shift_now {
            self.move_selected(-1);
        }
        if shift_right && can_shift_now {
            self.move_selected(1);
        }
        if quit_key {
            self.request_quit();
        }
        if cut {
            self.cut_selected();
        } else if copy {
            self.copy_selected();
        } else if paste {
            self.paste();
        } else if link && self.can_link() {
            self.edit_link();
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.quit {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.request_quit();
        }
        if self.quit {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        keys
    }

    fn file_menu(&mut self, ui: &mut egui::Ui, has_doc: bool) {
        if ui
            .add(egui::Button::new("New").shortcut_text(shortcut(ui.ctx(), Key::N)))
            .clicked()
        {
            self.request_new();
            ui.close();
        }
        if ui
            .add(egui::Button::new("Open...").shortcut_text(shortcut(ui.ctx(), Key::O)))
            .clicked()
        {
            ui.close();
            self.open_dialog();
        }
        ui.separator();
        if ui
            .add_enabled(
                has_doc,
                egui::Button::new("Save").shortcut_text(shortcut(ui.ctx(), Key::S)),
            )
            .clicked()
        {
            ui.close();
            self.save_or_save_as();
        }
        if ui
            .add_enabled(
                has_doc,
                egui::Button::new("Save As...").shortcut_text(shortcut(ui.ctx(), Key::A)),
            )
            .clicked()
        {
            ui.close();
            self.save_as_dialog();
        }
        ui.separator();
        self.file_menu_output(ui, has_doc);
        ui.separator();
        if ui
            .add(egui::Button::new("Quit").shortcut_text(shortcut(ui.ctx(), Key::Q)))
            .clicked()
        {
            ui.close();
            self.request_quit();
        }
    }

    /// Reports, bundles, images and printing.
    fn file_menu_output(&mut self, ui: &mut egui::Ui, has_doc: bool) {
        if ui
            .add_enabled(has_doc, egui::Button::new("Export report..."))
            .clicked()
        {
            ui.close();
            self.export_report_dialog();
        }
        if ui
            .add_enabled(
                self.mc.is_some(),
                egui::Button::new("Export Monte Carlo failure modes (CSV)..."),
            )
            .clicked()
        {
            ui.close();
            self.export_modes_csv_dialog();
        }
        if ui
            .add_enabled(has_doc, egui::Button::new("Export bug report bundle..."))
            .clicked()
        {
            ui.close();
            self.export_bundle_dialog();
        }
        if ui.button("Import bundle...").clicked() {
            ui.close();
            self.import_bundle_dialog();
        }
        if ui
            .add_enabled(has_doc, egui::Button::new("Export image..."))
            .clicked()
        {
            ui.close();
            self.export_image_dialog();
        }
        if ui
            .add_enabled(
                has_doc,
                egui::Button::new("Print...").shortcut_text(shortcut(ui.ctx(), Key::P)),
            )
            .clicked()
        {
            ui.close();
            self.print_now();
        }
        if ui
            .add_enabled(
                has_doc,
                egui::Button::new("Page setup and print preview..."),
            )
            .clicked()
        {
            ui.close();
            self.preview.open = true;
        }
        ui.checkbox(
            &mut self.image_probabilities,
            "Probabilities in images and prints",
        );
        ui.checkbox(&mut self.print_report, "Include the report when printing");
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui, has_doc: bool) {
        self.edit_menu_history(ui);
        ui.separator();
        self.edit_menu_tree(ui, has_doc);
    }

    /// Undo, redo and the clipboard.
    fn edit_menu_history(&mut self, ui: &mut egui::Ui) {
        if ui
            .add_enabled(
                self.history.can_undo(),
                egui::Button::new("Undo").shortcut_text(shortcut(ui.ctx(), Key::Z)),
            )
            .clicked()
        {
            ui.close();
            self.undo();
        }
        if ui
            .add_enabled(
                self.history.can_redo(),
                egui::Button::new("Redo").shortcut_text(shortcut(ui.ctx(), Key::Y)),
            )
            .clicked()
        {
            ui.close();
            self.redo();
        }
        ui.separator();
        if ui
            .add_enabled(
                self.can_cut(),
                egui::Button::new("Cut").shortcut_text(shortcut(ui.ctx(), Key::X)),
            )
            .clicked()
        {
            ui.close();
            self.cut_selected();
        }
        if ui
            .add_enabled(
                self.can_copy(),
                egui::Button::new("Copy").shortcut_text(shortcut(ui.ctx(), Key::C)),
            )
            .clicked()
        {
            ui.close();
            self.copy_selected();
        }
        if ui
            .add_enabled(
                self.can_paste(),
                egui::Button::new("Paste").shortcut_text(shortcut(ui.ctx(), Key::V)),
            )
            .clicked()
        {
            ui.close();
            self.paste();
        }
    }

    /// Moving, linking, editing and deleting tree events, and the other editors.
    fn edit_menu_tree(&mut self, ui: &mut egui::Ui, has_doc: bool) {
        let can_shift = self.selected.as_ref().is_some_and(|p| !p.is_empty());
        if ui
            .add_enabled(
                can_shift,
                egui::Button::new("Shift Left").shortcut_text(shortcut(ui.ctx(), Key::L)),
            )
            .clicked()
        {
            ui.close();
            self.move_selected(-1);
        }
        if ui
            .add_enabled(
                can_shift,
                egui::Button::new("Shift Right").shortcut_text(shortcut(ui.ctx(), Key::R)),
            )
            .clicked()
        {
            ui.close();
            self.move_selected(1);
        }
        if ui
            .add_enabled(
                self.can_link(),
                egui::Button::new("Link").shortcut_text(shortcut(ui.ctx(), Key::I)),
            )
            .on_disabled_hover_text(
                "Select a primary event in the tree and an event in the Primary event database window",
            )
            .clicked()
        {
            ui.close();
            self.edit_link();
        }
        if ui
            .add_enabled(self.selected.is_some(), egui::Button::new("Selected..."))
            .clicked()
        {
            ui.close();
            self.edit_selected();
        }
        ui.separator();
        if ui
            .add_enabled(has_doc, egui::Button::new("Primary events..."))
            .clicked()
        {
            ui.close();
            self.events_open = true;
        }
        if ui.button("Primary event database...").clicked() {
            ui.close();
            self.ped.open = true;
        }
        if ui.button("Preferences...").clicked() {
            ui.close();
            self.prefs_open = true;
        }
        ui.separator();
        let can_delete = self.selected.as_ref().is_some_and(|p| !p.is_empty());
        if ui
            .add_enabled(
                can_delete,
                egui::Button::new("Delete selected").shortcut_text(shortcut(ui.ctx(), Key::D)),
            )
            .clicked()
        {
            ui.close();
            self.delete_selected();
        }
    }

    fn view_menu(&mut self, ui: &mut egui::Ui, has_doc: bool) {
        if ui
            .add_enabled(has_doc, egui::Button::new("Tree Overview"))
            .clicked()
        {
            ui.close();
            self.overview_open = true;
        }
        ui.menu_button("Theme", |ui| {
            for (t, label) in [
                (ftar_render::Theme::System, "System"),
                (ftar_render::Theme::Light, "Light"),
                (ftar_render::Theme::Dark, "Dark"),
            ] {
                if ui.selectable_label(self.prefs.theme == t, label).clicked() {
                    ui.close();
                    let mut p = self.prefs.clone();
                    p.theme = t;
                    self.set_prefs(p);
                }
            }
        });
        ui.separator();
        if ui
            .add_enabled(
                has_doc,
                egui::Button::new("Zoom In").shortcut_text(shortcut(ui.ctx(), Key::Plus)),
            )
            .clicked()
        {
            ui.close();
            self.zoom_in();
        }
        if ui
            .add_enabled(
                has_doc,
                egui::Button::new("Zoom Out").shortcut_text(shortcut(ui.ctx(), Key::Minus)),
            )
            .clicked()
        {
            ui.close();
            self.zoom_out();
        }
        if ui
            .add_enabled(has_doc, egui::Button::new("Set Zoom..."))
            .clicked()
        {
            ui.close();
            self.set_zoom_percent = self.zoom_percent() as f32;
            self.set_zoom_open = true;
        }
        if ui
            .add_enabled(has_doc, egui::Button::new("Fit Tree"))
            .clicked()
        {
            ui.close();
            self.fit_next = true;
        }
    }

    fn analysis_menu(&mut self, ui: &mut egui::Ui, has_doc: bool) {
        if ui
            .add_enabled(has_doc, egui::Button::new("Validate"))
            .clicked()
        {
            ui.close();
            self.validate();
        }
        ui.separator();
        for (tab, label) in [
            (Tab::CutSets, "Minimal Cut Sets..."),
            (Tab::Probability, "Numerical Probability..."),
            (Tab::MonteCarlo, "Monte Carlo Simulation..."),
        ] {
            if ui.add_enabled(has_doc, egui::Button::new(label)).clicked() {
                ui.close();
                self.show_analysis(tab);
            }
        }
    }

    fn report_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Print", |ui| {
            for (k, label) in [
                (ReportKind::CutSets, "Qualitative Report"),
                (ReportKind::Probability, "Probability Report"),
                (ReportKind::MonteCarlo, "Monte Carlo Report"),
            ] {
                let on = self.saved_report_exists(k);
                if ui.add_enabled(on, egui::Button::new(label)).clicked() {
                    ui.close();
                    self.print_report_now(k);
                }
            }
        });
        ui.menu_button("View", |ui| {
            for (k, label) in [
                (ReportKind::CutSets, "Qualitative Report"),
                (ReportKind::Probability, "Probability Report"),
                (ReportKind::MonteCarlo, "Monte Carlo Report"),
            ] {
                let on = self.saved_report_exists(k);
                if ui.add_enabled(on, egui::Button::new(label)).clicked() {
                    ui.close();
                    let _ = self.view_saved_report(k);
                }
            }
        });
    }

    fn help_menu(&mut self, ui: &mut egui::Ui) {
        // OpenFTA's five pages first, then ftar's own two
        for (label, topic) in [
            ("Help on Help", "Help menu"),
            ("Index", "Overview"),
            ("User Support", "User support"),
            ("History", "History"),
            ("Copyright", "Copyright"),
        ] {
            if ui.button(label).clicked() {
                ui.close();
                self.show_help(Some(topic));
            }
        }
        ui.separator();
        if ui.button("User guide").clicked() {
            ui.close();
            self.show_help(None);
        }
        if ui.button("About ftar").clicked() {
            ui.close();
            self.show_help(Some(crate::help::ABOUT));
        }
    }

    /// The tree's database name, applied when the field loses focus.
    fn database_field(&mut self, ui: &mut egui::Ui) {
        let l = ui.label("database");
        let r = ui
            .add(
                egui::TextEdit::singleline(&mut self.buf.database)
                    .hint_text("none")
                    .desired_width(110.0),
            )
            .labelled_by(l.id);
        let current = self
            .doc
            .as_ref()
            .and_then(|d| d.tree.db_name.clone())
            .unwrap_or_default();
        if r.lost_focus() && self.buf.database.trim() != current {
            let name = self.buf.database.clone();
            self.set_database(&name);
        }
    }

    fn export_report_dialog(&mut self) {
        if let Some(p) = crate::desktop::FileDialog::new()
            .add_filter("Text report", &["txt"])
            .set_file_name(self.default_report_name())
            .save_file()
        {
            let _ = self.export_report(&p);
        }
    }

    fn export_bundle_dialog(&mut self) {
        if let Some(p) = crate::desktop::FileDialog::new()
            .add_filter("Zip bundle", &["zip"])
            .set_file_name(self.default_bundle_name())
            .save_file()
        {
            let _ = self.export_bundle(&p);
        }
    }

    /// Pick a bundle, then an empty folder to unpack it into.
    fn import_bundle_dialog(&mut self) {
        let Some(zip) = crate::desktop::FileDialog::new()
            .add_filter("Zip bundle", &["zip"])
            .pick_file()
        else {
            return;
        };
        if let Some(dest) = crate::desktop::FileDialog::new()
            .set_title("Unpack the bundle into (an empty folder)")
            .pick_folder()
        {
            let _ = self.import_bundle(&zip, &dest);
        }
    }

    fn export_image_dialog(&mut self) {
        if let Some(p) = crate::desktop::FileDialog::new()
            .add_filter("JPEG image", &["jpg", "jpeg"])
            .add_filter("PNG image", &["png"])
            .add_filter("SVG drawing", &["svg"])
            .set_file_name(self.default_image_name())
            .save_file()
        {
            let _ = self.export_image(&p);
        }
    }

    /// Report > Print: open the browser's print dialog for a saved report.
    pub(crate) fn print_report_now(&mut self, kind: ReportKind) {
        if let Ok(path) = self.prepare_report_print(kind) {
            if let Err(e) = open_with_default_app(&path, &self.prefs.browser) {
                self.error = Some(format!("could not open the print page: {e}"));
            }
        }
    }

    /// Open the browser's print dialog for the tree (and report).
    pub(crate) fn print_now(&mut self) {
        if let Ok(path) = self.prepare_print() {
            if let Err(e) = open_with_default_app(&path, &self.prefs.browser) {
                self.error = Some(format!("could not open the print page: {e}"));
            }
        }
    }

    fn export_modes_csv_dialog(&mut self) {
        if let Some(p) = crate::desktop::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .set_file_name(self.default_modes_csv_name())
            .save_file()
        {
            let _ = self.export_modes_csv(&p);
        }
    }

    pub(crate) fn open_dialog(&mut self) {
        if let Some(p) = crate::desktop::FileDialog::new()
            .add_filter("OpenFTA tree", &["fta"])
            .add_filter("ftar text tree", &["ftt"])
            .pick_file()
        {
            self.request_open(p);
        }
    }

    pub(crate) fn save_or_save_as(&mut self) {
        if self.doc.as_ref().is_some_and(|d| d.path.is_some()) {
            let _ = self.save();
        } else {
            self.save_as_dialog();
        }
    }

    /// Returns true if the file was saved.
    fn save_as_dialog(&mut self) -> bool {
        let name = self
            .doc
            .as_ref()
            .and_then(|d| d.path.as_ref())
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled.fta".into());
        match crate::desktop::FileDialog::new()
            .add_filter("OpenFTA tree", &["fta"])
            .add_filter("ftar text tree", &["ftt"])
            .set_file_name(name)
            .save_file()
        {
            Some(p) => self.save_as(p).is_ok(),
            None => false,
        }
    }

    /// The primary event editor, laid out like OpenPED: the events on the left,
    /// the selected event's fields on the right, Apply / Clear / Delete below.
    /// View > Set Zoom: OpenFTA's preset percentages, or any value.
    fn set_zoom_window(&mut self, ctx: &egui::Context) {
        if !self.set_zoom_open || self.doc.is_none() {
            return;
        }
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Set Zoom")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for p in [25.0, 50.0, 100.0, 150.0, 200.0] {
                        if ui
                            .selectable_label(self.set_zoom_percent == p, format!("{p}%"))
                            .clicked()
                        {
                            self.set_zoom_percent = p;
                            apply = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("zoom");
                    let r = ui.add(
                        egui::DragValue::new(&mut self.set_zoom_percent)
                            .range(App::ZOOM_RANGE.0 * 100.0..=App::ZOOM_RANGE.1 * 100.0)
                            .suffix("%"),
                    );
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        apply = true;
                    }
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                });
            });
        if apply {
            self.set_zoom(self.set_zoom_percent / 100.0);
        }
        self.set_zoom_open = open;
    }

    /// View > Tree Overview: the whole tree, with the part the canvas shows
    /// outlined; click or drag to move the view there.
    fn overview_window(&mut self, ctx: &egui::Context) {
        if !self.overview_open {
            return;
        }
        let Some(scene) = self.scene() else {
            self.overview_open = false;
            return;
        };
        let mut open = true;
        let mut target = None;
        egui::Window::new("Tree Overview")
            .open(&mut open)
            .default_size([300.0, 200.0])
            .resizable(true)
            .show(ctx, |ui| {
                let size = ui.available_size().max(Vec2::new(120.0, 80.0));
                let (resp, painter) = ui.allocate_painter(size, Sense::click_and_drag());
                let r = resp.rect;
                let z = (r.width() / scene.width).min(r.height() / scene.height);
                let origin = r.center() - Vec2::new(scene.width, scene.height) * z / 2.0;
                paint_scene(
                    &painter.with_clip_rect(r),
                    &scene,
                    origin,
                    z,
                    None,
                    &self.prefs,
                );
                let seen = self.visible_world();
                let frame = Rect::from_min_max(
                    origin + seen.min.to_vec2() * z,
                    origin + seen.max.to_vec2() * z,
                )
                .intersect(r);
                painter.rect_stroke(
                    frame,
                    0.0,
                    Stroke::new(2.0, Color32::from_rgb(0, 120, 215)),
                    egui::StrokeKind::Inside,
                );
                if resp.clicked() || resp.dragged() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        target = Some((p - origin) / z);
                    }
                }
            });
        if let Some(w) = target {
            self.centre_on(w);
        }
        self.overview_open = open;
    }

    /// Analysis > Validate: the structural problems of the tree; click one to
    /// select its node.
    fn validate_window(&mut self, ctx: &egui::Context) {
        if !self.validate_open {
            return;
        }
        let mut open = true;
        let mut pick = None;
        let issues = self.issues();
        let names: Vec<String> = match &self.doc {
            Some(doc) => issues
                .iter()
                .map(|i| match ftar_core::edit::node_at(&doc.tree, &i.path) {
                    Some(n) if !n.id.is_empty() => n.id.clone(),
                    Some(n) => kind_name(n.kind).to_string(),
                    None => String::new(),
                })
                .collect(),
            None => Vec::new(),
        };
        egui::Window::new("Validate")
            .open(&mut open)
            .default_size([420.0, 240.0])
            .show(ctx, |ui| {
                if issues.is_empty() {
                    ui.label("No structural problems found.");
                    return;
                }
                ui.label(format!(
                    "{} problem(s); click one to select it.",
                    issues.len()
                ));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, (issue, name)) in issues.iter().zip(&names).enumerate() {
                        let (tag, colour) = match issue.severity {
                            ftar_core::edit::Severity::Error => {
                                ("Error", Color32::from_rgb(230, 90, 90))
                            }
                            ftar_core::edit::Severity::Warning => {
                                ("Warning", Color32::from_rgb(230, 170, 60))
                            }
                        };
                        ui.horizontal(|ui| {
                            ui.colored_label(colour, tag);
                            if ui.link(format!("{name}: {}", issue.message)).clicked() {
                                pick = Some(i);
                            }
                        });
                    }
                });
            });
        if let Some(i) = pick {
            self.select_issue(i);
        }
        self.validate_open = open;
    }

    fn events_window(&mut self, ctx: &egui::Context) {
        if !self.events_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Primary events")
            .open(&mut open)
            .default_size([620.0, 380.0])
            .show(ctx, |ui| {
                if self.doc.is_none() {
                    ui.label("Open or create a tree first.");
                    return;
                }
                let name = self
                    .doc
                    .as_ref()
                    .and_then(|d| d.tree.db_name.clone())
                    .unwrap_or_else(|| "(not named yet)".into());
                ui.label(format!("Database: {name}"));
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.strong("Primary Events");
                        egui::ScrollArea::vertical()
                            .id_salt("event_list")
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for id in self.event_ids() {
                                    let sel = self.event_form.original.as_deref() == Some(&id);
                                    if ui.selectable_label(sel, &id).clicked() {
                                        self.event_form_load(&id);
                                    }
                                }
                            });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        let l = ui.label("Id");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.event_form.id)
                                .desired_width(200.0),
                        )
                        .labelled_by(l.id);
                        let l = ui.label("Probability");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.event_form.prob)
                                .desired_width(120.0),
                        )
                        .labelled_by(l.id);
                        ui.checkbox(&mut self.event_form.rate, "Constant Failure Rate/Unit Time");
                        if let Some(h) = self.rate_hint(&self.event_form.prob, self.event_form.rate)
                        {
                            ui.weak(h);
                        }
                        ui.add_space(4.0);
                        ui.strong("Type");
                        for (k, label) in [
                            (EventType::Basic, "Basic"),
                            (EventType::External, "External"),
                            (EventType::Undeveloped, "Undeveloped"),
                            (
                                EventType::ConditionNotAnalysed,
                                "Conditional (not analysed)",
                            ),
                            (EventType::ConditionAnalysed, "Conditional (analysed)"),
                        ] {
                            ui.radio_value(&mut self.event_form.kind, k, label);
                        }
                        ui.checkbox(&mut self.event_form.dormant, "Dormant");
                        let l = ui.label("Description");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.event_form.desc)
                                .desired_rows(3)
                                .desired_width(280.0),
                        )
                        .labelled_by(l.id);
                        if let Some(id) = self.event_form.original.clone() {
                            let used = self.other_trees_using(&id);
                            ui.label(used);
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() {
                                self.event_form_apply();
                            }
                            if ui.button("Clear").clicked() {
                                self.event_form_clear();
                            }
                            if ui.button("Delete").clicked() {
                                self.event_form_delete();
                            }
                        });
                    });
                });
                if let Some(e) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
            });
        self.events_open = open;
    }

    /// The saved report opened by Report > View.
    fn report_window(&mut self, ctx: &egui::Context) {
        let Some((kind, text)) = &self.report_view else {
            return;
        };
        let mut open = true;
        egui::Window::new("Report")
            .open(&mut open)
            .default_size([640.0, 480.0])
            .show(ctx, |ui| {
                ui.label(kind.label());
                egui::ScrollArea::both().show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(text).monospace()).selectable(true),
                    );
                });
            });
        if !open {
            self.report_view = None;
        }
    }

    fn unsaved_prompt(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pending.clone() else {
            return;
        };
        let what = match p {
            Pending::New => "start a new tree",
            Pending::Open(_) => "open another file",
            Pending::Quit => "quit",
        };
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!("Save your changes before you {what}?"));
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        let has_path = self.doc.as_ref().is_some_and(|d| d.path.is_some());
                        if has_path {
                            self.resolve_pending(true);
                        } else if self.save_as_dialog() {
                            self.resolve_pending(false);
                        }
                    }
                    if ui.button("Discard").clicked() {
                        self.resolve_pending(false);
                    }
                    if ui.button("Cancel").clicked() {
                        self.cancel_pending();
                    }
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if let Some(e) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                } else if let Some(s) = &self.status {
                    ui.label(s);
                }
                let n = self
                    .issues()
                    .iter()
                    .filter(|i| i.severity == ftar_core::edit::Severity::Error)
                    .count();
                if n > 0 {
                    ui.colored_label(
                        Color32::from_rgb(230, 170, 60),
                        format!("{n} structural problem(s): see Analysis > Validate"),
                    );
                }
            });
        });
    }

    /// Reload the text buffers when the selection or document changed.
    fn refresh_buffers(&mut self) {
        let key = (self.selected.clone(), self.version);
        if self.buf.key.as_ref() == Some(&key) {
            return;
        }
        self.buf.key = Some(key);
        let (id, text, kind) = self
            .selected_node()
            .map(|n| (n.id.clone(), n.text.clone(), Some(n.kind)))
            .unwrap_or_default();
        self.buf.id = id.clone();
        self.buf.text = text;
        let ev = self
            .doc
            .as_ref()
            .and_then(|d| d.db.events.get(&id))
            .cloned();
        let _ = kind;
        self.buf.prob = ev
            .as_ref()
            .map(|e| format!("{:e}", e.probability))
            .unwrap_or_default();
        self.buf.desc = ev
            .as_ref()
            .map(|e| e.description.clone())
            .unwrap_or_default();
        self.buf.rate = ev.as_ref().is_some_and(|e| e.lambda == Some('L'));
        self.buf.database = self
            .doc
            .as_ref()
            .and_then(|d| d.tree.db_name.clone())
            .unwrap_or_default();
    }

    fn details_panel(&mut self, ui: &mut egui::Ui) {
        self.refresh_buffers();
        egui::Panel::bottom("details").show(ui, |ui| {
            let Some(node) = self.selected_node().cloned() else {
                ui.label("Click a node to edit it. Scroll to zoom, drag to pan.");
                if self.doc.is_some() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Nothing selected: new nodes go under the top event.");
                        self.add_controls(ui, true);
                    });
                }
                return;
            };
            let is_root = self.selected.as_ref().is_some_and(|p| p.is_empty());
            ui.horizontal_wrapped(|ui| {
                ui.strong(kind_name(node.kind));
                egui::ComboBox::from_label("type")
                    .selected_text(kind_name(node.kind))
                    .show_ui(ui, |ui| {
                        for &k in GATES.iter().chain(EVENTS) {
                            if ui.selectable_label(node.kind == k, kind_name(k)).clicked()
                                && node.kind != k
                            {
                                self.set_kind(k);
                            }
                        }
                    });
                let l = ui.label("id");
                let r = ui
                    .add(egui::TextEdit::singleline(&mut self.buf.id).desired_width(110.0))
                    .labelled_by(l.id);
                if std::mem::take(&mut self.focus_details) {
                    r.request_focus();
                }
                if r.lost_focus() && self.buf.id != node.id {
                    let id = self.buf.id.clone();
                    self.set_id(&id);
                }
                if node.kind == EventType::Intermediate {
                    let l = ui.label("description");
                    let r = ui
                        .add(egui::TextEdit::singleline(&mut self.buf.text).desired_width(260.0))
                        .labelled_by(l.id);
                    if r.lost_focus() && self.buf.text != node.text {
                        let t = self.buf.text.clone();
                        self.set_text(&t);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if accepts_children(node.kind) {
                    self.add_controls(ui, false);
                    ui.separator();
                }
                if !is_root {
                    if ui.button("Delete").clicked() {
                        self.delete_selected();
                    }
                    if ui.button("Shift left").clicked() {
                        self.move_selected(-1);
                    }
                    if ui.button("Shift right").clicked() {
                        self.move_selected(1);
                    }
                }
            });
            if (node.kind.is_primary() || node.kind == EventType::ConditionAnalysed)
                && !node.id.is_empty()
            {
                ui.horizontal_wrapped(|ui| {
                    ui.label(self.event_origin(&node.id));
                    ui.label(self.other_trees_using(&node.id));
                    let l = ui.label("probability");
                    let pr = ui
                        .add(egui::TextEdit::singleline(&mut self.buf.prob).desired_width(90.0))
                        .labelled_by(l.id);
                    let l = ui.label("event description");
                    let dr = ui
                        .add(egui::TextEdit::singleline(&mut self.buf.desc).desired_width(240.0))
                        .labelled_by(l.id);
                    let rr = ui.checkbox(&mut self.buf.rate, "failure rate");
                    if let Some(h) = self.rate_hint(&self.buf.prob, self.buf.rate) {
                        ui.weak(h);
                    }
                    if pr.lost_focus() || dr.lost_focus() || rr.changed() {
                        match self.buf.prob.trim().parse::<f64>() {
                            Ok(p) => {
                                let (id, d, rate) =
                                    (node.id.clone(), self.buf.desc.clone(), self.buf.rate);
                                self.set_event(&id, p, &d, rate);
                            }
                            Err(_) => {
                                self.error = Some("probability is not a number".into());
                            }
                        }
                    }
                });
            }
        });
    }

    fn add_controls(&mut self, ui: &mut egui::Ui, _root: bool) {
        egui::ComboBox::from_label("add")
            .selected_text(kind_name(self.add_kind))
            .show_ui(ui, |ui| {
                for &k in GATES.iter().chain(EVENTS) {
                    ui.selectable_value(&mut self.add_kind, k, kind_name(k));
                }
            });
        if ui.button("Add child").clicked() {
            let k = self.add_kind;
            self.add_child(k);
        }
    }
}

fn kind_name(k: EventType) -> &'static str {
    use EventType::*;
    match k {
        Unknown => "Unknown",
        Basic => "Basic event",
        And => "AND gate",
        Or => "OR gate",
        TransferIn => "Transfer in",
        TransferOut => "Transfer out",
        Intermediate => "Intermediate event",
        Undeveloped => "Undeveloped event",
        Inhibit => "Inhibit gate",
        PriorityAnd => "Priority AND gate",
        Xor => "XOR gate",
        External => "External event",
        ConditionNotAnalysed => "Condition (not analysed)",
        ConditionAnalysed => "Condition (analysed)",
        Dormant => "Dormant event",
    }
}

/// Empty border around the fitted tree, in screen pixels.
const MARGIN: f32 = 12.0;

/// Zoom and pan that centre a drawing of `content` size in a canvas of `size`, with a margin.
pub(crate) fn fit(size: Vec2, content: Vec2) -> (f32, Vec2) {
    let c = content.max(Vec2::splat(1.0));
    let avail = (size - Vec2::splat(2.0 * MARGIN)).max(Vec2::splat(1.0));
    let z = (avail.x / c.x).min(avail.y / c.y).clamp(0.02, 1.5);
    (
        z,
        Vec2::new(
            (size.x - c.x * z) / 2.0,
            ((size.y - c.y * z) / 2.0).max(0.0),
        ),
    )
}

fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Paint `scene` on white paper with its origin at `origin` and scale `z`;
/// the selected item is outlined in OpenFTA's highlight colour.
fn paint_scene(
    painter: &egui::Painter,
    scene: &Scene,
    origin: Pos2,
    z: f32,
    selected: Option<&[usize]>,
    prefs: &Preferences,
) {
    let to = |p: [f32; 2]| origin + Vec2::new(p[0], p[1]) * z;
    painter.rect_filled(
        Rect::from_min_size(origin, Vec2::new(scene.width, scene.height) * z),
        0.0,
        rgb32(prefs.background_colour()),
    );
    let pen = Stroke::new((1.2 * z).max(0.6), rgb32(prefs.pen_colour()));
    let highlight = selected
        .and_then(|path| scene.items.iter().find(|it| it.path == path))
        .map(|it| it.prims.clone());
    let clip = painter.clip_rect();
    for (i, prim) in scene.prims.iter().enumerate() {
        match prim {
            Prim::Path {
                pts,
                closed,
                fill,
                dashed,
            } => {
                let pts: Vec<Pos2> = pts.iter().map(|&p| to(p)).collect();
                if !Rect::from_points(&pts).expand(4.0).intersects(clip) {
                    continue;
                }
                if let Some(f) = fill {
                    let mut mesh = egui::Mesh::default();
                    for &p in &pts {
                        mesh.colored_vertex(p, rgb(*f));
                    }
                    let flat: Vec<[f32; 2]> = pts.iter().map(|p| [p.x, p.y]).collect();
                    for [a, b, c] in triangulate(&flat) {
                        mesh.add_triangle(a as u32, b as u32, c as u32);
                    }
                    painter.add(Shape::mesh(mesh));
                }
                let stroke = if highlight.as_ref().is_some_and(|r| r.contains(&i)) {
                    Stroke::new((2.5 * z).max(2.0), rgb32(prefs.highlight_colour()))
                } else {
                    pen
                };
                let mut line = pts;
                if *closed {
                    line.push(line[0]);
                }
                if *dashed {
                    painter.extend(Shape::dashed_line(&line, stroke, 5.0 * z, 3.0 * z));
                } else {
                    painter.add(Shape::line(line, stroke));
                }
            }
            Prim::Text {
                pos,
                size,
                anchor,
                bold: _,
                text,
            } => {
                let px = size * z;
                if px < 5.0 {
                    continue; // too small to read, as OpenFTA hides text when zoomed out
                }
                let p = to(*pos);
                if !clip.expand(300.0).contains(p) {
                    continue;
                }
                let align = match anchor {
                    Anchor::Start => Align2::LEFT_CENTER,
                    Anchor::Middle => Align2::CENTER_CENTER,
                };
                painter.text(
                    p,
                    align,
                    text,
                    FontId::proportional(px),
                    rgb32(prefs.pen_colour()),
                );
            }
        }
    }
}

impl App {
    /// The tree as the canvas draws it.
    pub fn scene(&self) -> Option<Scene> {
        let doc = self.doc.as_ref()?;
        Some(tree_scene(
            &doc.tree,
            &doc.db,
            &SceneOptions {
                title: self.tree_name(),
                probabilities: true,
                prefs: self.prefs.clone(),
            },
        ))
    }

    /// Where the whole drawing currently is on screen.
    pub fn content_bounds(&self) -> Option<Rect> {
        let scene = self.scene()?;
        let min = self.canvas.min + self.pan;
        Some(Rect::from_min_size(
            min,
            Vec2::new(scene.width, scene.height) * self.zoom,
        ))
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.set_theme(match self.prefs.theme {
            ftar_render::Theme::Light => egui::ThemePreference::Light,
            ftar_render::Theme::Dark => egui::ThemePreference::Dark,
            ftar_render::Theme::System => egui::ThemePreference::System,
        });

        self.top_bar(ui);
        self.toolbar(ui);
        self.unsaved_prompt(&ctx);
        self.help_window(&ctx);

        self.palette(ui);

        egui::Panel::right("analysis")
            .default_size(420.0)
            .show(ui, |ui| self.analysis_panel(ui));

        self.status_bar(ui);
        self.details_panel(ui);
        self.events_window(ui.ctx());
        self.validate_window(ui.ctx());
        self.ped_link_question(ui.ctx());
        self.ped_window(ui.ctx());
        self.report_window(ui.ctx());
        self.overview_window(ui.ctx());
        self.set_zoom_window(ui.ctx());
        self.prefs_window(ui.ctx());
        self.preview_window(ui.ctx());

        // The canvas is the drawing area itself: no frame or margin, filled edge to
        // edge with the preferences' background colour (#97).
        let panel = egui::Frame::NONE.fill(if self.doc.is_some() {
            rgb32(self.prefs.background_colour())
        } else {
            ctx.global_style().visuals.panel_fill
        });
        egui::CentralPanel::default()
            .frame(panel)
            .show(ui, |ui| self.canvas_area(ui));
    }
}

impl App {
    /// The analysis side panel: controls, actions, then the results of the current tab.
    fn analysis_panel(&mut self, ui: &mut egui::Ui) {
        self.analysis_controls(ui);
        self.analysis_parameters(ui);
        self.analysis_actions(ui);
        ui.separator();
        self.analysis_results(ui);
    }

    /// Tabs, the event count and the cut set order limit.
    fn analysis_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let before = self.tab;
            ui.selectable_value(&mut self.tab, Tab::CutSets, "Cut sets");
            ui.selectable_value(&mut self.tab, Tab::Probability, "Probability");
            ui.selectable_value(&mut self.tab, Tab::MonteCarlo, "Monte Carlo");
            if self.tab != before {
                // the report to export follows the tab you are looking at
                self.report_kind = App::kind_for_tab(self.tab);
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if self.doc.is_some() {
                ui.label(format!(
                    "There are {} basic events in the tree",
                    self.basic_event_count()
                ));
            }
            if ui.button("Analysis help").clicked() {
                self.show_help(Some("Analysis"));
            }
        });
        ui.horizontal(|ui| {
            let l = ui.label("max order (0 = all)");
            ui.add(egui::DragValue::new(&mut self.order).range(0..=50))
                .labelled_by(l.id);
        });
    }

    /// The current tab's own parameters.
    fn analysis_parameters(&mut self, ui: &mut egui::Ui) {
        match self.tab {
            Tab::Probability => {
                ui.horizontal(|ui| {
                    let l = ui.label("terms");
                    ui.add(egui::DragValue::new(&mut self.terms).range(1..=16))
                        .labelled_by(l.id);
                    let l = ui.label("unit time");
                    ui.add(egui::DragValue::new(&mut self.unit_time).speed(0.1))
                        .labelled_by(l.id);
                });
            }
            Tab::MonteCarlo => {
                ui.horizontal(|ui| {
                    let l = ui.label("trials");
                    ui.add(egui::DragValue::new(&mut self.trials).range(1..=100_000_000))
                        .labelled_by(l.id);
                    let l = ui.label("unit time");
                    ui.add(egui::DragValue::new(&mut self.unit_time).speed(0.1))
                        .labelled_by(l.id);
                });
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("method")
                        .selected_text(match self.mc_method {
                            Method::FirstFailure => "first failure (fast)",
                            Method::Direct => "direct",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.mc_method,
                                Method::FirstFailure,
                                "first failure (fast)",
                            );
                            ui.selectable_value(&mut self.mc_method, Method::Direct, "direct");
                        });
                    let l = ui.label("seed");
                    ui.add(egui::DragValue::new(&mut self.seed))
                        .labelled_by(l.id);
                });
            }
            Tab::CutSets => {}
        }
    }

    /// Run, the report choice and its export/copy, and the last error.
    fn analysis_actions(&mut self, ui: &mut egui::Ui) {
        if ui
            .add_enabled(self.doc.is_some(), egui::Button::new("Run"))
            .clicked()
        {
            self.run_analysis();
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("report")
                .selected_text(self.report_kind.label())
                .show_ui(ui, |ui| {
                    for k in ReportKind::ALL {
                        ui.selectable_value(&mut self.report_kind, k, k.label());
                    }
                });
            let has_doc = self.doc.is_some();
            if ui
                .add_enabled(has_doc, egui::Button::new("Export report..."))
                .clicked()
            {
                self.export_report_dialog();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Copy report"))
                .clicked()
            {
                match self.report_text(self.report_kind, &App::now_text()) {
                    Ok(t) => {
                        ui.ctx().copy_text(t);
                        self.status = Some("Report copied to the clipboard".into());
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        });
        if self.tab == Tab::MonteCarlo
            && ui
                .add_enabled(
                    self.mc.is_some(),
                    egui::Button::new("Export failure modes (CSV)..."),
                )
                .clicked()
        {
            self.export_modes_csv_dialog();
        }
        if let Some(e) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, e);
        }
    }

    fn analysis_results(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
            Tab::CutSets => {
                if let Some(cs) = &self.cut_sets {
                    ui.label(format!("{} minimal cut sets", cs.len()));
                    for (i, s) in cs.iter().enumerate() {
                        ui.monospace(format!("{:>4}  {}", i + 1, s.join(" ")));
                    }
                }
            }
            Tab::Probability => {
                if let Some(r) = &self.prob {
                    ui.strong("Cut set probabilities");
                    for (i, (cs, p)) in r.cut_sets.iter().enumerate() {
                        ui.monospace(format!("{:>3} {:<28} {:.3E}", i + 1, cs.join(" "), p));
                    }
                    ui.add_space(6.0);
                    ui.strong("Top event");
                    for (k, (t, s)) in r.terms.iter().zip(&r.partial_sums).enumerate() {
                        let b = if k % 2 == 0 { "upper" } else { "lower" };
                        ui.monospace(format!("{:>2} terms {:+.3E} = {:.6E} ({b})", k + 1, t, s));
                    }
                    if r.exact {
                        ui.monospace(format!("Exact value: {:.6E}", r.top));
                    }
                    ui.add_space(6.0);
                    ui.strong("Primary events");
                    for e in &r.events {
                        ui.monospace(format!(
                            "{:<14} {:.3E} {:6.2}%",
                            e.id, e.contribution, e.importance_pct
                        ));
                    }
                }
            }
            Tab::MonteCarlo => {
                if let Some(r) = &self.mc {
                    ui.monospace(format!("tests           {}", r.trials));
                    ui.monospace(format!("system failures {}", r.system_failures));
                    ui.monospace(match r.upper_bound_95() {
                        Some(b) => {
                            format!("P(top)          < {b:.2E} (95%, no failures simulated)")
                        }
                        None => format!(
                            "P(top)          {:.4E} +/- {:.2E}",
                            r.probability, r.std_error
                        ),
                    });
                    ui.monospace(format!("P(any failure)  {:.4E}", r.any_component_failure));
                    ui.add_space(6.0);
                    ui.strong(format!("Failure modes ({})", r.modes.len()));
                    for (cs, n) in r.modes.iter().take(40) {
                        ui.monospace(format!(
                            "{:>8}  {:.3E}  {}",
                            n,
                            r.bas_prob * *n as f64 / r.trials.max(1) as f64,
                            cs.join(" ")
                        ));
                    }
                    if r.modes.len() > 40 {
                        ui.label(format!(
                            "... {} more (export to see all)",
                            r.modes.len() - 40
                        ));
                    }
                }
            }
        });
    }

    /// The tree drawing with zoom, pan, scrollbars and selection.
    fn canvas_area(&mut self, ui: &mut egui::Ui) {
        let Some(scene) = self.scene() else {
            ui.centered_and_justified(|ui| {
                ui.label("Open a .fta file (Open...), or start a New tree.")
            });
            return;
        };
        let rect = ui.available_rect_before_wrap();
        let resp = ui.interact(
            rect,
            ui.scope_id().with("canvas_area"),
            Sense::click_and_drag(),
        );

        self.canvas_navigate(ui, &resp, rect, &scene);

        // The drawing goes under the scrollbars, so it is painted first.
        let origin = rect.min + self.pan;
        let z = self.zoom;
        paint_scene(
            &ui.painter_at(rect),
            &scene,
            origin,
            z,
            self.selected.as_deref(),
            &self.prefs,
        );

        self.canvas_scrollbars(ui, rect, &scene);

        if resp.clicked() {
            let hit = resp.interact_pointer_pos().and_then(|pos| {
                let w = (pos - origin) / z;
                scene.item_at([w.x, w.y]).map(|it| it.path.clone())
            });
            self.selected = hit;
            self.version += 1;
        }
        if resp.double_clicked() && self.selected.is_some() {
            self.edit_selected();
        }
    }

    /// Fit on demand or on resize, drag to pan, scroll wheel to zoom about the pointer.
    fn canvas_navigate(&mut self, ui: &egui::Ui, resp: &egui::Response, rect: Rect, scene: &Scene) {
        let key = (rect.size(), scene.width, scene.height);
        if self.fit_next || (self.auto_fit && self.fit_key != Some(key)) {
            let (z, pan) = fit(rect.size(), Vec2::new(scene.width, scene.height));
            self.zoom = z;
            self.pan = pan;
            self.fit_next = false;
            self.auto_fit = true;
            self.fit_key = Some(key);
        }
        self.canvas = rect;
        if resp.dragged() {
            self.pan += resp.drag_delta();
            self.auto_fit = false;
        }
        if resp.hovered() {
            let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.auto_fit = false;
                let old = self.zoom;
                self.zoom = (self.zoom * (1.0 + scroll * 0.002)).clamp(0.02, 4.0);
                if let Some(m) = resp.hover_pos() {
                    let k = self.zoom / old;
                    let rel = m - rect.min;
                    self.pan = rel - (rel - self.pan) * k;
                }
            }
        }
    }

    /// Scrollbars: an invisible scroll area sized to the drawing plus a margin
    /// carries them; the pan is mirrored to its offset and back.
    fn canvas_scrollbars(&mut self, ui: &mut egui::Ui, rect: Rect, scene: &Scene) {
        let content = Vec2::new(scene.width, scene.height) * self.zoom;
        let extent = scroll_extent(rect.size(), content);
        self.scroll_extent = extent;
        let want = scroll_offset_for(self.pan, rect.size(), extent);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        // Permanent (solid) scrollbars, as OpenFTA shows, only on an axis that overflows.
        child.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
        let out = egui::ScrollArea::both()
            .id_salt("canvas_scroll")
            .auto_shrink(false)
            .scroll([extent.x > rect.width(), extent.y > rect.height()])
            .scroll_source(egui::containers::scroll_area::ScrollSource::SCROLL_BAR)
            .scroll_offset(want)
            .show(&mut child, |ui| {
                ui.allocate_exact_size(extent, Sense::hover());
            });
        let pan_before = self.pan;
        let got = out.state.clamped_offset();
        if (got.x - want.x).abs() > 0.5 && extent.x > rect.width() {
            self.pan.x = SCROLL_MARGIN - got.x;
            self.auto_fit = false;
        }
        if (got.y - want.y).abs() > 0.5 && extent.y > rect.height() {
            self.pan.y = SCROLL_MARGIN - got.y;
            self.auto_fit = false;
        }

        if self.pan != pan_before {
            ui.ctx().request_repaint();
        }
    }
}

/// Empty space kept around the drawing inside the scrollable extent, in screen pixels.
const SCROLL_MARGIN: f32 = MARGIN;

/// Size of the scrollable area for a drawing of `content` pixels in a canvas of `view`:
/// the drawing plus a margin, never smaller than the canvas (then there is no scrollbar).
pub(crate) fn scroll_extent(view: Vec2, content: Vec2) -> Vec2 {
    (content + Vec2::splat(2.0 * SCROLL_MARGIN)).max(view)
}

/// Scroll offset showing the drawing at screen offset `pan`.
fn scroll_offset_for(pan: Vec2, view: Vec2, extent: Vec2) -> Vec2 {
    let off = Vec2::splat(SCROLL_MARGIN) - pan;
    Vec2::new(
        off.x.clamp(0.0, (extent.x - view.x).max(0.0)),
        off.y.clamp(0.0, (extent.y - view.y).max(0.0)),
    )
}

fn rgb32(c: ftar_render::prefs::Rgb) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Open a file with the system's default application (the browser, for HTML).
pub(crate) fn open_with_default_app(path: &std::path::Path, browser: &str) -> std::io::Result<()> {
    use std::process::Command;
    // the GUI's own tests build with `test-no-launch`: nothing opens on the desktop
    if !crate::desktop::LAUNCHES {
        return Ok(());
    }
    if let Some((program, args)) = browser_command(browser, path) {
        return Command::new(program).args(args).spawn().map(|_| ());
    }
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]).arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    cmd.spawn().map(|_| ())
}

/// The program and arguments for a user's browser launch command (the page is
/// added as the last argument), or `None` for the system default when the
/// command is empty. Words are split on spaces; double quotes keep spaces in
/// one word. No shell is involved, so nothing in a file name is interpreted.
pub fn browser_command(command: &str, page: &std::path::Path) -> Option<(String, Vec<String>)> {
    let mut words: Vec<String> = Vec::new();
    let (mut cur, mut quoted, mut any) = (String::new(), false, false);
    for c in command.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    words.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(cur);
    }
    let mut it = words.into_iter();
    let program = it.next()?;
    let mut args: Vec<String> = it.collect();
    args.push(page.to_string_lossy().into_owned());
    Some((program, args))
}

#[cfg(test)]
mod scroll_tests {
    use super::*;

    #[test]
    fn extent_never_smaller_than_the_view() {
        let v = Vec2::new(800.0, 600.0);
        assert_eq!(scroll_extent(v, Vec2::new(100.0, 100.0)), v);
        let e = scroll_extent(v, Vec2::new(2000.0, 100.0));
        assert_eq!(e.x, 2000.0 + 2.0 * SCROLL_MARGIN);
        assert_eq!(e.y, 600.0);
    }

    #[test]
    fn pan_and_offset_round_trip() {
        let v = Vec2::new(800.0, 600.0);
        let e = scroll_extent(v, Vec2::new(2000.0, 1500.0));
        let pan = Vec2::new(-300.0, -50.0);
        let off = scroll_offset_for(pan, v, e);
        assert_eq!(SCROLL_MARGIN - off.x, pan.x);
        assert_eq!(SCROLL_MARGIN - off.y, pan.y);
        // a centred small drawing has nothing to scroll
        let e = scroll_extent(v, Vec2::new(100.0, 100.0));
        assert_eq!(scroll_offset_for(Vec2::new(350.0, 250.0), v, e), Vec2::ZERO);
    }
}
