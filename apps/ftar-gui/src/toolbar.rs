//! OpenFTA's icon toolbar under the menu bar and the symbol palette down the
//! left. Icons are drawn here from simple shapes and from the tree's own symbol
//! outlines, not copied from OpenFTA.

use eframe::egui::{self, Color32, Mesh, Pos2, Rect, Sense, Shape, Stroke, Vec2, WidgetInfo};
use ftar_core::EventType;
use ftar_render::scene::{symbol_outline, triangulate};

use crate::app::App;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Icon {
    New,
    Open,
    Save,
    Print,
    Cut,
    Copy,
    Paste,
    Delete,
    Link,
    Database,
    Undo,
    Redo,
    ZoomIn,
    ZoomOut,
    Fit,
}

/// Strokes on an icon's square, in coordinates from 0 to 1 across it.
struct Pen<'a> {
    p: &'a egui::Painter,
    r: Rect,
    st: Stroke,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.r.min + Vec2::new(x * self.r.width(), y * self.r.height())
    }

    fn line(&self, pts: &[(f32, f32)], closed: bool) {
        let mut v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        if closed {
            v.push(v[0]);
        }
        self.p.add(Shape::line(v, self.st));
    }

    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.line(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], true);
    }

    fn circle(&self, x: f32, y: f32, rad: f32) {
        self.p
            .circle_stroke(self.at(x, y), rad * self.r.width(), self.st);
    }
}

/// Paint `icon` in `r` (a square), thin strokes in `col`.
fn paint_icon(p: &egui::Painter, r: Rect, icon: Icon, col: Color32) {
    let pen = Pen {
        p,
        r,
        st: Stroke::new(1.5, col),
    };
    use Icon::*;
    match icon {
        New | Open | Save | Print => paint_file_icon(&pen, icon),
        Cut | Copy | Paste | Delete | Link | Database => paint_edit_icon(&pen, icon),
        Undo | Redo | ZoomIn | ZoomOut | Fit => paint_view_icon(&pen, icon),
    }
}

/// New, Open, Save, Print.
fn paint_file_icon(pen: &Pen, icon: Icon) {
    use Icon::*;
    match icon {
        New => {
            pen.line(
                &[
                    (0.25, 0.1),
                    (0.62, 0.1),
                    (0.8, 0.28),
                    (0.8, 0.9),
                    (0.25, 0.9),
                ],
                true,
            );
            pen.line(&[(0.62, 0.1), (0.62, 0.28), (0.8, 0.28)], false);
        }
        Open => pen.line(
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
            pen.rect(0.15, 0.15, 0.85, 0.85);
            pen.rect(0.3, 0.15, 0.7, 0.4);
            pen.rect(0.3, 0.55, 0.7, 0.85);
        }
        Print => {
            pen.rect(0.1, 0.4, 0.9, 0.75);
            pen.rect(0.28, 0.12, 0.72, 0.4);
            pen.rect(0.28, 0.6, 0.72, 0.9);
        }
        // the other icons are drawn by the other groups (see paint_icon)
        _ => {}
    }
}

/// Clipboard, delete, link and the database.
fn paint_edit_icon(pen: &Pen, icon: Icon) {
    use Icon::*;
    match icon {
        Cut => {
            pen.line(&[(0.3, 0.12), (0.7, 0.68)], false);
            pen.line(&[(0.7, 0.12), (0.3, 0.68)], false);
            pen.circle(0.3, 0.8, 0.12);
            pen.circle(0.7, 0.8, 0.12);
        }
        Copy => {
            pen.rect(0.15, 0.1, 0.6, 0.7);
            pen.rect(0.4, 0.3, 0.85, 0.9);
        }
        Paste => {
            pen.rect(0.2, 0.2, 0.8, 0.9);
            pen.rect(0.38, 0.08, 0.62, 0.28);
            pen.line(&[(0.32, 0.5), (0.68, 0.5)], false);
            pen.line(&[(0.32, 0.68), (0.68, 0.68)], false);
        }
        Delete => {
            // a bin: lid, handle and body
            pen.line(&[(0.15, 0.25), (0.85, 0.25)], false);
            pen.line(&[(0.4, 0.25), (0.4, 0.12), (0.6, 0.12), (0.6, 0.25)], false);
            pen.line(&[(0.25, 0.25), (0.3, 0.9), (0.7, 0.9), (0.75, 0.25)], false);
            pen.line(&[(0.45, 0.4), (0.45, 0.75)], false);
            pen.line(&[(0.55, 0.4), (0.55, 0.75)], false);
        }
        Link => {
            pen.circle(0.36, 0.5, 0.22);
            pen.circle(0.64, 0.5, 0.22);
        }
        Database => {
            // a cylinder: the primary event database
            pen.line(&[(0.25, 0.22), (0.25, 0.78)], false);
            pen.line(&[(0.75, 0.22), (0.75, 0.78)], false);
            pen.line(
                &[
                    (0.25, 0.22),
                    (0.4, 0.12),
                    (0.6, 0.12),
                    (0.75, 0.22),
                    (0.6, 0.32),
                    (0.4, 0.32),
                ],
                true,
            );
            pen.line(
                &[(0.25, 0.78), (0.4, 0.88), (0.6, 0.88), (0.75, 0.78)],
                false,
            );
        }
        // the other icons are drawn by the other groups (see paint_icon)
        _ => {}
    }
}

/// Undo, redo, zoom and fit.
fn paint_view_icon(pen: &Pen, icon: Icon) {
    use Icon::*;
    match icon {
        Undo | Redo => {
            let m = if icon == Undo { 1.0 } else { -1.0 };
            let x = |v: f32| if m > 0.0 { v } else { 1.0 - v };
            pen.line(&[(x(0.8), 0.75), (x(0.8), 0.42), (x(0.3), 0.42)], false);
            pen.line(&[(x(0.45), 0.25), (x(0.28), 0.42), (x(0.45), 0.59)], false);
        }
        ZoomIn | ZoomOut => {
            pen.circle(0.42, 0.42, 0.27);
            pen.line(&[(0.62, 0.62), (0.88, 0.88)], false);
            pen.line(&[(0.3, 0.42), (0.54, 0.42)], false);
            if icon == ZoomIn {
                pen.line(&[(0.42, 0.3), (0.42, 0.54)], false);
            }
        }
        Fit => {
            for (sx, sy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let x = |v: f32| if sx == 0.0 { v } else { 1.0 - v };
                let y = |v: f32| if sy == 0.0 { v } else { 1.0 - v };
                pen.line(
                    &[(x(0.12), y(0.4)), (x(0.12), y(0.12)), (x(0.4), y(0.12))],
                    false,
                );
            }
            pen.rect(0.32, 0.32, 0.68, 0.68);
        }
        // the other icons are drawn by the other groups (see paint_icon)
        _ => {}
    }
}

/// One toolbar button: an icon, a tooltip and an accessible label.
pub(crate) fn icon_button(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool) -> bool {
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
                // like Edit > Delete selected: any node but the top event
                let delete = self.can_cut();
                if icon_button(ui, Icon::Cut, "Cut", cut) {
                    self.cut_selected();
                }
                if icon_button(ui, Icon::Copy, "Copy", copy) {
                    self.copy_selected();
                }
                if icon_button(ui, Icon::Paste, "Paste", paste) {
                    self.paste();
                }
                if icon_button(ui, Icon::Delete, "Delete", delete) {
                    self.delete_selected();
                }
                if icon_button(ui, Icon::Link, "Link", link) {
                    self.edit_link();
                }
                ui.separator();
                if icon_button(ui, Icon::Database, "Primary event database", true) {
                    self.ped.open = true;
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
        let can_add = self.can_add_child();
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
                        if self.palette_button(ui, kind, name, can_add) {
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
