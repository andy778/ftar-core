pub mod layout;

use std::path::PathBuf;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use ftar_core::analysis::{
    minimal_cut_sets_limited, probability_report, CutSet, ProbabilityOptions, ProbabilityReport,
};
use ftar_core::montecarlo::{simulate, MonteCarloOptions, MonteCarloReport};
use ftar_core::project::Project;
use ftar_core::EventType;

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

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Tab {
    CutSets,
    Probability,
    MonteCarlo,
}

pub struct App {
    pub path: Option<PathBuf>,
    pub project: Option<Project>,
    pub error: Option<String>,
    pub selected: Option<usize>,
    // view
    pub zoom: f32,
    pub pan: Vec2,
    pub fit_next: bool,
    // analysis
    pub tab: Tab,
    pub order: usize,
    pub terms: usize,
    pub unit_time: f64,
    pub trials: u64,
    pub cut_sets: Option<Vec<CutSet>>,
    pub prob: Option<ProbabilityReport>,
    pub mc: Option<MonteCarloReport>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            path: None,
            project: None,
            error: None,
            selected: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit_next: true,
            tab: Tab::CutSets,
            order: 0,
            terms: 5,
            unit_time: 1.0,
            trials: 100_000,
            cut_sets: None,
            prob: None,
            mc: None,
        }
    }
}

impl App {
    pub fn open(&mut self, path: PathBuf) {
        match Project::load(&path) {
            Ok(p) => {
                self.project = Some(p);
                self.path = Some(path);
                self.error = None;
                self.selected = None;
                self.cut_sets = None;
                self.prob = None;
                self.mc = None;
                self.fit_next = true;
            }
            Err(e) => self.error = Some(format!("{}: {e}", path.display())),
        }
    }

    fn max_order(&self) -> usize {
        if self.order == 0 { usize::MAX } else { self.order }
    }

    pub fn run_analysis(&mut self) {
        let Some(p) = self.project.take() else { return };
        self.error = None;
        let res = (|| -> ftar_core::Result<()> {
            match self.tab {
                Tab::CutSets => {
                    self.cut_sets = Some(minimal_cut_sets_limited(&p.tree, self.max_order())?)
                }
                Tab::Probability => {
                    self.prob = Some(probability_report(
                        &p.tree,
                        &p.db,
                        &ProbabilityOptions {
                            max_order: self.max_order(),
                            terms: self.terms,
                            unit_time: self.unit_time,
                        },
                    )?)
                }
                Tab::MonteCarlo => {
                    self.mc = Some(simulate(
                        &p.tree,
                        &p.db,
                        &MonteCarloOptions {
                            trials: self.trials,
                            unit_time: self.unit_time,
                            ..Default::default()
                        },
                    )?)
                }
            }
            Ok(())
        })();
        self.project = Some(p);
        if let Err(e) = res {
            self.error = Some(e.to_string());
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

const NODE_W: f32 = 120.0;
const NODE_H: f32 = 56.0;
const GAP_X: f32 = 150.0;
const GAP_Y: f32 = 110.0;

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        egui::Panel::top("menu").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open...").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("OpenFTA tree", &["fta"])
                        .pick_file()
                    {
                        self.open(p);
                    }
                }
                if ui.button("Fit").clicked() {
                    self.fit_next = true;
                }
                if let Some(p) = &self.path {
                    ui.label(p.display().to_string());
                }
            });
        });

        egui::Panel::right("analysis").default_size(420.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::CutSets, "Cut sets");
                ui.selectable_value(&mut self.tab, Tab::Probability, "Probability");
                ui.selectable_value(&mut self.tab, Tab::MonteCarlo, "Monte Carlo");
            });
            ui.separator();
            ui.horizontal(|ui| {
                let l = ui.label("max order (0 = all)");
                ui.add(egui::DragValue::new(&mut self.order).range(0..=50)).labelled_by(l.id);
            });
            match self.tab {
                Tab::Probability => {
                    ui.horizontal(|ui| {
                        let l = ui.label("terms");
                        ui.add(egui::DragValue::new(&mut self.terms).range(1..=12)).labelled_by(l.id);
                        let l = ui.label("unit time");
                        ui.add(egui::DragValue::new(&mut self.unit_time).speed(0.1)).labelled_by(l.id);
                    });
                }
                Tab::MonteCarlo => {
                    ui.horizontal(|ui| {
                        let l = ui.label("trials");
                        ui.add(egui::DragValue::new(&mut self.trials).range(1..=100_000_000)).labelled_by(l.id);
                        let l = ui.label("unit time");
                        ui.add(egui::DragValue::new(&mut self.unit_time).speed(0.1)).labelled_by(l.id);
                    });
                }
                Tab::CutSets => {}
            }
            if ui.add_enabled(self.project.is_some(), egui::Button::new("Run")).clicked() {
                self.run_analysis();
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
                        ui.monospace(format!(
                            "P(top)          {:.4E} +/- {:.2E}",
                            r.probability, r.std_error
                        ));
                        ui.monospace(format!("P(any failure)  {:.4E}", r.any_component_failure));
                        ui.add_space(6.0);
                        let mut h = r.cut_set_hits.clone();
                        h.sort_by_key(|x| std::cmp::Reverse(x.1));
                        for (cs, n) in h {
                            ui.monospace(format!(
                                "{:>8}  {}",
                                n,
                                cs.iter().cloned().collect::<Vec<_>>().join(" ")
                            ));
                        }
                    }
                }
            });
        });

        egui::Panel::bottom("details").show(ui, |ui| {
            if let (Some(p), Some(i)) = (&self.project, self.selected) {
                let placed = layout::layout(&p.tree.root);
                if let Some(pl) = placed.get(i) {
                    let n = pl.node;
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(kind_name(n.kind));
                        if !n.id.is_empty() {
                            ui.monospace(&n.id);
                        }
                        if !n.text.is_empty() {
                            ui.label(&n.text);
                        }
                        if let Some(e) = p.db.events.get(&n.id) {
                            ui.label(format!("- {}  p/rate = {:E}", e.description, e.probability));
                        }
                    });
                }
            } else {
                ui.label("Click a node for details. Scroll to zoom, drag to pan.");
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(project) = &self.project else {
                ui.centered_and_justified(|ui| ui.label("Open a .fta file (File > Open, or pass a path)."));
                return;
            };
            let placed = layout::layout(&project.tree.root);
            let (resp, painter) =
                ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = resp.rect;

            // world -> screen
            let (max_x, max_y) = placed
                .iter()
                .fold((0.0f32, 0.0f32), |a, p| (a.0.max(p.x), a.1.max(p.y)));
            if self.fit_next {
                let w = (max_x * GAP_X + NODE_W).max(1.0);
                let h = (max_y * GAP_Y + NODE_H).max(1.0);
                self.zoom = (rect.width() / w).min(rect.height() / h).clamp(0.1, 1.5) * 0.95;
                self.pan = Vec2::new(
                    (rect.width() - w * self.zoom) / 2.0,
                    (rect.height() - h * self.zoom) / 2.0,
                );
                self.fit_next = false;
            }
            if resp.dragged() {
                self.pan += resp.drag_delta();
            }
            if resp.hovered() {
                let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let old = self.zoom;
                    self.zoom = (self.zoom * (1.0 + scroll * 0.002)).clamp(0.05, 4.0);
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
            let line = Stroke::new(1.5 * z.max(0.5), if dark { Color32::GRAY } else { Color32::DARK_GRAY });
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
            let mut clicked = None;
            for (i, p) in placed.iter().enumerate() {
                let c = centre(p);
                let r = Rect::from_center_size(c, Vec2::new(NODE_W, NODE_H) * z);
                if !r.intersects(rect) {
                    continue;
                }
                let sel = self.selected == Some(i);
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
                            vec![c + Vec2::new(0.0, -h), c + Vec2::new(w, 0.0), c + Vec2::new(0.0, h), c + Vec2::new(-w, 0.0)],
                            fill,
                            stroke,
                        ));
                    }
                    EventType::External => {
                        let (w, h) = (NODE_H * 0.5 * z, NODE_H * 0.45 * z);
                        painter.add(Shape::convex_polygon(
                            vec![c + Vec2::new(0.0, -h), c + Vec2::new(w, 0.0), c + Vec2::new(w, h), c + Vec2::new(-w, h), c + Vec2::new(-w, 0.0)],
                            fill,
                            stroke,
                        ));
                    }
                    k if k.is_gate() => {
                        let rr = Rect::from_center_size(c, Vec2::new(NODE_H * 0.9, NODE_H * 0.55) * z);
                        painter.rect(rr, 14.0 * z, fill, stroke, egui::StrokeKind::Middle);
                    }
                    _ => {
                        let rr = Rect::from_center_size(c, Vec2::new(NODE_W, NODE_H * 0.8) * z);
                        painter.rect(rr, 3.0 * z, fill, stroke, egui::StrokeKind::Middle);
                    }
                }
                if z > 0.35 {
                    let fs = (12.0 * z).clamp(7.0, 22.0);
                    painter.text(c, Align2::CENTER_CENTER, label, FontId::proportional(fs), text_col);
                    // text for intermediate events / ids under basic events
                    let cap = match n.kind {
                        EventType::Intermediate => n.text.as_str(),
                        _ => "",
                    };
                    if !cap.is_empty() {
                        let short: String = cap.chars().take(40).collect();
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
                            clicked = Some(i);
                        }
                    }
                }
            }
            if resp.clicked() {
                self.selected = clicked;
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
    } else if n.kind == Intermediate {
        n.id.clone()
    } else {
        n.id.clone()
    };
    (fill, label)
}
