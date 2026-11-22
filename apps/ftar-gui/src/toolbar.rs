//! OpenFTA's icon toolbar under the menu bar and the symbol palette down the
//! left. Icons are drawn here from simple shapes and from the tree's own symbol
//! outlines, not copied from OpenFTA.

use eframe::egui::{self, Color32, Mesh, Pos2, Rect, Sense, Shape, Stroke, Vec2, WidgetInfo};
use ftar_core::EventType;
use ftar_render::scene::{symbol_outline, triangulate};

use crate::app::App;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Icon {
    New,
    Open,
    Save,
    Print,
    Cut,
    Copy,
    Paste,
    Link,
    Undo,
    Redo,
    ZoomIn,
    ZoomOut,
    Fit,
}

/// Paint `icon` in `r` (a square), thin strokes in `col`.
fn paint_icon(p: &egui::Painter, r: Rect, icon: Icon, col: Color32) {
    let st = Stroke::new(1.5, col);
    let at = |x: f32, y: f32| r.min + Vec2::new(x * r.width(), y * r.height());
    let line = |pts: &[(f32, f32)], closed: bool| {
        let mut v: Vec<Pos2> = pts.iter().map(|&(x, y)| at(x, y)).collect();
        if closed {
            v.push(v[0]);
        }
        p.add(Shape::line(v, st));
    };
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| {
        line(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], true);
    };
    let circle = |x: f32, y: f32, rad: f32| {
        p.circle_stroke(at(x, y), rad * r.width(), st);
    };
    use Icon::*;
    match icon {
        New => {
            line(
                &[
                    (0.25, 0.1),
                    (0.62, 0.1),
                    (0.8, 0.28),
                    (0.8, 0.9),
                    (0.25, 0.9),
                ],
                true,
            );
            line(&[(0.62, 0.1), (0.62, 0.28), (0.8, 0.28)], false);
        }
        Open => line(
            &[
                (0.1, 0.25),
                (0.4, 0.25),
                (0.5, 0.38),
                (0.9, 0.38),
                (0.9, 0.85),
                (0.1, 0.85),
            ],
            true,
        ),
        Save => {
            rect(0.15, 0.15, 0.85, 0.85);
            rect(0.3, 0.15, 0.7, 0.4);
            rect(0.3, 0.55, 0.7, 0.85);
        }
        Print => {
            rect(0.1, 0.4, 0.9, 0.75);
            rect(0.28, 0.12, 0.72, 0.4);
            rect(0.28, 0.6, 0.72, 0.9);
        }
        Cut => {
            line(&[(0.3, 0.12), (0.7, 0.68)], false);
            line(&[(0.7, 0.12), (0.3, 0.68)], false);
            circle(0.3, 0.8, 0.12);
            circle(0.7, 0.8, 0.12);
        }
        Copy => {
            rect(0.15, 0.1, 0.6, 0.7);
            rect(0.4, 0.3, 0.85, 0.9);
        }
        Paste => {
            rect(0.2, 0.2, 0.8, 0.9);
            rect(0.38, 0.08, 0.62, 0.28);
            line(&[(0.32, 0.5), (0.68, 0.5)], false);
            line(&[(0.32, 0.68), (0.68, 0.68)], false);
        }
        Link => {
            circle(0.36, 0.5, 0.22);
            circle(0.64, 0.5, 0.22);
        }
        Undo | Redo => {
            let m = if icon == Undo { 1.0 } else { -1.0 };
            let x = |v: f32| if m > 0.0 { v } else { 1.0 - v };
            line(&[(x(0.8), 0.75), (x(0.8), 0.42), (x(0.3), 0.42)], false);
            line(&[(x(0.45), 0.25), (x(0.28), 0.42), (x(0.45), 0.59)], false);
        }
        ZoomIn | ZoomOut => {
            circle(0.42, 0.42, 0.27);
            line(&[(0.62, 0.62), (0.88, 0.88)], false);
            line(&[(0.3, 0.42), (0.54, 0.42)], false);
            if icon == ZoomIn {
                line(&[(0.42, 0.3), (0.42, 0.54)], false);
            }
        }
        Fit => {
            for (sx, sy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let x = |v: f32| if sx == 0.0 { v } else { 1.0 - v };
                let y = |v: f32| if sy == 0.0 { v } else { 1.0 - v };
                line(
                    &[(x(0.12), y(0.4)), (x(0.12), y(0.12)), (x(0.4), y(0.12))],
                    false,
                );
            }
            rect(0.32, 0.32, 0.68, 0.68);
        }
    }
}

/// One toolbar button: an icon, a tooltip and an accessible label.
fn icon_button(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let resp = resp.on_hover_text(label);
    let visuals = ui.style().interact(&resp);
    if enabled && (resp.hovered() || resp.has_focus()) {
        ui.painter()
            .rect_filled(rect, 3.0, visuals.weak_bg_fill.gamma_multiply(1.5));
    }
    let col = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color().gamma_multiply(0.7)
    };
    paint_icon(ui.painter(), rect.shrink(4.0), icon, col);
    enabled && resp.clicked()
}

/// Palette order and tooltips: events, gates, transfers (OpenFTA's three groups).
const PALETTE: &[&[(EventType, &str)]] = &[
    &[
        (EventType::Intermediate, "Intermediate event"),
        (EventType::Basic, "Basic event"),
        (EventType::Undeveloped, "Undeveloped event"),
        (EventType::External, "External event"),
        (EventType::ConditionNotAnalysed, "Condition (not analysed)"),
    ],
    &[
        (EventType::And, "AND gate"),
        (EventType::Or, "OR gate"),
        (EventType::PriorityAnd, "Priority AND gate"),
        (EventType::Xor, "XOR gate"),
        (EventType::Inhibit, "Inhibit gate"),
    ],
    &[
        (EventType::TransferIn, "Transfer in"),
        (EventType::TransferOut, "Transfer out"),
    ],
];

impl App {
    /// The icon toolbar: file, clipboard and view commands (same state as the menus).
    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let has_doc = self.doc.is_some();
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if icon_button(ui, Icon::New, "New", true) {
                    self.request_new();
                }
                if icon_button(ui, Icon::Open, "Open...", true) {
                    self.open_dialog();
                }
                if icon_button(ui, Icon::Save, "Save", has_doc) {
                    self.save_or_save_as();
                }
                if icon_button(ui, Icon::Print, "Print...", has_doc) {
                    self.print_now();
                }
                ui.separator();
                let (cut, copy, paste, link) = (
                    self.can_cut(),
                    self.can_copy(),
                    self.can_paste(),
                    self.can_link(),
                );
                if icon_button(ui, Icon::Cut, "Cut", cut) {
                    self.cut_selected();
                }
                if icon_button(ui, Icon::Copy, "Copy", copy) {
                    self.copy_selected();
                }
                if icon_button(ui, Icon::Paste, "Paste", paste) {
                    self.paste();
                }
                if icon_button(ui, Icon::Link, "Link", link) {
                    self.edit_link();
                }
                ui.separator();
                if icon_button(ui, Icon::Undo, "Undo", self.history.can_undo()) {
                    self.undo();
                }
                if icon_button(ui, Icon::Redo, "Redo", self.history.can_redo()) {
                    self.redo();
                }
                ui.separator();
                if icon_button(ui, Icon::ZoomIn, "Zoom In", has_doc) {
                    self.zoom_in();
                }
                if icon_button(ui, Icon::ZoomOut, "Zoom Out", has_doc) {
                    self.zoom_out();
                }
                if icon_button(ui, Icon::Fit, "Fit", has_doc) {
                    self.fit_next = true;
                }
                if has_doc {
                    ui.label(format!("{}%", self.zoom_percent()));
                }
            });
        });
    }

    /// The symbol palette: click a symbol to add it to the selected gate or
    /// intermediate event (the top event when nothing is selected). The one
    /// added last stays highlighted.
    pub(crate) fn palette(&mut self, ui: &mut egui::Ui) {
        let has_doc = self.doc.is_some();
        egui::Panel::left("palette")
            .resizable(false)
            .exact_size(44.0)
            .show(ui, |ui| {
                ui.add_space(4.0);
                for (gi, group) in PALETTE.iter().enumerate() {
                    if gi > 0 {
                        ui.separator();
                    }
                    for &(kind, name) in *group {
                        if self.palette_button(ui, kind, name, has_doc) {
                            self.add_kind = kind;
                            self.add_child(kind);
                        }
                    }
                }
            });
    }

    fn palette_button(
        &self,
        ui: &mut egui::Ui,
        kind: EventType,
        name: &str,
        enabled: bool,
    ) -> bool {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
        let label = format!("Add {name}");
        resp.widget_info(|| WidgetInfo::labeled(egui::Role::Button, enabled, &label));
        let resp = resp.on_hover_text(format!("Add {name}"));
        let pen = Stroke::new(
            1.2,
            if enabled {
                ui.visuals().text_color()
            } else {
                ui.visuals().weak_text_color()
            },
        );
        let selected = self.add_kind == kind;
        if selected || (enabled && resp.hovered()) {
            ui.painter().rect(
                rect,
                3.0,
                ui.visuals().widgets.hovered.weak_bg_fill,
                if selected {
                    Stroke::new(1.5, ui.visuals().selection.stroke.color)
                } else {
                    Stroke::NONE
                },
                egui::StrokeKind::Inside,
            );
        }
        let (shapes, [w, d]) = symbol_outline(kind);
        let inner = rect.shrink(6.0);
        let k = (inner.width() / w.max(0.01)).min(inner.height() / d.max(0.01));
        let origin = Pos2::new(inner.center().x, inner.center().y - d * k / 2.0);
        let c = self.prefs.fill(kind);
        let fill = if enabled {
            Color32::from_rgb(c[0], c[1], c[2])
        } else {
            Color32::from_gray(90)
        };
        for (pts, closed, filled) in shapes {
            let pts: Vec<Pos2> = pts
                .iter()
                .map(|p| origin + Vec2::new(p[0], p[1]) * k)
                .collect();
            if filled {
                let mut mesh = Mesh::default();
                for &q in &pts {
                    mesh.colored_vertex(q, fill);
                }
                let flat: Vec<[f32; 2]> = pts.iter().map(|q| [q.x, q.y]).collect();
                for [a, b, cc] in triangulate(&flat) {
                    mesh.add_triangle(a as u32, b as u32, cc as u32);
                }
                ui.painter().add(Shape::mesh(mesh));
            }
            let mut line = pts;
            if closed {
                line.push(line[0]);
            }
            ui.painter().add(Shape::line(line, pen));
        }
        enabled && resp.clicked()
    }
}
