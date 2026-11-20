//! Application state and operations. No egui here: everything the user can do
//! is a method on [`App`], so it can be tested headlessly and reused by the UI.

use std::path::{Path, PathBuf};

use eframe::egui::Vec2;
use ftar_core::analysis::{
    minimal_cut_sets_limited, probability_report, CutSet, ProbabilityOptions, ProbabilityReport,
};
use ftar_core::edit::{self, History, Issue};
use ftar_core::montecarlo::{simulate, Method, MonteCarloOptions, MonteCarloReport};
use ftar_core::project::Document;
use ftar_core::report::{self, ReportKind, ReportParams};
use ftar_core::write::{write_fta_with, write_ped, LineEnding};
use ftar_core::{EventType, PrimaryEvent, PrimaryEventDb, Tree};
use ftar_render::{print_html, svg_to_png, tree_svg, PrintOptions, SvgOptions};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Tab {
    CutSets,
    Probability,
    MonteCarlo,
}

/// What the user asked for while there were unsaved changes.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    New,
    Open(PathBuf),
    Quit,
}

type Snapshot = (Tree, PrimaryEventDb);

/// Text-field buffers of the edit panel (kept across frames).
#[derive(Default)]
pub struct Buffers {
    pub key: Option<(Option<Vec<usize>>, u64)>,
    pub id: String,
    pub text: String,
    pub prob: String,
    pub desc: String,
    pub rate: bool,
    /// Name of the primary event database the tree refers to.
    pub database: String,
}

pub struct App {
    pub doc: Option<Document>,
    pub history: History<Snapshot>,
    pub dirty: bool,
    /// Bumped on every change to the document; the UI uses it to refresh its text buffers.
    pub version: u64,
    pub error: Option<String>,
    pub status: Option<String>,
    pub selected: Option<Vec<usize>>,
    pub pending: Option<Pending>,
    pub quit: bool,
    pub add_kind: EventType,
    pub report_kind: ReportKind,
    pub buf: Buffers,
    // view
    pub zoom: f32,
    pub pan: Vec2,
    pub fit_next: bool,
    /// Keep the tree fitted while the canvas or the tree changes size, until the user pans or zooms.
    pub auto_fit: bool,
    pub(crate) fit_key: Option<(Vec2, f32, f32)>,
    /// Canvas rectangle of the last frame.
    pub canvas: eframe::egui::Rect,
    // analysis
    pub tab: Tab,
    pub order: usize,
    pub terms: usize,
    pub unit_time: f64,
    pub trials: u64,
    pub seed: u64,
    pub mc_method: Method,
    /// PNG export scale (1.0 = the drawing's natural size).
    pub image_scale: f32,
    /// Show event probabilities in image exports and prints.
    pub image_probabilities: bool,
    /// Append the selected text report to printouts.
    pub print_report: bool,
    pub cut_sets: Option<Vec<CutSet>>,
    pub prob: Option<ProbabilityReport>,
    pub mc: Option<MonteCarloReport>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            doc: None,
            history: History::new(),
            dirty: false,
            version: 0,
            error: None,
            status: None,
            selected: None,
            pending: None,
            quit: false,
            add_kind: EventType::Basic,
            report_kind: ReportKind::CutSets,
            buf: Buffers::default(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit_next: true,
            auto_fit: true,
            fit_key: None,
            canvas: eframe::egui::Rect::ZERO,
            tab: Tab::CutSets,
            order: 0,
            terms: ftar_core::analysis::DEFAULT_TERMS,
            unit_time: 1.0,
            trials: 100_000,
            seed: 1,
            mc_method: Method::FirstFailure,
            image_scale: 2.0,
            image_probabilities: true,
            print_report: true,
            cut_sets: None,
            prob: None,
            mc: None,
        }
    }
}

fn accepts_children(k: EventType) -> bool {
    k.is_gate() || k == EventType::Intermediate
}

impl App {
    // ------------------------------------------------------------ documents

    fn reset_view_state(&mut self) {
        self.history.clear();
        self.dirty = false;
        self.version += 1;
        self.selected = None;
        self.cut_sets = None;
        self.prob = None;
        self.mc = None;
        self.fit_next = true;
        self.error = None;
    }

    /// Start a new empty tree (asks first when there are unsaved changes).
    pub fn request_new(&mut self) {
        if self.dirty {
            self.pending = Some(Pending::New);
        } else {
            self.new_doc();
        }
    }

    pub fn new_doc(&mut self) {
        self.doc = Some(Document::default());
        self.reset_view_state();
        self.status = Some("New tree".into());
    }

    /// Open a file (asks first when there are unsaved changes).
    pub fn request_open(&mut self, path: PathBuf) {
        if self.dirty {
            self.pending = Some(Pending::Open(path));
        } else {
            self.open(path);
        }
    }

    pub fn request_quit(&mut self) {
        if self.dirty {
            self.pending = Some(Pending::Quit);
        } else {
            self.quit = true;
        }
    }

    pub fn open(&mut self, path: PathBuf) {
        match Document::load(&path) {
            Ok(d) => {
                self.doc = Some(d);
                self.reset_view_state();
                self.status = Some(format!("Opened {}", path.display()));
            }
            Err(e) => self.error = Some(format!("{}: {e}", path.display())),
        }
    }

    /// Resolve a pending prompt: `save` first if asked, then do what was requested.
    pub fn resolve_pending(&mut self, save_first: bool) {
        let Some(p) = self.pending.take() else { return };
        if save_first && self.save().is_err() {
            // keep the prompt's intent so the user can pick a file and retry
            self.pending = Some(p);
            return;
        }
        self.dirty = false;
        match p {
            Pending::New => self.new_doc(),
            Pending::Open(path) => self.open(path),
            Pending::Quit => self.quit = true,
        }
    }

    pub fn cancel_pending(&mut self) {
        self.pending = None;
    }

    pub fn title(&self) -> String {
        let name = self
            .doc
            .as_ref()
            .map(|d| {
                d.path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "untitled".into())
            })
            .unwrap_or_default();
        format!(
            "{}{}",
            name,
            if self.dirty && !name.is_empty() {
                " *"
            } else {
                ""
            }
        )
    }

    // ------------------------------------------------------------- saving

    /// Save to the document's path. Fails if it has none (use [`App::save_as`]).
    pub fn save(&mut self) -> Result<(), String> {
        let path = self
            .doc
            .as_ref()
            .and_then(|d| d.path.clone())
            .ok_or_else(|| "no file name yet: use Save As".to_string())?;
        self.save_as(path)
    }

    /// Write the tree as `.fta` and, if it has events, its database as `.ped`
    /// next to it. A tree without a database name gets `<stem>.ped`.
    pub fn save_as(&mut self, path: PathBuf) -> Result<(), String> {
        let Some(doc) = self.doc.as_mut() else {
            return Err("nothing to save".into());
        };
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "tree".into());
        if doc.tree.db_name.is_none() && !doc.db.events.is_empty() {
            doc.tree.db_name = Some(format!("{stem}.ped"));
        }
        let res = (|| -> Result<Vec<PathBuf>, String> {
            let mut written = vec![path.clone()];
            std::fs::write(&path, write_fta_with(&doc.tree, LineEnding::CrLf))
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if let Some(name) = &doc.tree.db_name {
                let ped = ftar_core::project::ped_save_path(
                    path.parent().unwrap_or_else(|| Path::new(".")),
                    name,
                );
                std::fs::write(&ped, write_ped(&doc.db))
                    .map_err(|e| format!("{}: {e}", ped.display()))?;
                written.push(ped);
            }
            Ok(written)
        })();
        match res {
            Ok(w) => {
                doc.path = Some(path);
                self.dirty = false;
                self.error = None;
                self.status = Some(format!(
                    "Saved {}",
                    w.iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(" and ")
                ));
                Ok(())
            }
            Err(e) => {
                self.error = Some(e.clone());
                Err(e)
            }
        }
    }

    // ------------------------------------------------------------ editing

    /// Run an edit on the document. On success it is recorded for undo; on
    /// failure the document is restored and the error shown.
    fn edit<R>(&mut self, f: impl FnOnce(&mut Document) -> Result<R, String>) -> Option<R> {
        let doc = self.doc.as_mut()?;
        let before: Snapshot = (doc.tree.clone(), doc.db.clone());
        match f(doc) {
            Ok(r) => {
                self.history.record(before);
                self.dirty = true;
                self.version += 1;
                self.error = None;
                self.cut_sets = None;
                self.prob = None;
                self.mc = None;
                Some(r)
            }
            Err(e) => {
                doc.tree = before.0;
                doc.db = before.1;
                self.error = Some(e);
                None
            }
        }
    }

    pub fn select(&mut self, path: Option<Vec<usize>>) {
        self.selected = path;
        self.version += 1;
    }

    pub fn selected_node(&self) -> Option<&ftar_core::Node> {
        let doc = self.doc.as_ref()?;
        edit::node_at(&doc.tree, self.selected.as_ref()?)
    }

    fn unique_id(doc: &Document) -> String {
        let used: Vec<&str> = doc.tree.nodes().iter().map(|n| n.id.as_str()).collect();
        (1..)
            .map(|i| format!("EVENT_{i}"))
            .find(|id| !used.contains(&id.as_str()) && !doc.db.events.contains_key(id))
            .unwrap_or_default()
    }

    /// Add a child of `kind` to the selected gate/intermediate event (the
    /// root when nothing is selected) and select it. New primary events get a
    /// database entry with probability 0.
    pub fn add_child(&mut self, kind: EventType) {
        let parent = self.selected.clone().unwrap_or_default();
        let new_path = self.edit(|doc| {
            let p = edit::node_at(&doc.tree, &parent).ok_or("no such node")?;
            if !accepts_children(p.kind) {
                return Err("select a gate or an intermediate event to add to".to_string());
            }
            let id = if kind.is_primary() || kind == EventType::ConditionAnalysed {
                Self::unique_id(doc)
            } else {
                String::new()
            };
            let mut node = edit::new_node(kind, &id);
            if kind == EventType::Intermediate {
                node.text = "New event".into();
            }
            if (kind.is_primary() || kind == EventType::ConditionAnalysed) && !id.is_empty() {
                doc.db.events.insert(
                    id.clone(),
                    PrimaryEvent {
                        id: id.clone(),
                        group: String::new(),
                        kind,
                        modifier: None,
                        description: String::new(),
                        probability: 0.0,
                        lambda: None,
                    },
                );
            }
            edit::append_child(&mut doc.tree, &parent, node).map_err(|e| e.to_string())
        });
        if let Some(p) = new_path {
            self.select(Some(p));
        }
    }

    pub fn delete_selected(&mut self) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        if self
            .edit(|doc| {
                edit::delete_subtree(&mut doc.tree, &path)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
            .is_some()
        {
            let mut parent = path;
            parent.pop();
            self.select(Some(parent));
        }
    }

    pub fn set_kind(&mut self, kind: EventType) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        self.edit(|doc| {
            edit::change_type(&mut doc.tree, &path, kind, None).map_err(|e| e.to_string())?;
            // The database type overrides the tree's on load, so keep them in step.
            let id = edit::node_at(&doc.tree, &path)
                .map(|n| n.id.clone())
                .unwrap_or_default();
            if let Some(ev) = doc.db.events.get_mut(&id) {
                if kind.is_primary() || kind == EventType::ConditionAnalysed {
                    ev.kind = kind;
                }
            }
            Ok(())
        });
    }

    /// Rename the selected node. The database entry follows the new name
    /// (copied, and the old one dropped when no other node uses it).
    pub fn set_id(&mut self, id: &str) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let id = id.to_string();
        self.edit(|doc| {
            let old = edit::node_at(&doc.tree, &path)
                .ok_or("no such node")?
                .id
                .clone();
            if old == id {
                return Ok(());
            }
            edit::set_id(&mut doc.tree, &path, &id).map_err(|e| e.to_string())?;
            if !old.is_empty() && !id.is_empty() {
                if let Some(mut ev) = doc.db.events.get(&old).cloned() {
                    let still_used = doc.tree.nodes().iter().any(|n| n.id == old);
                    if !doc.db.events.contains_key(&id) {
                        ev.id = id.clone();
                        doc.db.events.insert(id.clone(), ev);
                    }
                    if !still_used {
                        doc.db.events.remove(&old);
                    }
                }
            }
            Ok(())
        });
    }

    /// Attach the tree to the primary event database file `name` (empty
    /// detaches). It may be a path relative to the tree, e.g.
    /// `../library/sensors.ped`, so several trees share one database. When the
    /// file exists its events are read in, so a later save keeps them.
    pub fn set_database(&mut self, name: &str) {
        let name = name.to_string();
        let loaded = self.edit(|doc| {
            edit::set_db_name(&mut doc.tree, Some(&name)).map_err(|e| e.to_string())?;
            let Some(dir) = doc.path.as_deref().and_then(Path::parent) else {
                return Ok(0);
            };
            ftar_core::project::load_database_events(
                &mut doc.tree,
                &mut doc.db,
                dir,
                &ftar_core::project::LoadOptions::default(),
            )
            .map_err(|e| e.to_string())
        });
        if let Some(n) = loaded.filter(|n| *n > 0) {
            self.status = Some(format!("Read {n} events from {name}"));
        }
    }

    /// Text shown next to an event: where its data comes from.
    pub fn event_origin(&self, id: &str) -> String {
        let Some(doc) = self.doc.as_ref() else {
            return String::new();
        };
        let db = doc.tree.db_name.as_deref().filter(|n| !n.is_empty());
        match (doc.db.events.contains_key(id), db) {
            (true, Some(n)) => format!("Data from database {n}"),
            (true, None) => "Data from the primary event database (not saved yet)".into(),
            (false, Some(n)) => format!("Not in database {n} yet - edit a value to add it"),
            (false, None) => "Not in a database yet - edit a value to add it".into(),
        }
    }

    pub fn set_text(&mut self, text: &str) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let text = text.to_string();
        self.edit(|doc| edit::set_text(&mut doc.tree, &path, &text).map_err(|e| e.to_string()));
    }

    /// Move the selected node among its siblings (`-1` earlier, `+1` later).
    pub fn move_selected(&mut self, delta: i32) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let Some(&idx) = path.last() else { return };
        let new = if delta < 0 {
            idx.saturating_sub(1)
        } else {
            idx + 1
        };
        if new == idx {
            return;
        }
        if let Some(p) =
            self.edit(|doc| edit::reorder(&mut doc.tree, &path, new).map_err(|e| e.to_string()))
        {
            self.select(Some(p));
        }
    }

    /// Create or update the database entry `id`.
    pub fn set_event(&mut self, id: &str, probability: f64, description: &str, rate: bool) {
        if !(probability.is_finite() && probability >= 0.0) || (!rate && probability > 1.0) {
            self.error = Some("probability must be within 0..1 (or a rate >= 0)".into());
            return;
        }
        let (id, description) = (id.to_string(), description.to_string());
        self.edit(|doc| {
            let kind = doc
                .tree
                .nodes()
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.kind)
                .unwrap_or(EventType::Basic);
            let e = doc.db.events.entry(id.clone()).or_insert(PrimaryEvent {
                id: id.clone(),
                group: String::new(),
                kind,
                modifier: None,
                description: String::new(),
                probability: 0.0,
                lambda: None,
            });
            e.probability = probability;
            e.description = description;
            e.lambda = rate.then_some('L');
            Ok(())
        });
    }

    pub fn undo(&mut self) {
        if let Some(doc) = self.doc.as_mut() {
            if let Some((t, d)) = self.history.undo(&(doc.tree.clone(), doc.db.clone())) {
                doc.tree = t;
                doc.db = d;
                self.after_history_move();
            }
        }
    }

    pub fn redo(&mut self) {
        if let Some(doc) = self.doc.as_mut() {
            if let Some((t, d)) = self.history.redo(&(doc.tree.clone(), doc.db.clone())) {
                doc.tree = t;
                doc.db = d;
                self.after_history_move();
            }
        }
    }

    fn after_history_move(&mut self) {
        self.dirty = true;
        self.version += 1;
        self.cut_sets = None;
        self.prob = None;
        self.mc = None;
        // the selected path may no longer exist
        if let (Some(doc), Some(p)) = (&self.doc, &self.selected) {
            if edit::node_at(&doc.tree, p).is_none() {
                self.selected = None;
            }
        }
    }

    pub fn issues(&self) -> Vec<Issue> {
        self.doc
            .as_ref()
            .map(|d| edit::validate(&d.tree))
            .unwrap_or_default()
    }

    // ------------------------------------------------- images and printing

    fn svg_options(&self) -> SvgOptions {
        SvgOptions {
            title: self.tree_name(),
            probabilities: self.image_probabilities,
            background: true,
        }
    }

    /// Draw the tree as shown in the editor (transfer gates stay as symbols) as SVG.
    pub fn svg_text(&self) -> Result<String, String> {
        let doc = self.doc.as_ref().ok_or("no tree open")?;
        Ok(tree_svg(&doc.tree, &doc.db, &self.svg_options()))
    }

    pub fn default_image_name(&self) -> String {
        format!("{}.png", self.tree_name())
    }

    /// Export the tree as an image; the format follows the extension (`.png` or `.svg`).
    pub fn export_image(&mut self, path: &Path) -> Result<(), String> {
        let res = (|| -> Result<(), String> {
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let svg = self.svg_text()?;
            let bytes = match ext.as_str() {
                "svg" => svg.into_bytes(),
                "png" => svg_to_png(&svg, self.image_scale).map_err(|e| e.to_string())?,
                _ => return Err("image files must end in .png or .svg".into()),
            };
            std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
        })();
        match &res {
            Ok(()) => {
                self.error = None;
                self.status = Some(format!("Exported {}", path.display()));
            }
            Err(e) => self.error = Some(e.clone()),
        }
        res
    }

    /// The printable page (tree, note and, optionally, the selected report).
    pub fn print_page(&self, auto_print: bool) -> Result<String, String> {
        let doc = self.doc.as_ref().ok_or("no tree open")?;
        let report = if self.print_report {
            // a failing analysis (e.g. an unfinished tree) must not stop the tree from printing
            self.report_text(self.report_kind, &Self::now_text()).ok()
        } else {
            None
        };
        Ok(print_html(
            &doc.tree,
            &doc.db,
            &PrintOptions {
                title: self.tree_name(),
                probabilities: self.image_probabilities,
                report,
                auto_print,
            },
        ))
    }

    /// Write the printable page to a temporary file that opens the print dialog
    /// when loaded in a browser. The caller opens the returned path.
    pub fn prepare_print(&mut self) -> Result<PathBuf, String> {
        let res = self.print_page(true).and_then(|html| {
            let path = std::env::temp_dir().join(format!(
                "ftar_print_{}_{}.html",
                std::process::id(),
                self.version
            ));
            std::fs::write(&path, html)
                .map(|_| path.clone())
                .map_err(|e| format!("{}: {e}", path.display()))
        });
        match &res {
            Ok(p) => {
                self.error = None;
                self.status = Some(format!("Print page written to {}", p.display()));
            }
            Err(e) => self.error = Some(e.clone()),
        }
        res
    }

    // ------------------------------------------------------------ reports

    /// Monte Carlo settings as chosen on screen.
    pub fn mc_options(&self) -> MonteCarloOptions {
        MonteCarloOptions {
            trials: self.trials,
            unit_time: self.unit_time,
            seed: self.seed,
            method: self.mc_method,
        }
    }

    /// The report kind that matches an analysis tab.
    pub fn kind_for_tab(tab: Tab) -> ReportKind {
        match tab {
            Tab::CutSets => ReportKind::CutSets,
            Tab::Probability => ReportKind::Probability,
            Tab::MonteCarlo => ReportKind::MonteCarlo,
        }
    }

    /// Failure modes of the last Monte Carlo run as CSV:
    /// `rank,failures,probability,events` (events separated by spaces).
    pub fn modes_csv(&self) -> Result<String, String> {
        let r = self
            .mc
            .as_ref()
            .ok_or("run the Monte Carlo analysis first")?;
        let mut out = String::from("rank,failures,probability,events\r\n");
        for (i, (cs, n)) in r.modes.iter().enumerate() {
            let events = cs.iter().cloned().collect::<Vec<_>>().join(" ");
            out.push_str(&format!(
                "{},{},{:e},{}\r\n",
                i + 1,
                n,
                r.bas_prob * *n as f64 / r.trials.max(1) as f64,
                csv_field(&events)
            ));
        }
        Ok(out)
    }

    pub fn export_modes_csv(&mut self, path: &Path) -> Result<(), String> {
        let res = self.modes_csv().and_then(|text| {
            std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
        });
        match &res {
            Ok(()) => {
                self.error = None;
                self.status = Some(format!("Exported {}", path.display()));
            }
            Err(e) => self.error = Some(e.clone()),
        }
        res
    }

    pub fn default_modes_csv_name(&self) -> String {
        format!("{}_failure_modes.csv", self.tree_name())
    }

    /// File name with extension, as OpenFTA prints it in report headers.
    pub fn file_name(&self) -> String {
        self.doc
            .as_ref()
            .and_then(|d| d.path.as_ref())
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled".into())
    }

    /// Name used in default file names.
    pub fn tree_name(&self) -> String {
        self.doc
            .as_ref()
            .and_then(|d| d.path.as_ref())
            .and_then(|p| p.file_stem())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled".into())
    }

    pub fn default_report_name(&self) -> String {
        format!("{}_{}.txt", self.tree_name(), self.report_kind.slug())
    }

    /// The OpenFTA-style text report for `kind`, using the analysis settings
    /// on screen and the current (edited) tree. `time` is the header timestamp.
    pub fn report_text(&self, kind: ReportKind, time: &str) -> Result<String, String> {
        let doc = self.doc.as_ref().ok_or("no tree open")?;
        let p = doc.analysis_project().map_err(|e| e.to_string())?;
        let params = ReportParams {
            max_order: self.max_order(),
            terms: self.terms,
            monte_carlo: self.mc_options(),
        };
        report::generate(kind, &p.tree, &p.db, &self.file_name(), time, &params)
            .map_err(|e| e.to_string())
    }

    /// Timestamp for report headers, in the style of C's `ctime`.
    pub fn now_text() -> String {
        chrono::Local::now()
            .format("%a %b %e %H:%M:%S %Y")
            .to_string()
    }

    /// Write the selected report to `path`.
    pub fn export_report(&mut self, path: &Path) -> Result<(), String> {
        let res = self
            .report_text(self.report_kind, &Self::now_text())
            .and_then(|text| {
                std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
            });
        match &res {
            Ok(()) => {
                self.error = None;
                self.status = Some(format!("Exported {}", path.display()));
            }
            Err(e) => self.error = Some(e.clone()),
        }
        res
    }

    // ----------------------------------------------------------- analysis

    fn max_order(&self) -> usize {
        if self.order == 0 {
            usize::MAX
        } else {
            self.order
        }
    }

    pub fn run_analysis(&mut self) {
        let Some(doc) = &self.doc else { return };
        self.error = None;
        let res = (|| -> ftar_core::Result<_> {
            let p = doc.analysis_project()?;
            Ok(match self.tab {
                Tab::CutSets => (
                    Some(minimal_cut_sets_limited(&p.tree, self.max_order())?),
                    None,
                    None,
                ),
                Tab::Probability => (
                    None,
                    Some(probability_report(
                        &p.tree,
                        &p.db,
                        &ProbabilityOptions {
                            max_order: self.max_order(),
                            terms: self.terms,
                            unit_time: self.unit_time,
                        },
                    )?),
                    None,
                ),
                Tab::MonteCarlo => (
                    None,
                    None,
                    Some(simulate(&p.tree, &p.db, &self.mc_options())?),
                ),
            })
        })();
        match res {
            Ok((c, p, m)) => {
                self.cut_sets = c.or(self.cut_sets.take());
                self.prob = p.or(self.prob.take());
                self.mc = m.or(self.mc.take());
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}

/// Quote a CSV field when it contains a separator, quote or line break.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
