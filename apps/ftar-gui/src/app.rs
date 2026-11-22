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
use ftar_render::{
    print_html_with, svg_to_png, tree_svg_with, Preferences, PrintOptions, SvgOptions,
};

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

/// The form of the primary event editor (modelled on OpenPED).
#[derive(Clone)]
pub struct EventForm {
    /// Id of the database entry the form was loaded from, if any.
    pub original: Option<String>,
    pub id: String,
    pub prob: String,
    /// The probability field holds a failure rate per unit time.
    pub rate: bool,
    pub kind: EventType,
    pub dormant: bool,
    pub desc: String,
}

impl Default for EventForm {
    fn default() -> Self {
        Self {
            original: None,
            id: String::new(),
            prob: String::new(),
            rate: false,
            kind: EventType::Basic,
            dormant: false,
            desc: String::new(),
        }
    }
}

pub struct App {
    /// Drawing colours and monochrome mode (Edit > Preferences).
    pub prefs: Preferences,
    /// Where `prefs` are saved between sessions (`None`: not persisted).
    pub prefs_path: Option<PathBuf>,
    /// The preferences window is shown.
    pub prefs_open: bool,
    /// Page setup and print preview window state.
    pub preview: crate::preview::PrintPreview,
    /// The primary event editor window is shown.
    pub events_open: bool,
    /// The next frame puts the keyboard in the selected node's id field (Edit > Selected...).
    pub focus_details: bool,
    /// Analysis > Validate window is shown.
    pub validate_open: bool,
    /// Help window (user guide and about page).
    pub help: crate::help::HelpState,
    /// The primary event database window (OpenPED), independent of the tree.
    pub ped: crate::ped::PedWindow,
    /// Report window (Report > View): the report kind and its text.
    pub report_view: Option<(ReportKind, String)>,
    pub event_form: EventForm,
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
    /// View > Tree Overview window is shown.
    pub overview_open: bool,
    /// View > Set Zoom window is shown, with the percentage being edited.
    pub set_zoom_open: bool,
    pub set_zoom_percent: f32,
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
    /// Last answer of [`App::other_trees_using`]: (tree file, database, event id) and the text.
    usage_cache: Option<(UsageKey, String)>,
    /// The branch last cut or copied (Edit > Cut/Copy); kept when another tree is opened.
    pub clipboard: Option<Clip>,
}

/// A cut or copied branch, with the database entries of its primary events so
/// that pasting it into another tree keeps their probabilities.
#[derive(Debug, Clone)]
pub struct Clip {
    pub node: ftar_core::Node,
    pub events: Vec<PrimaryEvent>,
}

type UsageKey = (Option<PathBuf>, Option<String>, String);

impl Default for App {
    fn default() -> Self {
        Self {
            prefs: Preferences::default(),
            prefs_path: None,
            prefs_open: false,
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
            preview: Default::default(),
            events_open: false,
            focus_details: false,
            validate_open: false,
            help: crate::help::HelpState::default(),
            ped: Default::default(),
            report_view: None,
            event_form: EventForm::default(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit_next: true,
            overview_open: false,
            set_zoom_open: false,
            set_zoom_percent: 100.0,
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
            usage_cache: None,
            clipboard: None,
        }
    }
}

/// "GATE1 (3 events)" for status messages.
fn describe(n: &ftar_core::Node) -> String {
    let count = {
        let mut k = 0;
        let mut stack = vec![n];
        while let Some(m) = stack.pop() {
            k += 1;
            stack.extend(m.children.iter());
        }
        k
    };
    let name = if n.id.is_empty() {
        "branch"
    } else {
        n.id.as_str()
    };
    if count == 1 {
        name.to_string()
    } else {
        format!("{name} ({count} events)")
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
        self.usage_cache = None;
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
                self.usage_cache = None;
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

    // --------------------------------------------------------------- view

    /// Smallest and largest zoom the canvas allows.
    pub const ZOOM_RANGE: (f32, f32) = (0.02, 4.0);
    /// Zoom In / Zoom Out multiply or divide the zoom by this.
    pub const ZOOM_STEP: f32 = 1.25;

    /// Set the zoom, keeping the point at the centre of the canvas in place.
    /// Turns automatic fitting off.
    pub fn set_zoom(&mut self, zoom: f32) {
        if !zoom.is_finite() {
            return;
        }
        let new = zoom.clamp(Self::ZOOM_RANGE.0, Self::ZOOM_RANGE.1);
        let k = new / self.zoom;
        let rel = self.canvas.size() / 2.0;
        self.pan = rel - (rel - self.pan) * k;
        self.zoom = new;
        self.auto_fit = false;
    }

    pub fn zoom_in(&mut self) {
        self.set_zoom(self.zoom * Self::ZOOM_STEP);
    }

    pub fn zoom_out(&mut self) {
        self.set_zoom(self.zoom / Self::ZOOM_STEP);
    }

    /// Zoom in percent, rounded.
    pub fn zoom_percent(&self) -> u32 {
        (self.zoom * 100.0).round() as u32
    }

    /// Move the view so the drawing point `world` (pixels at 100 %) is at the
    /// centre of the canvas. Turns automatic fitting off.
    pub fn centre_on(&mut self, world: Vec2) {
        self.pan = self.canvas.size() / 2.0 - world * self.zoom;
        self.auto_fit = false;
    }

    /// The part of the drawing (pixels at 100 %) the canvas shows.
    pub fn visible_world(&self) -> eframe::egui::Rect {
        let z = self.zoom.max(f32::EPSILON);
        eframe::egui::Rect::from_min_size((-self.pan / z).to_pos2(), self.canvas.size() / z)
    }

    // ------------------------------------------------------------ editing

    /// Run an edit on the document. On success it is recorded for undo; on
    /// failure the document is restored and the error shown.
    pub(crate) fn edit<R>(
        &mut self,
        f: impl FnOnce(&mut Document) -> Result<R, String>,
    ) -> Option<R> {
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

    // ------------------------------------------------- cut, copy and paste

    /// Something is selected that can be copied (any node, the top event included).
    pub fn can_copy(&self) -> bool {
        self.selected_node().is_some()
    }

    /// Something is selected that can be cut (any node but the top event).
    pub fn can_cut(&self) -> bool {
        self.can_copy() && self.selected.as_ref().is_some_and(|p| !p.is_empty())
    }

    pub fn can_paste(&self) -> bool {
        self.doc.is_some() && self.clipboard.is_some()
    }

    /// Copy the selected branch, with its primary events' database entries.
    pub fn copy_selected(&mut self) {
        let (Some(doc), Some(path)) = (self.doc.as_ref(), self.selected.as_ref()) else {
            return;
        };
        let Ok(node) = edit::copy_subtree(&doc.tree, path) else {
            return;
        };
        let mut ids: Vec<&str> = Vec::new();
        let mut stack = vec![&node];
        while let Some(n) = stack.pop() {
            if !ids.contains(&n.id.as_str()) {
                ids.push(&n.id);
            }
            stack.extend(n.children.iter());
        }
        let events = ids
            .iter()
            .filter_map(|id| doc.db.events.get(*id).cloned())
            .collect();
        self.status = Some(format!("Copied {}", describe(&node)));
        self.clipboard = Some(Clip {
            node: node.clone(),
            events,
        });
    }

    /// Copy the selected branch and remove it from the tree (one undo step).
    pub fn cut_selected(&mut self) {
        if !self.can_cut() {
            return;
        }
        self.copy_selected();
        self.delete_selected();
        if self.error.is_none() {
            if let Some(c) = &self.clipboard {
                self.status = Some(format!("Cut {}", describe(&c.node)));
            }
        }
    }

    /// Paste the clipboard as the last child of the selected gate or
    /// intermediate event (the top event when nothing is selected), or after
    /// the selected event when it cannot have children. Primary events the
    /// tree's database does not have yet are added; existing ones are kept.
    pub fn paste(&mut self) {
        let Some(clip) = self.clipboard.clone() else {
            return;
        };
        let sel = self.selected.clone().unwrap_or_default();
        let new_path = self.edit(|doc| {
            let target = edit::node_at(&doc.tree, &sel).ok_or("no such node")?;
            let (parent, index) = if accepts_children(target.kind) {
                (sel.clone(), target.children.len())
            } else if let Some((&i, parent)) = sel.split_last() {
                (parent.to_vec(), i + 1)
            } else {
                return Err("select a gate or an intermediate event to paste into".to_string());
            };
            let p = edit::paste_subtree(&mut doc.tree, &parent, index, &clip.node)
                .map_err(|e| format!("cannot paste here: {e}"))?;
            for e in &clip.events {
                doc.db
                    .events
                    .entry(e.id.clone())
                    .or_insert_with(|| e.clone());
            }
            Ok(p)
        });
        if let Some(p) = new_path {
            self.status = Some(format!("Pasted {}", describe(&clip.node)));
            self.select(Some(p));
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

    /// Ids of the events in the database, sorted.
    pub fn event_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .doc
            .as_ref()
            .map(|d| d.db.events.keys().cloned().collect())
            .unwrap_or_default();
        ids.sort();
        ids
    }

    /// Fill the editor form from database event `id`.
    pub fn event_form_load(&mut self, id: &str) {
        let Some(e) = self.doc.as_ref().and_then(|d| d.db.events.get(id)).cloned() else {
            return;
        };
        self.event_form = EventForm {
            original: Some(e.id.clone()),
            id: e.id,
            prob: format!("{:E}", e.probability),
            rate: e.lambda == Some('L'),
            kind: e.kind,
            dormant: e.modifier == Some('D'),
            desc: e.description,
        };
    }

    /// Empty the editor form (OpenPED's Clear).
    pub fn event_form_clear(&mut self) {
        self.event_form = EventForm::default();
    }

    /// Add the event in the form to the database, or replace the one with the
    /// same id (OpenPED's Apply). Returns false and sets `error` when refused.
    pub fn event_form_apply(&mut self) -> bool {
        let f = self.event_form.clone();
        let Ok(probability) = f.prob.trim().parse::<f64>() else {
            self.error = Some("probability is not a number".into());
            return false;
        };
        let id = f.id.trim().to_string();
        let ok = self
            .edit(|doc| {
                let group = doc
                    .db
                    .events
                    .get(&id)
                    .map(|e| e.group.clone())
                    .unwrap_or_default();
                let ev = PrimaryEvent {
                    id: id.clone(),
                    group,
                    kind: f.kind,
                    modifier: f.dormant.then_some('D'),
                    description: f.desc.clone(),
                    probability,
                    lambda: f.rate.then_some('L'),
                };
                ftar_core::database::upsert_event(&mut doc.tree, &mut doc.db, ev)
                    .map_err(|e| e.to_string())
            })
            .is_some();
        if ok {
            self.event_form.original = Some(id.clone());
            self.event_form.id = id;
        }
        ok
    }

    /// Remove the event the form was loaded from (or whose id is in it).
    pub fn event_form_delete(&mut self) -> bool {
        let id = self
            .event_form
            .original
            .clone()
            .unwrap_or_else(|| self.event_form.id.trim().to_string());
        let removed = self
            .edit(|doc| {
                ftar_core::database::remove_event(&doc.tree, &mut doc.db, &id)
                    .ok_or_else(|| format!("no event `{id}` in the database"))
            })
            .is_some();
        if removed {
            self.event_form_clear();
        }
        removed
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

    /// Text shown with an event: the other tree files in this tree's folder
    /// that use it from the same database (as saved on disk). Read once per
    /// event, database and file, and again after a save.
    pub fn other_trees_using(&mut self, id: &str) -> String {
        let Some(doc) = self.doc.as_ref() else {
            return String::new();
        };
        let key = (doc.path.clone(), doc.tree.db_name.clone(), id.to_string());
        if let Some((k, text)) = &self.usage_cache {
            if *k == key {
                return text.clone();
            }
        }
        let text = Self::usage_text(doc, id);
        self.usage_cache = Some((key, text.clone()));
        text
    }

    fn usage_text(doc: &Document, id: &str) -> String {
        let Some(path) = doc.path.as_deref() else {
            return "Save the tree to see which other trees use this event".into();
        };
        let usage = match ftar_core::project::trees_using_event(
            &doc.dir(),
            id,
            doc.tree.db_name.as_deref(),
            &ftar_core::project::LoadOptions::default(),
        ) {
            Ok(u) => u,
            Err(e) => return format!("Could not look for other trees: {e}"),
        };
        let me = path.canonicalize().ok();
        let names: Vec<String> = usage
            .trees
            .iter()
            .filter(|t| t.path.canonicalize().ok() != me)
            .map(|t| {
                let name = t
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if t.nodes > 1 {
                    format!("{name} ({} times)", t.nodes)
                } else {
                    name
                }
            })
            .collect();
        let mut text = if names.is_empty() {
            "Not used by other trees in this folder".to_string()
        } else {
            format!("Also used by {}", names.join(", "))
        };
        if usage.truncated {
            text.push_str(" (folder has too many trees; not all were read)");
        }
        if !usage.skipped.is_empty() {
            text.push_str(&format!(
                " ({} unreadable tree file(s) skipped)",
                usage.skipped.len()
            ));
        }
        text
    }

    pub fn set_text(&mut self, text: &str) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let text = text.to_string();
        self.edit(|doc| edit::set_text(&mut doc.tree, &path, &text).map_err(|e| e.to_string()));
    }

    /// Move the selected node among its siblings (`-1` earlier, `+1` later).
    /// Shift Left / Shift Right (delta -1 / +1): move the selected node one
    /// place among its siblings, with OpenFTA's rules: conditions and
    /// transfer-out symbols stay where they are (with a warning), a node is not
    /// shifted past a condition or transfer-out, and shifting at the end of the
    /// row does nothing.
    pub fn move_selected(&mut self, delta: i32) {
        let Some(path) = self.selected.clone() else {
            self.error = Some("Select an item first.".into());
            return;
        };
        let Some(&idx) = path.last() else { return };
        let Some(doc) = self.doc.as_ref() else { return };
        let Some(node) = edit::node_at(&doc.tree, &path) else {
            return;
        };
        let pinned = |k: EventType| {
            matches!(
                k,
                EventType::ConditionAnalysed
                    | EventType::ConditionNotAnalysed
                    | EventType::TransferOut
            )
        };
        if pinned(node.kind) {
            self.error = Some("A condition or a transfer-out cannot be shifted.".into());
            return;
        }
        let parent = &path[..path.len() - 1];
        let siblings = edit::node_at(&doc.tree, parent).map_or(0, |p| p.children.len());
        let new = if delta < 0 {
            if idx == 0 {
                return;
            }
            let left = &edit::node_at(&doc.tree, parent).unwrap().children[idx - 1];
            if pinned(left.kind) {
                return;
            }
            idx - 1
        } else {
            if idx + 1 >= siblings {
                return;
            }
            idx + 1
        };
        if let Some(p) =
            self.edit(|doc| edit::reorder(&mut doc.tree, &path, new).map_err(|e| e.to_string()))
        {
            self.select(Some(p));
        }
    }

    /// Edit > Selected... and double-click: edit the selected symbol. Primary
    /// events open in the primary event editor, as in OpenFTA; everything else
    /// puts the keyboard in the edit panel's id field.
    pub fn edit_selected(&mut self) {
        let Some(node) = self.selected_node().cloned() else {
            self.error = Some("Select an item first.".into());
            return;
        };
        if node.kind.is_primary() && !node.id.is_empty() {
            self.event_form_load(&node.id);
            self.events_open = true;
        } else {
            self.focus_details = true;
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

    // ------------------------------------------------------------ preferences

    /// Load the saved preferences from `path` and keep saving there.
    pub fn use_prefs_file(&mut self, path: PathBuf) {
        self.prefs = Preferences::load(&path);
        self.prefs_path = Some(path);
    }

    /// Replace the preferences and save them (when a file is set).
    pub fn set_prefs(&mut self, prefs: Preferences) {
        self.prefs = prefs;
        if let Some(p) = &self.prefs_path {
            if let Err(e) = self.prefs.save(p) {
                self.error = Some(format!("could not save preferences {}: {e}", p.display()));
            }
        }
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
        Ok(tree_svg_with(
            &doc.tree,
            &doc.db,
            &self.svg_options(),
            &self.prefs,
        ))
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
        Ok(print_html_with(
            &doc.tree,
            &doc.db,
            &PrintOptions {
                title: self.tree_name(),
                probabilities: self.image_probabilities,
                report,
                auto_print,
                page: self.preview.setup,
            },
            &self.prefs,
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

    /// Report > Print: write the last saved report of `kind` as a page of its
    /// own (the page setup from File > Page setup) that opens the print
    /// dialog. Does not run the analysis again.
    pub fn prepare_report_print(&mut self, kind: ReportKind) -> Result<PathBuf, String> {
        let res = self
            .tree_path()
            .ok_or_else(|| "save the tree and run the analysis first".to_string())
            .and_then(|t| ftar_core::report_files::read(t, kind))
            .and_then(|text| {
                let what = match kind {
                    ReportKind::CutSets => "Qualitative report",
                    ReportKind::Probability => "Probability report",
                    ReportKind::MonteCarlo => "Monte Carlo report",
                    ReportKind::FailureStatistics => "Failure statistics",
                };
                let html = ftar_render::report_html(
                    &format!("{} - {what}", self.tree_name()),
                    &text,
                    true,
                    &self.preview.setup,
                );
                let path = std::env::temp_dir().join(format!(
                    "ftar_report_print_{}_{}.html",
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

    /// Analysis > Validate: check the tree's structure and show the problems
    /// in a window, with the result in the status line.
    pub fn validate(&mut self) {
        if self.doc.is_none() {
            return;
        }
        let issues = self.issues();
        self.status = Some(match issues.len() {
            0 => "Validate: no structural problems".to_string(),
            n => format!("Validate: {n} problem(s)"),
        });
        self.validate_open = true;
    }

    /// Select the node a validation problem is about.
    pub fn select_issue(&mut self, index: usize) {
        if let Some(i) = self.issues().get(index) {
            self.select(Some(i.path.clone()));
        }
    }

    /// Analysis > Minimal Cut Sets / Numerical Probability / Monte Carlo
    /// Simulation: show that analysis and its report.
    pub fn show_analysis(&mut self, tab: Tab) {
        self.tab = tab;
        self.report_kind = Self::kind_for_tab(tab);
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

    /// Timestamp for report headers, in the style of C's `ctime`, local time.
    /// Falls back to UTC when the system time zone cannot be determined.
    pub fn now_text() -> String {
        ctime_text(&jiff::ZonedDateTime::now())
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
                self.save_last_report(Self::kind_for_tab(self.tab));
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Path of the tree file, when it has been saved.
    fn tree_path(&self) -> Option<&Path> {
        self.doc.as_ref().and_then(|d| d.path.as_deref())
    }

    /// Write the report of the analysis just run next to the tree, like OpenFTA
    /// (`.crp`, `.prp`, `.mrp`). Trees without a file have nowhere to put it.
    fn save_last_report(&mut self, kind: ReportKind) {
        let Some(tree) = self.tree_path().map(Path::to_path_buf) else {
            return;
        };
        let res = self
            .report_text(kind, &Self::now_text())
            .and_then(|text| ftar_core::report_files::write(&tree, kind, &text));
        if let Err(e) = res {
            self.status = Some(format!("Could not save the report: {e}"));
        }
    }

    /// Whether Report > View has a saved report of `kind` to show.
    pub fn saved_report_exists(&self, kind: ReportKind) -> bool {
        self.tree_path()
            .is_some_and(|t| ftar_core::report_files::exists(t, kind))
    }

    /// Open the last saved report of `kind` in the report window without
    /// running the analysis again.
    pub fn view_saved_report(&mut self, kind: ReportKind) -> Result<(), String> {
        let res = self
            .tree_path()
            .ok_or_else(|| "save the tree and run the analysis first".to_string())
            .and_then(|t| ftar_core::report_files::read(t, kind));
        match res {
            Ok(text) => {
                self.error = None;
                self.report_view = Some((kind, text));
                Ok(())
            }
            Err(e) => {
                self.error = Some(e.clone());
                Err(e)
            }
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

/// Format like C's `ctime` without the trailing newline: `"Mon Jan  1 12:00:00 2024"`.
pub fn ctime_text(t: &jiff::ZonedDateTime) -> String {
    t.strftime("%a %b %e %H:%M:%S %Y").to_string()
}
