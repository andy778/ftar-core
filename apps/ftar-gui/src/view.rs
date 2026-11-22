//! View commands: zoom in/out, set zoom, and the Tree Overview (minimap) window.
//! The maths are plain functions and `App` methods so they can be tested without a window.

use eframe::egui::{self, Color32, Key, Modifiers, Rect, Sense, Stroke, Vec2};

use crate::app::App;
use crate::layout;
use crate::ui::{content_size, GAP_X, GAP_Y, ID_SPACE, NODE_W, SYM_U};

/// Smallest and largest zoom factor (2% .. 400%).
pub const ZOOM_MIN: f32 = 0.02;
pub const ZOOM_MAX: f32 = 4.0;
/// Factor of one Zoom In / Zoom Out step.
pub const ZOOM_STEP: f32 = 1.25;

/// New `(zoom, pan)` after multiplying the zoom by `factor`, keeping the canvas point
/// `anchor` (relative to the canvas corner) fixed. Non-finite input leaves the view as is.
pub fn zoom_about(zoom: f32, pan: Vec2, factor: f32, anchor: Vec2) -> (f32, Vec2) {
    if !(zoom.is_finite() && zoom > 0.0 && factor.is_finite() && factor > 0.0) {
        return (zoom, pan);
    }
    let z = (zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    let k = z / zoom;
    (z, anchor - (anchor - pan) * k)
}

/// Pan that puts the tree-space point `centre` in the middle of a canvas of `canvas` size.
pub fn pan_for_centre(centre: Vec2, zoom: f32, canvas: Vec2) -> Vec2 {
    canvas / 2.0 - centre * zoom
}

/// Scale (overview pixels per tree unit) that fits `content` into `size`.
pub fn overview_scale(content: Vec2, size: Vec2) -> f32 {
    (size.x / content.x.max(1.0)).min(size.y / content.y.max(1.0))
}

impl App {
    /// Multiply the zoom by `factor` about the canvas centre.
    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom_to_factor(self.zoom * factor);
    }

    fn zoom_to_factor(&mut self, target: f32) {
        if self.zoom > 0.0 && target.is_finite() {
            let anchor = self.canvas.size() / 2.0;
            let (z, pan) = zoom_about(self.zoom, self.pan, target / self.zoom, anchor);
            self.zoom = z;
            self.pan = pan;
            self.auto_fit = false;
            self.fit_next = false;
        }
    }

    pub fn zoom_in(&mut self) {
        self.zoom_by(ZOOM_STEP);
    }

    pub fn zoom_out(&mut self) {
        self.zoom_by(1.0 / ZOOM_STEP);
    }

    /// Set the zoom to `percent` (100 = natural size), clamped to 2..400, about the centre.
    pub fn set_zoom_percent(&mut self, percent: f32) {
        self.zoom_to_factor(percent / 100.0);
    }

    pub fn zoom_percent(&self) -> f32 {
        self.zoom * 100.0
    }

    /// Size of the whole drawing in tree units, if a tree is open.
    fn content_extent(&self) -> Option<Vec2> {
        let doc = self.doc.as_ref()?;
        let (mx, my) = layout::layout(&doc.tree.root)
            .iter()
            .fold((0.0f32, 0.0f32), |a, p| (a.0.max(p.x), a.1.max(p.y)));
        Some(content_size(mx, my))
    }

    /// Centre the view on the tree-space point `p` (a click in the overview).
    pub fn centre_view_on(&mut self, p: Vec2) {
        self.pan = pan_for_centre(p, self.zoom, self.canvas.size());
        self.auto_fit = false;
        self.fit_next = false;
    }

    pub(crate) fn view_shortcuts(&mut self, ctx: &egui::Context) {
        let (zin, zin2, zout) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::Equals),
                i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Plus),
                i.consume_key(Modifiers::COMMAND, Key::Minus),
            )
        });
        if self.doc.is_some() {
            if zin || zin2 {
                self.zoom_in();
            }
            if zout {
                self.zoom_out();
            }
        }
    }

    pub(crate) fn view_menu(&mut self, ui: &mut egui::Ui) {
        let has_doc = self.doc.is_some();
        ui.menu_button("View", |ui| {
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Zoom In").shortcut_text("Ctrl+="),
                )
                .clicked()
            {
                ui.close();
                self.zoom_in();
            }
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Zoom Out").shortcut_text("Ctrl+-"),
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
                self.zoom_dialog = Some(self.zoom_percent());
            }
            ui.separator();
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Tree Overview").selected(self.overview_open),
                )
                .clicked()
            {
                ui.close();
                self.overview_open = !self.overview_open;
            }
        });
    }

    pub(crate) fn set_zoom_window(&mut self, ctx: &egui::Context) {
        let Some(mut pct) = self.zoom_dialog else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Set Zoom")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let l = ui.label("Zoom (%)");
                    ui.add(
                        egui::DragValue::new(&mut pct)
                            .range(ZOOM_MIN * 100.0..=ZOOM_MAX * 100.0)
                            .speed(1.0)
                            .max_decimals(0),
                    )
                    .labelled_by(l.id);
                });
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if apply {
            self.set_zoom_percent(pct);
            self.zoom_dialog = None;
        } else if cancel || !open {
            self.zoom_dialog = None;
        } else {
            self.zoom_dialog = Some(pct);
        }
    }

    pub(crate) fn overview_window(&mut self, ctx: &egui::Context) {
        if !self.overview_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Tree Overview")
            .open(&mut open)
            .default_size([220.0, 160.0])
            .show(ctx, |ui| {
                let Some(content) = self.content_extent() else {
                    ui.label("Open or create a tree first.");
                    return;
                };
                let avail = Vec2::new(ui.available_width().max(60.0), 140.0);
                let (resp, painter) = ui.allocate_painter(avail, Sense::click_and_drag());
                let s = overview_scale(content, resp.rect.size());
                let origin = resp.rect.min;
                let to_o = |t: Vec2| origin + t * s;
                painter.rect_stroke(
                    Rect::from_min_size(origin, content * s),
                    0.0,
                    Stroke::new(1.0, Color32::GRAY),
                    egui::StrokeKind::Inside,
                );
                if let Some(doc) = &self.doc {
                    for p in layout::layout(&doc.tree.root).iter() {
                        let c = to_o(Vec2::new(
                            p.x * GAP_X + NODE_W / 2.0,
                            p.y * GAP_Y + (ID_SPACE + SYM_U) / 2.0,
                        ));
                        painter.circle_filled(c, 2.0, Color32::from_rgb(120, 150, 200));
                    }
                }
                // the part of the drawing currently visible in the canvas
                if self.zoom > 0.0 {
                    let vis = Rect::from_min_size(
                        to_o(-self.pan / self.zoom),
                        self.canvas.size() / self.zoom * s,
                    );
                    painter.rect_stroke(
                        vis,
                        0.0,
                        Stroke::new(2.0, Color32::from_rgb(255, 170, 0)),
                        egui::StrokeKind::Middle,
                    );
                }
                if (resp.clicked() || resp.dragged()) && s > 0.0 {
                    if let Some(p) = resp.interact_pointer_pos() {
                        let t = (p - origin) / s;
                        self.centre_view_on(Vec2::new(
                            t.x.clamp(0.0, content.x),
                            t.y.clamp(0.0, content.y),
                        ));
                    }
                }
            });
        if !open {
            self.overview_open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_the_anchor_fixed_and_clamps() {
        let (z, pan) = zoom_about(1.0, Vec2::new(10.0, 20.0), 2.0, Vec2::new(100.0, 50.0));
        assert_eq!(z, 2.0);
        // the anchor's tree point stays under the anchor
        let before = Vec2::new(100.0, 50.0) - Vec2::new(10.0, 20.0);
        let after = (Vec2::new(100.0, 50.0) - pan) / z;
        assert!((before - after).length() < 1e-4);
        assert_eq!(zoom_about(1.0, Vec2::ZERO, 1e9, Vec2::ZERO).0, ZOOM_MAX);
        assert_eq!(zoom_about(1.0, Vec2::ZERO, 1e-9, Vec2::ZERO).0, ZOOM_MIN);
    }

    #[test]
    fn bad_input_leaves_the_view_alone() {
        for f in [f32::NAN, f32::INFINITY, 0.0, -1.0] {
            assert_eq!(zoom_about(1.5, Vec2::new(1.0, 2.0), f, Vec2::ZERO).0, 1.5);
        }
        assert_eq!(zoom_about(0.0, Vec2::ZERO, 2.0, Vec2::ZERO).0, 0.0);
    }

    #[test]
    fn centring_puts_the_point_in_the_middle() {
        let canvas = Vec2::new(800.0, 600.0);
        let pan = pan_for_centre(Vec2::new(300.0, 200.0), 2.0, canvas);
        assert_eq!(Vec2::new(300.0, 200.0) * 2.0 + pan, canvas / 2.0);
    }

    #[test]
    fn overview_scale_fits_the_longer_side() {
        let s = overview_scale(Vec2::new(1000.0, 100.0), Vec2::new(200.0, 140.0));
        assert!((s - 0.2).abs() < 1e-6);
    }
}
