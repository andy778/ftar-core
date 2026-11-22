//! SVG to PNG with resvg.

use std::fmt;
use std::sync::{Arc, OnceLock};

use resvg::{tiny_skia, usvg};

/// Largest image accepted, in pixels per side and in total.
const MAX_SIDE: u32 = 20_000;
const MAX_PIXELS: u64 = 120_000_000;

#[derive(Debug, Clone, PartialEq)]
pub enum RenderError {
    /// The SVG could not be parsed.
    Svg(String),
    /// The requested size is zero or too large.
    Size(String),
    Encode(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Svg(m) => write!(f, "invalid SVG: {m}"),
            RenderError::Size(m) => write!(f, "image size: {m}"),
            RenderError::Encode(m) => write!(f, "PNG encoding: {m}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// System fonts, loaded once (scanning them is slow).
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

/// Rasterise `svg` at `scale` and encode it as PNG.
pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>, RenderError> {
    rasterise(svg, scale)?
        .encode_png()
        .map_err(|e| RenderError::Encode(e.to_string()))
}

/// Rasterise `svg` at `scale` into premultiplied RGBA: (width, height, bytes).
/// Used by on-screen previews, which need pixels rather than a PNG file.
pub fn svg_to_rgba(svg: &str, scale: f32) -> Result<(u32, u32, Vec<u8>), RenderError> {
    let p = rasterise(svg, scale)?;
    Ok((p.width(), p.height(), p.take()))
}

/// Rasterise `svg` at `scale` (1.0 = the SVG's own size).
fn rasterise(svg: &str, scale: f32) -> Result<tiny_skia::Pixmap, RenderError> {
    if !(scale.is_finite() && scale > 0.0) {
        return Err(RenderError::Size("scale must be a positive number".into()));
    }
    let opt = usvg::Options {
        fontdb: fonts(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| RenderError::Svg(e.to_string()))?;
    let size = tree.size();
    let w = (size.width() * scale).ceil();
    let h = (size.height() * scale).ceil();
    if !(w >= 1.0 && h >= 1.0) || w > MAX_SIDE as f32 || h > MAX_SIDE as f32 {
        return Err(RenderError::Size(format!(
            "{w}x{h} pixels is outside 1..={MAX_SIDE} per side"
        )));
    }
    let (w, h) = (w as u32, h as u32);
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(RenderError::Size(format!("{w}x{h} is too many pixels")));
    }
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| RenderError::Size(format!("cannot allocate {w}x{h}")))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}
