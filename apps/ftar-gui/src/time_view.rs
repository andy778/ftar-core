//! The Time tab: PFTA's time-dependent analysis (`ftar_core::quant`) of the
//! open tree. The settings and results live in [`TimeView`]; the methods on
//! [`App`] run and export without a window, the `ui` functions draw.

use std::path::Path;

use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Vec2};
use ftar_core::quant::{analyse_document, tsv, GateResult, QuantOptions, QuantReport, Series};

use crate::App;

/// Which quantity the chart shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Curve {
    #[default]
    Probability,
    Intensity,
    Rate,
}

impl Curve {
    pub const ALL: [Curve; 3] = [Curve::Probability, Curve::Intensity, Curve::Rate];

    pub fn label(self) -> &'static str {
        match self {
            Curve::Probability => "probability q",
            Curve::Intensity => "intensity ω",
            Curve::Rate => "rate λ",
        }
    }

    fn values(self, s: &Series) -> &[f64] {
        match self {
            Curve::Probability => &s.q,
            Curve::Intensity => &s.w,
            Curve::Rate => &s.rate,
        }
    }
}

/// Settings and last result of the Time tab. Empty settings use the
/// tree's own (a `.ftt` file's properties; time 1 for OpenFTA trees).
#[derive(Debug, Clone, Default)]
pub struct TimeView {
    /// Comma-separated times.
    pub times: String,
    /// Samples of the distributions; 0 for the tree's own.
    pub samples: u32,
    pub seed: String,
    /// Truncation order of the expansions; 0 for the tree's own.
    pub order: u32,
    pub report: Option<QuantReport>,
    /// The gate shown (index into the report's gates).
    pub gate: usize,
    /// The time of the importance table (index into the report's times).
    pub at: usize,
    pub curve: Curve,
}

impl TimeView {
    /// The analysis options these settings ask for.
    pub fn options(&self) -> Result<QuantOptions, String> {
        let times = if self.times.trim().is_empty() {
            None
        } else {
            let t: Result<Vec<f64>, String> = self
                .times
                .split(',')
                .map(|s| {
                    s.trim()
                        .parse::<f64>()
                        .ok()
                        // `nan` is PFTA's "any time"; negative times are not.
                        .filter(|t| t.is_nan() || *t >= 0.0)
                        .ok_or_else(|| format!("`{}` is not a time (0 or more)", s.trim()))
                })
                .collect();
            Some(t?)
        };
        Ok(QuantOptions {
            times,
            sample_size: (self.samples > 0).then_some(self.samples as usize),
            seed: (!self.seed.trim().is_empty()).then(|| self.seed.trim().to_string()),
            computational_order: (self.order > 0).then_some(self.order as usize),
            ..QuantOptions::default()
        })
    }

    /// The gate shown, if there is a result.
    pub fn shown_gate(&self) -> Option<&GateResult> {
        self.report.as_ref()?.gates.get(self.gate)
    }
}

impl App {
    /// Run the time analysis of the open tree with the Time tab's settings.
    pub fn run_time_analysis(&mut self) {
        let Some(doc) = &self.doc else { return };
        self.error = None;
        let res = self
            .time
            .options()
            .and_then(|o| analyse_document(doc, &o).map_err(|e| e.to_string()));
        match res {
            Ok(r) => {
                // Keep the gate shown when it is still there, else the first top gate.
                let keep = self
                    .time
                    .shown_gate()
                    .and_then(|g| r.gates.iter().position(|x| x.id == g.id));
                self.time.gate = keep
                    .or_else(|| r.gates.iter().position(|g| g.is_top_gate))
                    .unwrap_or(0);
                self.time.at = r.times.len().saturating_sub(1);
                self.time.report = Some(r);
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// The tree's own times, as the Time tab shows them when its field is empty.
    pub fn tree_times(&self) -> String {
        let times = self
            .doc
            .as_ref()
            .and_then(|d| d.extras.props.times.clone())
            .unwrap_or_else(|| vec![1.0]);
        times
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Write PFTA's tables of the last time analysis into `dir`.
    pub fn export_time_tables(&mut self, dir: &Path) -> Result<usize, String> {
        let r = self
            .time
            .report
            .as_ref()
            .ok_or("run the time analysis first")?;
        let n = tsv::write_dir(r, dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        self.status = Some(format!("Wrote {n} tables to {}", dir.display()));
        Ok(n)
    }

    pub(crate) fn export_time_tables_dialog(&mut self) {
        if let Some(dir) = crate::desktop::FileDialog::new()
            .set_title("Folder for the time analysis tables")
            .pick_folder()
        {
            if let Err(e) = self.export_time_tables(&dir) {
                self.error = Some(e);
            }
        }
    }

    /// The Time tab's settings.
    pub(crate) fn time_parameters(&mut self, ui: &mut egui::Ui) {
        let hint = self.tree_times();
        ui.horizontal(|ui| {
            let l = ui.label("times");
            ui.add(
                egui::TextEdit::singleline(&mut self.time.times)
                    .hint_text(hint)
                    .desired_width(160.0),
            )
            .labelled_by(l.id);
        });
        ui.horizontal(|ui| {
            let l = ui.label("samples");
            ui.add(egui::DragValue::new(&mut self.time.samples).range(0..=100_000))
                .labelled_by(l.id);
            let l = ui.label("seed");
            ui.add(
                egui::TextEdit::singleline(&mut self.time.seed)
                    .hint_text("the tree's")
                    .desired_width(70.0),
            )
            .labelled_by(l.id);
            let l = ui.label("truncation order");
            ui.add(egui::DragValue::new(&mut self.time.order).range(0..=50))
                .labelled_by(l.id);
        });
        ui.small("Empty or 0: the tree's own settings (a .ftt file's; time 1 for .fta trees).");
    }

    /// The Time tab's results: gate, chart, values per time, importance.
    pub(crate) fn time_results(&mut self, ui: &mut egui::Ui) {
        let Some(r) = self.time.report.clone() else {
            ui.label("Run computes probability, intensity and rate over time (PFTA's analysis).");
            return;
        };
        if ui
            .button("Export tables...")
            .on_hover_text("PFTA's .tsv tables: models, events, gates, cut sets, importances")
            .clicked()
        {
            self.export_time_tables_dialog();
        }
        egui::ComboBox::from_label("gate")
            .selected_text(gate_title(r.gates.get(self.time.gate)))
            .show_ui(ui, |ui| {
                for (i, g) in r.gates.iter().enumerate() {
                    ui.selectable_value(&mut self.time.gate, i, gate_title(Some(g)));
                }
            });
        let Some(g) = r.gates.get(self.time.gate) else {
            return;
        };
        egui::ComboBox::from_label("curve")
            .selected_text(self.time.curve.label())
            .show_ui(ui, |ui| {
                for c in Curve::ALL {
                    ui.selectable_value(&mut self.time.curve, c, c.label());
                }
            });
        chart(ui, &r, &g.series, self.time.curve);
        values_table(ui, &r, g);
        self.importance_table(ui, &r, g);
    }

    fn importance_table(&mut self, ui: &mut egui::Ui, r: &QuantReport, g: &GateResult) {
        if g.importances.is_empty() {
            return;
        }
        ui.add_space(6.0);
        egui::ComboBox::from_label("Importance at time")
            .selected_text(fmt_time(r.times.get(self.time.at).copied()))
            .show_ui(ui, |ui| {
                for (i, t) in r.times.iter().enumerate() {
                    ui.selectable_value(&mut self.time.at, i, fmt_time(Some(*t)));
                }
            });
        ui.monospace(format!(
            "{:<14} {:>9} {:>9} {:>9} {:>9} {:>9}",
            "event", "Birnbaum", "critical", "F-V", "RAW", "RRW"
        ));
        let at = self.time.at;
        let mean = |v: &[f64]| r.expected(v).get(at).copied().unwrap_or(f64::NAN);
        let mut rows: Vec<_> = g.importances.iter().collect();
        rows.sort_by(|a, b| mean(&b.criticality).total_cmp(&mean(&a.criticality)));
        for m in rows {
            ui.monospace(format!(
                "{:<14} {:>9.2E} {:>9.2E} {:>9.2E} {:>9.2E} {:>9.2E}",
                m.event,
                mean(&m.marginal),
                mean(&m.criticality),
                mean(&m.diagnostic),
                mean(&m.risk_achievement_worth),
                mean(&m.risk_reduction_worth)
            ));
        }
    }
}

/// How a gate is named in the gate list: its id, `(top)` for a top gate.
pub fn gate_title(g: Option<&GateResult>) -> String {
    let Some(g) = g else { return String::new() };
    let top = if g.is_top_gate { " (top)" } else { "" };
    format!("{}{top}", g.id)
}

/// A time as the importance list shows it.
pub fn fmt_time(t: Option<f64>) -> String {
    t.map_or_else(String::new, |t| t.to_string())
}

/// Time, q (with its 5% and 95% quantiles when sampled), ω and rate.
fn values_table(ui: &mut egui::Ui, r: &QuantReport, g: &GateResult) {
    let sampled = r.sample_size > 1;
    let unit = r.time_unit.as_deref().unwrap_or("");
    ui.add_space(6.0);
    ui.strong(if sampled {
        format!("Gate {} (means of {} samples)", g.id, r.sample_size)
    } else {
        format!("Gate {}", g.id)
    });
    let head = if sampled {
        format!(
            "{:>9} {:>10} {:>10} {:>10} {:>10} {:>10}",
            format!("time {unit}"),
            "q",
            "q 5%",
            "q 95%",
            "ω",
            "rate"
        )
    } else {
        format!(
            "{:>9} {:>10} {:>10} {:>10}",
            format!("time {unit}"),
            "q",
            "ω",
            "rate"
        )
    };
    ui.monospace(head);
    let s = &g.series;
    let (q, w, l) = (r.expected(&s.q), r.expected(&s.w), r.expected(&s.rate));
    let (lo, hi) = (r.quantile(&s.q, 0.05), r.quantile(&s.q, 0.95));
    for (i, t) in r.times.iter().enumerate() {
        let at = |v: &[f64]| v.get(i).copied().unwrap_or(f64::NAN);
        ui.monospace(if sampled {
            format!(
                "{t:>9} {:>10.3E} {:>10.3E} {:>10.3E} {:>10.3E} {:>10.3E}",
                at(&q),
                at(&lo),
                at(&hi),
                at(&w),
                at(&l)
            )
        } else {
            format!(
                "{t:>9} {:>10.3E} {:>10.3E} {:>10.3E}",
                at(&q),
                at(&w),
                at(&l)
            )
        });
    }
}

/// Points of the chart: (time, mean, 5%, 95%) at each finite time.
pub fn chart_points(r: &QuantReport, s: &Series, curve: Curve) -> Vec<(f64, f64, f64, f64)> {
    let v = curve.values(s);
    let (m, lo, hi) = (r.expected(v), r.quantile(v, 0.05), r.quantile(v, 0.95));
    r.times
        .iter()
        .enumerate()
        .filter(|(_, t)| t.is_finite())
        .map(|(i, t)| {
            let at = |x: &[f64]| x.get(i).copied().unwrap_or(f64::NAN);
            (*t, at(&m), at(&lo), at(&hi))
        })
        .collect()
}

/// Where the chart's lines go in `area`: the mean curve, and the 5–95%
/// band (up along the 95% values, back along the 5%) when `sampled`.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartShape {
    pub line: Vec<Pos2>,
    pub band: Vec<Pos2>,
    /// The value at the top of the chart: the largest mean, or 95% value
    /// when sampled.
    pub ymax: f64,
    /// The first and last time.
    pub t0: f64,
    pub t1: f64,
}

/// Lay out `points` (from [`chart_points`]) in `area`. `None` with fewer
/// than two points.
pub fn chart_shape(
    points: &[(f64, f64, f64, f64)],
    area: Rect,
    sampled: bool,
) -> Option<ChartShape> {
    if points.len() < 2 {
        return None;
    }
    let (t0, t1) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p.0), b.max(p.0))
        });
    let top = points
        .iter()
        .flat_map(|p| [p.1, if sampled { p.3 } else { p.1 }])
        .filter(|y| y.is_finite())
        .fold(0.0f64, f64::max);
    let ymax = if top > 0.0 { top } else { 1.0 };
    let to = |t: f64, y: f64| -> Pos2 {
        let x = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
        let y = if y.is_finite() {
            (y / ymax).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Pos2::new(
            area.left() + (x as f32) * area.width(),
            area.bottom() - (y as f32) * area.height(),
        )
    };
    let band = if sampled {
        points
            .iter()
            .map(|p| to(p.0, p.3))
            .chain(points.iter().rev().map(|p| to(p.0, p.2)))
            .collect()
    } else {
        Vec::new()
    };
    Some(ChartShape {
        line: points.iter().map(|p| to(p.0, p.1)).collect(),
        band,
        ymax,
        t0,
        t1,
    })
}

/// A line chart of the curve over time, with the 5–95% band when sampled.
fn chart(ui: &mut egui::Ui, r: &QuantReport, s: &Series, curve: Curve) {
    let size = Vec2::new(ui.available_width().min(420.0), 170.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let area = resp.rect.shrink2(Vec2::new(8.0, 14.0));
    let Some(c) = chart_shape(&chart_points(r, s, curve), area, r.sample_size > 1) else {
        ui.small("The chart needs two or more finite times.");
        return;
    };
    let name = format!(
        "chart of {} from time {} to {}, up to {:.2E}",
        curve.label(),
        c.t0,
        c.t1,
        c.ymax
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &name));
    let visuals = ui.visuals();
    let ink = visuals.text_color();
    let line = visuals.selection.bg_fill;
    painter.rect_stroke(
        area,
        0.0,
        Stroke::new(1.0, visuals.weak_text_color()),
        egui::StrokeKind::Inside,
    );
    for pair in c.band.windows(2) {
        if let [a, b] = pair {
            painter.line_segment([*a, *b], Stroke::new(1.0, line.gamma_multiply(0.5)));
        }
    }
    painter.add(egui::Shape::line(c.line, Stroke::new(2.0, line)));
    let font = egui::FontId::monospace(10.0);
    let text = |pos: Pos2, align, s: String| {
        painter.text(pos, align, s, font.clone(), ink);
    };
    text(
        area.left_top(),
        egui::Align2::LEFT_BOTTOM,
        format!("{:.2E}", c.ymax),
    );
    text(
        area.left_bottom(),
        egui::Align2::LEFT_TOP,
        format!("{}", c.t0),
    );
    text(
        area.right_bottom(),
        egui::Align2::RIGHT_TOP,
        format!("{}", c.t1),
    );
}
