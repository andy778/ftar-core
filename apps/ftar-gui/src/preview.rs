//! File > Page setup and print preview: choose the sheet size and orientation,
//! fit the tree to one sheet or split it across several, and look at each
//! printed sheet before printing. The layout itself is `ftar_render::page`.

use eframe::egui;
use ftar_render::{
    print_pages_with, svg_to_rgba, Fit, Orientation, PageSetup, PageSize, MAX_PAGES,
};

use crate::app::App;

type TexKey = (u64, PageSetup, bool, usize);

#[derive(Default)]
pub struct PrintPreview {
    pub open: bool,
    pub setup: PageSetup,
    /// Sheet shown in the preview, 0-based.
    pub sheet: usize,
    /// Rendered sheet: its key, the sheet count and the texture.
    tex: Option<(TexKey, usize, egui::TextureHandle)>,
    err: Option<String>,
}

impl App {
    /// One SVG per printed sheet under the current page setup.
    pub fn preview_sheets(&self) -> Result<Vec<String>, String> {
        let doc = self.doc.as_ref().ok_or("no tree open")?;
        Ok(print_pages_with(
            &doc.tree,
            &doc.db,
            &self.tree_name(),
            self.image_probabilities,
            &self.preview.setup,
            &self.prefs,
        ))
    }

    /// Rasterised sheet `idx` (clamped) for display: (width, height, premultiplied RGBA).
    pub fn preview_pixels(&self, idx: usize) -> Result<(usize, u32, u32, Vec<u8>), String> {
        let sheets = self.preview_sheets()?;
        let i = idx.min(sheets.len().saturating_sub(1));
        let svg = sheets.get(i).ok_or("nothing to preview")?;
        let (w, h, px) = svg_to_rgba(svg, 1.0).map_err(|e| e.to_string())?;
        Ok((sheets.len(), w, h, px))
    }

    pub fn preview_window(&mut self, ctx: &egui::Context) {
        if !self.preview.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Page setup and print preview")
            .open(&mut open)
            .default_size([760.0, 640.0])
            .show(ctx, |ui| {
                if self.doc.is_none() {
                    ui.label("Open or create a tree first.");
                    return;
                }
                self.preview_page_setup(ui);
                self.preview_refresh(ui.ctx());
                self.preview_pager(ui);
                self.preview_image(ui);
            });
        self.preview.open = open;
    }

    /// Page size, orientation, fit, margin, scale and the probabilities option.
    fn preview_page_setup(&mut self, ui: &mut egui::Ui) {
        let setup = &mut self.preview.setup;
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_label("page size")
                .selected_text(setup.size.name())
                .show_ui(ui, |ui| {
                    for s in PageSize::ALL {
                        ui.selectable_value(&mut setup.size, s, s.name());
                    }
                });
            egui::ComboBox::from_label("orientation")
                .selected_text(setup.orientation.name())
                .show_ui(ui, |ui| {
                    for o in [Orientation::Landscape, Orientation::Portrait] {
                        ui.selectable_value(&mut setup.orientation, o, o.name());
                    }
                });
            egui::ComboBox::from_label("fit")
                .selected_text(match setup.fit {
                    Fit::FitToPage => "Fit to one page",
                    Fit::SplitPages => "Split across pages",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut setup.fit, Fit::FitToPage, "Fit to one page");
                    ui.selectable_value(&mut setup.fit, Fit::SplitPages, "Split across pages");
                });
        });
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::DragValue::new(&mut setup.margin_mm)
                    .range(0.0..=40.0)
                    .suffix(" mm"),
            )
            .labelled_by(ui.label("margin").id);
            if setup.fit == Fit::SplitPages {
                ui.add(
                    egui::DragValue::new(&mut setup.scale_percent)
                        .range(10.0..=400.0)
                        .suffix(" %"),
                )
                .labelled_by(ui.label("scale").id);
            }
        });
        ui.checkbox(
            &mut self.image_probabilities,
            "Probabilities in images and prints",
        );
    }

    /// Re-render the shown sheet when the tree, the setup or the page changed.
    fn preview_refresh(&mut self, ctx: &egui::Context) {
        let setup = self.preview.setup;
        let key = (
            self.version,
            setup,
            self.image_probabilities,
            self.preview.sheet,
        );
        if self.preview.tex.as_ref().is_none_or(|(k, _, _)| *k != key) {
            match self.preview_pixels(self.preview.sheet) {
                Ok((n, w, h, px)) => {
                    let img =
                        egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], &px);
                    let tex = ctx.load_texture("print_preview", img, Default::default());
                    self.preview.tex = Some((key, n, tex));
                    self.preview.err = None;
                }
                Err(e) => {
                    self.preview.tex = None;
                    self.preview.err = Some(e);
                }
            }
        }
    }

    /// The render error, if any, and page navigation with Print.
    fn preview_pager(&mut self, ui: &mut egui::Ui) {
        if let Some(e) = &self.preview.err {
            ui.colored_label(egui::Color32::RED, e);
        }
        let count = self.preview.tex.as_ref().map_or(0, |(_, n, _)| *n);
        if let Some(last) = count.checked_sub(1) {
            self.preview.sheet = self.preview.sheet.min(last);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.preview.sheet > 0, egui::Button::new("Previous page"))
                    .clicked()
                {
                    self.preview.sheet -= 1;
                }
                ui.label(format!(
                    "Page {} of {}{}",
                    self.preview.sheet + 1,
                    count,
                    if count >= MAX_PAGES { " (maximum)" } else { "" }
                ));
                if ui
                    .add_enabled(
                        self.preview.sheet + 1 < count,
                        egui::Button::new("Next page"),
                    )
                    .clicked()
                {
                    self.preview.sheet += 1;
                }
                if ui.button("Print...").clicked() {
                    self.print_now();
                }
            });
        }
    }

    fn preview_image(&self, ui: &mut egui::Ui) {
        if let Some((_, _, tex)) = &self.preview.tex {
            egui::ScrollArea::both().show(ui, |ui| {
                let avail = ui.available_width().max(200.0);
                let size = tex.size_vec2();
                let k = (avail / size.x).min(1.0);
                ui.add(
                    egui::Image::new((tex.id(), size * k))
                        .alt_text("Print preview of the selected page"),
                );
            });
        }
    }
}
