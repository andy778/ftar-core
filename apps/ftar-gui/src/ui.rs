//! egui front end: drawing and widgets only; all behaviour is in [`crate::App`].

use eframe::egui::{
    self, Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, Sense, Shape, Stroke, Vec2,
    ViewportCommand,
};
use ftar_core::montecarlo::Method;
use ftar_core::report::ReportKind;
use ftar_core::EventType;

use crate::app::{App, Pending, Tab};
use crate::layout;

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

impl App {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // keyboard shortcuts
        let (save, undo, redo, del, new, open, print) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::S),
                i.consume_key(Modifiers::COMMAND, Key::Z),
                i.consume_key(Modifiers::COMMAND, Key::Y),
                i.consume_key(Modifiers::NONE, Key::Delete),
                i.consume_key(Modifiers::COMMAND, Key::N),
                i.consume_key(Modifiers::COMMAND, Key::O),
                i.consume_key(Modifiers::COMMAND, Key::P),
            )
        });
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.quit {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.request_quit();
        }
        if self.quit {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let has_doc = self.doc.is_some();
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        self.request_new();
                        ui.close();
                    }
                    if ui.button("Open...").clicked() {
                        ui.close();
                        self.open_dialog();
                    }
                    ui.separator();
                    if ui.add_enabled(has_doc, egui::Button::new("Save")).clicked() {
                        ui.close();
                        self.save_or_save_as();
                    }
                    if ui
                        .add_enabled(has_doc, egui::Button::new("Save As..."))
                        .clicked()
                    {
                        ui.close();
                        self.save_as_dialog();
                    }
                    ui.separator();
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
                        .add_enabled(has_doc, egui::Button::new("Export image..."))
                        .clicked()
                    {
                        ui.close();
                        self.export_image_dialog();
                    }
                    if ui
                        .add_enabled(has_doc, egui::Button::new("Print..."))
                        .clicked()
                    {
                        ui.close();
                        self.print_now();
                    }
                    ui.checkbox(
                        &mut self.image_probabilities,
                        "Probabilities in images and prints",
                    );
                    ui.checkbox(&mut self.print_report, "Include the report when printing");
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.close();
                        self.request_quit();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui
                        .add_enabled(self.history.can_undo(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        ui.close();
                        self.undo();
                    }
                    if ui
                        .add_enabled(self.history.can_redo(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        ui.close();
                        self.redo();
                    }
                    ui.separator();
                    let can_delete = self.selected.as_ref().is_some_and(|p| !p.is_empty());
                    if ui
                        .add_enabled(can_delete, egui::Button::new("Delete selected"))
                        .clicked()
                    {
                        ui.close();
                        self.delete_selected();
                    }
                });
                ui.separator();
                if ui
                    .add_enabled(self.history.can_undo(), egui::Button::new("Undo"))
                    .clicked()
                    || (undo && self.history.can_undo())
                {
                    self.undo();
                }
                if ui
                    .add_enabled(self.history.can_redo(), egui::Button::new("Redo"))
                    .clicked()
                    || (redo && self.history.can_redo())
                {
                    self.redo();
                }
                ui.separator();
                if ui.button("Fit").clicked() {
                    self.fit_next = true;
                }
                ui.label(self.title());
                if has_doc {
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
                if save && has_doc {
                    self.save_or_save_as();
                }
                if new {
                    self.request_new();
                }
                if open {
                    self.open_dialog();
                }
                if print && has_doc {
                    self.print_now();
                }
                if del && self.selected.as_ref().is_some_and(|p| !p.is_empty()) {
                    self.delete_selected();
                }
            });
        });
    }

    fn export_report_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Text report", &["txt"])
            .set_file_name(self.default_report_name())
            .save_file()
        {
            let _ = self.export_report(&p);
        }
    }

    fn export_image_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .add_filter("SVG drawing", &["svg"])
            .set_file_name(self.default_image_name())
            .save_file()
        {
            let _ = self.export_image(&p);
        }
    }

    /// Open the browser's print dialog for the tree (and report).
    fn print_now(&mut self) {
        if let Ok(path) = self.prepare_print() {
            if let Err(e) = open_with_default_app(&path) {
                self.error = Some(format!("could not open the print page: {e}"));
            }
        }
    }

    fn export_modes_csv_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .set_file_name(self.default_modes_csv_name())
            .save_file()
        {
            let _ = self.export_modes_csv(&p);
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("OpenFTA tree", &["fta"])
            .pick_file()
        {
            self.request_open(p);
        }
    }

    fn save_or_save_as(&mut self) {
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
        match rfd::FileDialog::new()
            .add_filter("OpenFTA tree", &["fta"])
            .set_file_name(name)
            .save_file()
        {
            Some(p) => self.save_as(p).is_ok(),
            None => false,
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
            ui.horizontal(|ui| {
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
                        format!("{n} structural problem(s): see Validate"),
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
                    ui.horizontal(|ui| {
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
                    if ui.button("Move earlier").clicked() {
                        self.move_selected(-1);
                    }
                    if ui.button("Move later").clicked() {
                        self.move_selected(1);
                    }
                }
            });
            if (node.kind.is_primary() || node.kind == EventType::ConditionAnalysed)
                && !node.id.is_empty()
            {
                ui.horizontal_wrapped(|ui| {
                    ui.label(self.event_origin(&node.id));
                    let l = ui.label("probability");
                    let pr = ui
                        .add(egui::TextEdit::singleline(&mut self.buf.prob).desired_width(90.0))
                        .labelled_by(l.id);
                    let l = ui.label("event description");
                    let dr = ui
                        .add(egui::TextEdit::singleline(&mut self.buf.desc).desired_width(240.0))
                        .labelled_by(l.id);
                    let rr = ui.checkbox(&mut self.buf.rate, "failure rate");
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

fn gate_label(k: EventType) -> &'static str {
    use EventType::*;
    match k {
        And => "AND",
        Or => "OR",
        Xor => "XOR",
        PriorityAnd => "PAND",
        Inhibit => "INH",
        _ => "",
    }
}

const NODE_W: f32 = 96.0;
const NODE_H: f32 = 56.0;
const GAP_X: f32 = 104.0;
const GAP_Y: f32 = 96.0;
/// Room under a node for its caption, in tree units.
const CAPTION: f32 = 24.0;
/// Empty border around the fitted tree, in screen pixels.
const MARGIN: f32 = 12.0;

/// Size of the whole drawing (nodes and captions) in tree units.
fn content_size(max_x: f32, max_y: f32) -> Vec2 {
    Vec2::new(
        (max_x * GAP_X + NODE_W).max(1.0),
        (max_y * GAP_Y + NODE_H + CAPTION).max(1.0),
    )
}

/// Zoom and pan that centre the whole tree in a canvas of `size`, with a margin.
pub(crate) fn fit(size: Vec2, max_x: f32, max_y: f32) -> (f32, Vec2) {
    let c = content_size(max_x, max_y);
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

impl App {
    /// Where the whole drawing (nodes and captions) currently is on screen.
    pub fn content_bounds(&self) -> Option<Rect> {
        let doc = self.doc.as_ref()?;
        let (mx, my) = layout::layout(&doc.tree.root)
            .iter()
            .fold((0.0f32, 0.0f32), |a, p| (a.0.max(p.x), a.1.max(p.y)));
        let min = self.canvas.min + self.pan;
        Some(Rect::from_min_size(min, content_size(mx, my) * self.zoom))
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        self.top_bar(ui);
        self.unsaved_prompt(&ctx);

        egui::Panel::right("analysis")
            .default_size(420.0)
            .show(ui, |ui| {
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
                    let l = ui.label("max order (0 = all)");
                    ui.add(egui::DragValue::new(&mut self.order).range(0..=50))
                        .labelled_by(l.id);
                });
                match self.tab {
                    Tab::Probability => {
                        ui.horizontal(|ui| {
                            let l = ui.label("terms");
                            ui.add(egui::DragValue::new(&mut self.terms).range(1..=12))
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
                                    ui.selectable_value(
                                        &mut self.mc_method,
                                        Method::Direct,
                                        "direct",
                                    );
                                });
                            let l = ui.label("seed");
                            ui.add(egui::DragValue::new(&mut self.seed))
                                .labelled_by(l.id);
                        });
                    }
                    Tab::CutSets => {}
                }
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
                                ctx.copy_text(t);
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
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    Tab::CutSets => {
                        if let Some(cs) = &self.cut_sets {
                            ui.label(format!("{} minimal cut sets", cs.len()));
                            for (i, s) in cs.iter().enumerate() {
                                ui.monospace(format!(
                                    "{:>4}  {}",
                                    i + 1,
                                    s.iter().cloned().collect::<Vec<_>>().join(" ")
                                ));
                            }
                        }
                    }
                    Tab::Probability => {
                        if let Some(r) = &self.prob {
                            ui.strong("Cut set probabilities");
                            for (i, (cs, p)) in r.cut_sets.iter().enumerate() {
                                ui.monospace(format!(
                                    "{:>3} {:<28} {:.3E}",
                                    i + 1,
                                    cs.iter().cloned().collect::<Vec<_>>().join(" "),
                                    p
                                ));
                            }
                            ui.add_space(6.0);
                            ui.strong("Top event");
                            for (k, (t, s)) in r.terms.iter().zip(&r.partial_sums).enumerate() {
                                let b = if k % 2 == 0 { "upper" } else { "lower" };
                                ui.monospace(format!(
                                    "{:>2} terms {:+.3E} = {:.6E} ({b})",
                                    k + 1,
                                    t,
                                    s
                                ));
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
                                Some(b) => format!(
                                    "P(top)          < {b:.2E} (95%, no failures simulated)"
                                ),
                                None => format!(
                                    "P(top)          {:.4E} +/- {:.2E}",
                                    r.probability, r.std_error
                                ),
                            });
                            ui.monospace(format!(
                                "P(any failure)  {:.4E}",
                                r.any_component_failure
                            ));
                            ui.add_space(6.0);
                            ui.strong(format!("Failure modes ({})", r.modes.len()));
                            for (cs, n) in r.modes.iter().take(40) {
                                ui.monospace(format!(
                                    "{:>8}  {:.3E}  {}",
                                    n,
                                    r.bas_prob * *n as f64 / r.trials.max(1) as f64,
                                    cs.iter().cloned().collect::<Vec<_>>().join(" ")
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
            });

        self.status_bar(ui);
        self.details_panel(ui);

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(doc) = &self.doc else {
                ui.centered_and_justified(|ui| {
                    ui.label("Open a .fta file (Open...), or start a New tree.")
                });
                return;
            };
            let placed = layout::layout(&doc.tree.root);
            let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = resp.rect;

            // world -> screen
            let (max_x, max_y) = placed
                .iter()
                .fold((0.0f32, 0.0f32), |a, p| (a.0.max(p.x), a.1.max(p.y)));
            let key = (rect.size(), max_x, max_y);
            if self.fit_next || (self.auto_fit && self.fit_key != Some(key)) {
                let (z, pan) = fit(rect.size(), max_x, max_y);
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
                let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
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
            let origin = rect.min + self.pan;
            let z = self.zoom;
            let centre = |p: &layout::Placed| {
                origin + Vec2::new(p.x * GAP_X + NODE_W / 2.0, p.y * GAP_Y + NODE_H / 2.0) * z
            };

            let dark = ctx.global_style().visuals.dark_mode;
            let line = Stroke::new(
                1.5 * z.max(0.5),
                if dark {
                    Color32::GRAY
                } else {
                    Color32::DARK_GRAY
                },
            );
            let text_col = if dark { Color32::WHITE } else { Color32::BLACK };

            // edges (elbow connectors)
            for p in &placed {
                if let Some(pi) = p.parent {
                    let a = centre(&placed[pi]) + Vec2::new(0.0, NODE_H / 2.0 * z);
                    let b = centre(p) - Vec2::new(0.0, NODE_H / 2.0 * z);
                    let mid = (a.y + b.y) / 2.0;
                    painter.add(Shape::line(
                        vec![a, Pos2::new(a.x, mid), Pos2::new(b.x, mid), b],
                        line,
                    ));
                }
            }

            // nodes
            let mut clicked: Option<Vec<usize>> = None;
            let mut hit_any = false;
            for p in placed.iter() {
                let c = centre(p);
                let r = Rect::from_center_size(c, Vec2::new(NODE_W, NODE_H) * z);
                if !r.intersects(rect) {
                    continue;
                }
                let sel = self.selected.as_ref() == Some(&p.path);
                let (fill, label) = node_style(p.node, dark);
                let stroke = if sel {
                    Stroke::new(3.0, Color32::from_rgb(255, 170, 0))
                } else {
                    line
                };
                let n = p.node;
                match n.kind {
                    EventType::Basic | EventType::Dormant => {
                        let rad = (NODE_H / 2.0 - 6.0) * z;
                        painter.circle(c + Vec2::new(0.0, 0.0), rad, fill, stroke);
                    }
                    EventType::Undeveloped => {
                        let (w, h) = (NODE_H * 0.6 * z, NODE_H * 0.45 * z);
                        painter.add(Shape::convex_polygon(
                            vec![
                                c + Vec2::new(0.0, -h),
                                c + Vec2::new(w, 0.0),
                                c + Vec2::new(0.0, h),
                                c + Vec2::new(-w, 0.0),
                            ],
                            fill,
                            stroke,
                        ));
                    }
                    EventType::External => {
                        let (w, h) = (NODE_H * 0.5 * z, NODE_H * 0.45 * z);
                        painter.add(Shape::convex_polygon(
                            vec![
                                c + Vec2::new(0.0, -h),
                                c + Vec2::new(w, 0.0),
                                c + Vec2::new(w, h),
                                c + Vec2::new(-w, h),
                                c + Vec2::new(-w, 0.0),
                            ],
                            fill,
                            stroke,
                        ));
                    }
                    k if k.is_gate() => {
                        let rr =
                            Rect::from_center_size(c, Vec2::new(NODE_H * 0.9, NODE_H * 0.55) * z);
                        painter.rect(rr, 14.0 * z, fill, stroke, egui::StrokeKind::Middle);
                    }
                    _ => {
                        let rr = Rect::from_center_size(c, Vec2::new(NODE_W, NODE_H * 0.8) * z);
                        painter.rect(rr, 3.0 * z, fill, stroke, egui::StrokeKind::Middle);
                    }
                }
                if z > 0.22 {
                    let fs = (12.0 * z).clamp(7.0, 22.0);
                    painter.text(
                        c,
                        Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(fs),
                        text_col,
                    );
                    // text for intermediate events / ids under basic events
                    let cap = match n.kind {
                        EventType::Intermediate => n.text.as_str(),
                        _ => "",
                    };
                    if !cap.is_empty() {
                        let fs = (10.0 * z).clamp(7.0, 16.0);
                        // keep captions inside their column so neighbours don't overlap
                        let max = ((GAP_X * z - 4.0) / (fs * 0.5)).floor().max(4.0) as usize;
                        let short: String = if cap.chars().count() > max {
                            cap.chars()
                                .take(max - 1)
                                .chain(std::iter::once('…'))
                                .collect()
                        } else {
                            cap.to_string()
                        };
                        painter.text(
                            c + Vec2::new(0.0, NODE_H * 0.5 * z + 2.0),
                            Align2::CENTER_TOP,
                            short,
                            FontId::proportional((10.0 * z).clamp(7.0, 16.0)),
                            text_col,
                        );
                    }
                }
                if resp.clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if r.contains(pos) {
                            clicked = Some(p.path.clone());
                            hit_any = true;
                        }
                    }
                }
            }
            if resp.clicked() {
                let _ = hit_any;
                self.selected = clicked;
                self.version += 1;
            }
        });
    }
}

fn node_style(n: &ftar_core::Node, dark: bool) -> (Color32, String) {
    use EventType::*;
    let (r, g, b) = match n.kind {
        Basic => (70, 130, 200),
        Dormant => (110, 110, 200),
        Undeveloped => (200, 160, 60),
        External => (90, 170, 120),
        And => (200, 90, 90),
        Or => (90, 150, 90),
        Xor | PriorityAnd | Inhibit => (170, 100, 170),
        Intermediate => (120, 120, 120),
        _ => (150, 150, 150),
    };
    let fill = if dark {
        Color32::from_rgb(r / 2, g / 2, b / 2)
    } else {
        Color32::from_rgb(r / 2 + 128, g / 2 + 128, b / 2 + 128)
    };
    let label = if n.kind.is_gate() {
        gate_label(n.kind).to_string()
    } else {
        n.id.clone()
    };
    (fill, label)
}

/// Open a file with the system's default application (the browser, for HTML).
fn open_with_default_app(path: &std::path::Path) -> std::io::Result<()> {
    use std::process::Command;
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
