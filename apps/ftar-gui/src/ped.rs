//! The primary event database as a window of its own (OpenFTA's OpenPED):
//! new, open, save, save as and text export without a tree, copying events
//! between this database and the tree's, and linking a tree event to one.
//! The operations are plain methods on [`App`] so tests drive them headless.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32};
use ftar_core::database::{self, LinkOutcome};
use ftar_core::parse::parse_ped;
use ftar_core::write::write_ped;
use ftar_core::{EventType, PrimaryEvent, PrimaryEventDb};

use crate::app::{App, EventForm};

/// State of the database window. It holds its own database, independent of
/// the open tree's.
#[derive(Default)]
pub struct PedWindow {
    pub open: bool,
    pub db: PrimaryEventDb,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    /// Event selected in the list.
    pub selected: Option<String>,
    pub form: EventForm,
    pub error: Option<String>,
    pub status: Option<String>,
    /// A link waits for the user to accept differing types (event id).
    pub confirm_link: Option<String>,
}

impl PedWindow {
    pub fn event_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.db.events.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Title for the window: file name, `*` when unsaved.
    pub fn name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

fn form_event(f: &EventForm, group: String) -> Result<PrimaryEvent, String> {
    let probability = f
        .prob
        .trim()
        .parse::<f64>()
        .map_err(|_| "probability is not a number".to_string())?;
    Ok(PrimaryEvent {
        id: f.id.trim().to_string(),
        group,
        kind: f.kind,
        modifier: f.dormant.then_some('D'),
        description: f.desc.clone(),
        probability,
        lambda: f.rate.then_some('L'),
    })
}

impl App {
    /// File > New in the database window: an empty, unnamed database.
    pub fn ped_new(&mut self) {
        let open = self.ped.open;
        self.ped = PedWindow {
            open,
            status: Some("New database".into()),
            ..Default::default()
        };
    }

    /// Read a `.ped` file into the window.
    pub fn ped_open(&mut self, path: &Path) -> bool {
        let read = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| parse_ped(&b).map_err(|e| e.to_string()));
        match read {
            Ok(db) => {
                let n = db.events.len();
                let open = self.ped.open;
                self.ped = PedWindow {
                    open,
                    db,
                    path: Some(path.to_path_buf()),
                    status: Some(format!("Opened {} ({n} events)", path.display())),
                    ..Default::default()
                };
                true
            }
            Err(e) => {
                self.ped.error = Some(format!("{}: {e}", path.display()));
                false
            }
        }
    }

    /// Write the database to `path` (it becomes the window's file).
    pub fn ped_save_as(&mut self, path: &Path) -> bool {
        match std::fs::write(path, write_ped(&self.ped.db)) {
            Ok(()) => {
                self.ped.path = Some(path.to_path_buf());
                self.ped.dirty = false;
                self.ped.error = None;
                self.ped.status = Some(format!("Saved {}", path.display()));
                true
            }
            Err(e) => {
                self.ped.error = Some(format!("{}: {e}", path.display()));
                false
            }
        }
    }

    /// Save to the window's file; false (and an error) when it has none.
    pub fn ped_save(&mut self) -> bool {
        match self.ped.path.clone() {
            Some(p) => self.ped_save_as(&p),
            None => {
                self.ped.error = Some("no file name yet: use Save As".into());
                false
            }
        }
    }

    /// File > Export Text File: the OpenPED text report.
    pub fn ped_export_text(&mut self, path: &Path) -> bool {
        let name = self.ped.name();
        let text = database::export_text(&self.ped.db, &name);
        match std::fs::write(path, text) {
            Ok(()) => {
                self.ped.error = None;
                self.ped.status = Some(format!("Exported {}", path.display()));
                true
            }
            Err(e) => {
                self.ped.error = Some(format!("{}: {e}", path.display()));
                false
            }
        }
    }

    /// Select event `id` and load it into the form.
    pub fn ped_select(&mut self, id: &str) {
        let Some(e) = self.ped.db.events.get(id).cloned() else {
            return;
        };
        self.ped.selected = Some(e.id.clone());
        self.ped.form = EventForm {
            original: Some(e.id.clone()),
            id: e.id,
            prob: format!("{:E}", e.probability),
            rate: e.lambda == Some('L'),
            kind: e.kind,
            dormant: e.modifier == Some('D'),
            desc: e.description,
        };
    }

    /// Apply: add the form's event or replace the one with the same id.
    pub fn ped_apply(&mut self) -> bool {
        let group = self
            .ped
            .db
            .events
            .get(self.ped.form.id.trim())
            .map(|e| e.group.clone())
            .unwrap_or_default();
        let ev = match form_event(&self.ped.form, group) {
            Ok(e) => e,
            Err(e) => {
                self.ped.error = Some(e);
                return false;
            }
        };
        if let Err(e) = database::validate_event(&ev) {
            self.ped.error = Some(e.to_string());
            return false;
        }
        let id = ev.id.clone();
        self.ped.db.events.insert(id.clone(), ev);
        self.ped.dirty = true;
        self.ped.error = None;
        self.ped.selected = Some(id.clone());
        self.ped.form.original = Some(id);
        true
    }

    /// Delete the selected event.
    pub fn ped_delete(&mut self) -> bool {
        let Some(id) = self.ped.selected.take() else {
            self.ped.error = Some("no event selected".into());
            return false;
        };
        let removed = self.ped.db.events.remove(&id).is_some();
        self.ped.dirty |= removed;
        self.ped.form = EventForm::default();
        removed
    }

    /// Copy the selected event (or all when none is selected) to the tree's database.
    pub fn ped_copy_to_tree(&mut self, overwrite: bool) -> Option<database::CopyOutcome> {
        let ids: Vec<String> = match &self.ped.selected {
            Some(id) => vec![id.clone()],
            None => self.ped.event_ids(),
        };
        let src = self.ped.db.clone();
        let out = self.edit_tree_db(|db| database::copy_events(&src, db, &ids, overwrite))?;
        self.ped.status = Some(format!(
            "Copied {} event(s) to the tree's database, skipped {}",
            out.copied.len(),
            out.skipped.len()
        ));
        Some(out)
    }

    /// Copy the tree's database events to this one (all of them, or the
    /// selected tree event's when a primary event is selected).
    pub fn ped_copy_from_tree(&mut self, overwrite: bool) -> Option<database::CopyOutcome> {
        let doc = self.doc.as_ref()?;
        let ids: Vec<String> = match self.selected_node() {
            Some(n) if doc.db.events.contains_key(&n.id) => vec![n.id.clone()],
            _ => doc.db.events.keys().cloned().collect(),
        };
        let src = doc.db.clone();
        match database::copy_events(&src, &mut self.ped.db, &ids, overwrite) {
            Ok(out) => {
                self.ped.dirty |= !out.copied.is_empty();
                self.ped.error = None;
                self.ped.status = Some(format!(
                    "Copied {} event(s) from the tree, skipped {}",
                    out.copied.len(),
                    out.skipped.len()
                ));
                Some(out)
            }
            Err(e) => {
                self.ped.error = Some(e.to_string());
                None
            }
        }
    }

    fn edit_tree_db<R>(
        &mut self,
        f: impl FnOnce(&mut PrimaryEventDb) -> ftar_core::edit::EditResult<R>,
    ) -> Option<R> {
        self.edit(|doc| f(&mut doc.db).map_err(|e| e.to_string()))
    }

    /// Edit > Link: the selected tree event takes the selected database
    /// event's id and type. When the types differ nothing changes and
    /// `ped.confirm_link` asks; call [`App::ped_link_confirmed`] to go on.
    pub fn ped_link(&mut self) -> Option<LinkOutcome> {
        let Some(path) = self.selected.clone() else {
            self.ped.error = Some("no tree event selected".into());
            return None;
        };
        let Some(id) = self.ped.selected.clone() else {
            self.ped.error = Some("no database event selected".into());
            return None;
        };
        self.link_with(&path, &id, false)
    }

    /// Link although the types differ.
    pub fn ped_link_confirmed(&mut self) -> Option<LinkOutcome> {
        let id = self.ped.confirm_link.take()?;
        let path = self.selected.clone()?;
        self.link_with(&path, &id, true)
    }

    fn link_with(&mut self, path: &[usize], id: &str, force: bool) -> Option<LinkOutcome> {
        let ped = self.ped.db.clone();
        let out = self.edit(|doc| {
            let o = database::link_node(&mut doc.tree, &ped, path, id, force)
                .map_err(|e| e.to_string())?;
            if o == LinkOutcome::Linked {
                // the tree's own database gets the event's data too
                database::copy_events(&ped, &mut doc.db, &[id.to_string()], true)
                    .map_err(|e| e.to_string())?;
            }
            Ok(o)
        })?;
        match out {
            LinkOutcome::TypesDiffer => self.ped.confirm_link = Some(id.to_string()),
            LinkOutcome::Linked => self.ped.status = Some(format!("Linked to {id}")),
        }
        Some(out)
    }

    /// The database window (draw once per frame).
    pub(crate) fn ped_window(&mut self, ctx: &egui::Context) {
        if !self.ped.open {
            return;
        }
        let mut open = true;
        let title = "Primary event database";
        egui::Window::new(title)
            .open(&mut open)
            .default_size([640.0, 420.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("New").clicked() {
                        self.ped_new();
                    }
                    if ui.button("Open...").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("Primary event database", &["ped"])
                            .pick_file()
                        {
                            self.ped_open(&p);
                        }
                    }
                    if ui.button("Save").clicked() {
                        if self.ped.path.is_some() {
                            self.ped_save();
                        } else {
                            self.ped_save_as_dialog();
                        }
                    }
                    if ui.button("Save As...").clicked() {
                        self.ped_save_as_dialog();
                    }
                    if ui.button("Export Text File...").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("Text files", &["txt"])
                            .set_file_name("primary_events.txt")
                            .save_file()
                        {
                            self.ped_export_text(&p);
                        }
                    }
                });
                let name = self.ped.name();
                ui.label(format!(
                    "Database: {}{}",
                    if name.is_empty() { "(unnamed)" } else { &name },
                    if self.ped.dirty { " *" } else { "" }
                ));
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.strong("Events");
                        egui::ScrollArea::vertical()
                            .id_salt("ped_list")
                            .max_height(260.0)
                            .show(ui, |ui| {
                                for id in self.ped.event_ids() {
                                    let sel = self.ped.selected.as_deref() == Some(&id);
                                    if ui.selectable_label(sel, &id).clicked() {
                                        self.ped_select(&id);
                                    }
                                }
                            });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        let l = ui.label("Event id");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.ped.form.id).desired_width(200.0),
                        )
                        .labelled_by(l.id);
                        let l = ui.label("Event probability");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.ped.form.prob)
                                .desired_width(120.0),
                        )
                        .labelled_by(l.id);
                        ui.checkbox(&mut self.ped.form.rate, "Event is a failure rate");
                        egui::ComboBox::from_label("Event type")
                            .selected_text(kind_label(self.ped.form.kind))
                            .show_ui(ui, |ui| {
                                for k in [
                                    EventType::Basic,
                                    EventType::External,
                                    EventType::Undeveloped,
                                    EventType::ConditionNotAnalysed,
                                    EventType::ConditionAnalysed,
                                ] {
                                    ui.selectable_value(&mut self.ped.form.kind, k, kind_label(k));
                                }
                            });
                        ui.checkbox(&mut self.ped.form.dormant, "Event is dormant");
                        let l = ui.label("Event description");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.ped.form.desc)
                                .desired_rows(2)
                                .desired_width(280.0),
                        )
                        .labelled_by(l.id);
                        ui.horizontal(|ui| {
                            if ui.button("Apply event").clicked() {
                                self.ped_apply();
                            }
                            if ui.button("Delete event").clicked() {
                                self.ped_delete();
                            }
                        });
                        ui.horizontal(|ui| {
                            let has_doc = self.doc.is_some();
                            if ui
                                .add_enabled(has_doc, egui::Button::new("Copy to tree database"))
                                .clicked()
                            {
                                self.ped_copy_to_tree(true);
                            }
                            if ui
                                .add_enabled(has_doc, egui::Button::new("Copy from tree database"))
                                .clicked()
                            {
                                self.ped_copy_from_tree(true);
                            }
                            if ui
                                .add_enabled(has_doc, egui::Button::new("Link tree event"))
                                .clicked()
                            {
                                self.ped_link();
                            }
                        });
                    });
                });
                if let Some(e) = &self.ped.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
                if let Some(s) = &self.ped.status {
                    ui.label(s);
                }
            });
        if let Some(id) = self.ped.confirm_link.clone() {
            egui::Window::new("Event types differ")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "The tree event and database event {id} have different types. Link anyway?"
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Link anyway").clicked() {
                            self.ped_link_confirmed();
                        }
                        if ui.button("Cancel link").clicked() {
                            self.ped.confirm_link = None;
                        }
                    });
                });
        }
        self.ped.open = open;
    }

    fn ped_save_as_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Primary event database", &["ped"])
            .set_file_name("events.ped")
            .save_file()
        {
            self.ped_save_as(&p);
        }
    }
}

fn kind_label(k: EventType) -> &'static str {
    match k {
        EventType::External => "External",
        EventType::Undeveloped => "Undeveloped",
        EventType::ConditionNotAnalysed => "Conditional (not analysed)",
        EventType::ConditionAnalysed => "Conditional (analysed)",
        _ => "Basic",
    }
}
