//! SVG output of a [`Drawing`].

use std::fmt::Write;

use housecraft_geom::DVec2;
use housecraft_model::Justify;

use crate::{Drawing, Fill, Ink, Prim};

#[derive(Debug, Clone)]
pub struct SvgOptions {
    /// Model millimetres per paper millimetre (48 = 1/4" = 1'-0"). Line
    /// weights and dash lengths are paper sizes, so this sets how heavy lines look.
    pub scale: f64,
    pub dark: bool,
    /// Margin around the drawing, model mm.
    pub margin: f64,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions {
            scale: 48.0,
            dark: false,
            margin: 1000.0,
        }
    }
}

pub(crate) fn ink_rgb(ink: Ink, dark: bool) -> (u8, u8, u8, f64) {
    match ink {
        Ink::Default if dark => (230, 230, 230, 1.0),
        Ink::Default => (0, 0, 0, 1.0),
        Ink::Faded if dark => (120, 120, 120, 1.0),
        Ink::Faded => (165, 165, 165, 1.0),
        Ink::Color(c) => {
            let [r, g, b, a] = c.0;
            (r, g, b, a as f64 / 255.0)
        }
    }
}

pub(crate) fn fill_rgb(fill: Fill, dark: bool) -> (u8, u8, u8, f64) {
    match fill {
        Fill::Poche if dark => (95, 95, 95, 1.0),
        Fill::Poche => (190, 190, 190, 1.0),
        Fill::Shadow if dark => (48, 52, 60, 1.0),
        Fill::Shadow => (226, 228, 232, 1.0),
        Fill::Paper if dark => (30, 30, 30, 1.0),
        Fill::Paper => (255, 255, 255, 1.0),
        Fill::Ink(ink) => ink_rgb(ink, dark),
    }
}

fn css((r, g, b, a): (u8, u8, u8, f64)) -> String {
    if a >= 1.0 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r},{g},{b},{a:.3})")
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Render to an SVG document in model units (1 user unit = 1 mm, y up).
pub fn to_svg(drawing: &Drawing, opts: &SvgOptions) -> String {
    let (lo, hi) = drawing
        .bounds()
        .unwrap_or((DVec2::ZERO, DVec2::splat(1000.0)));
    let (lo, hi) = (lo - opts.margin, hi + opts.margin);
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    let y = |v: f64| -v;
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.1} {:.1} {:.1} {:.1}" width="{:.1}mm" height="{:.1}mm">"#,
        lo.x,
        y(hi.y),
        w,
        h,
        w / opts.scale,
        h / opts.scale
    );
    let bg = if opts.dark { "#1e1e1e" } else { "#ffffff" };
    let _ = writeln!(
        s,
        r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{bg}"/>"#,
        lo.x,
        y(hi.y),
        w,
        h
    );
    for item in &drawing.items {
        match &item.prim {
            Prim::Polygon { points, fill } => {
                let pts: Vec<String> = points
                    .iter()
                    .map(|p| format!("{:.1},{:.1}", p.x, y(p.y)))
                    .collect();
                let _ = writeln!(
                    s,
                    r#"<polygon points="{}" fill="{}" stroke="none"/>"#,
                    pts.join(" "),
                    css(fill_rgb(*fill, opts.dark))
                );
            }
            Prim::Polyline {
                points,
                closed,
                stroke,
            } => {
                let pts: Vec<String> = points
                    .iter()
                    .map(|p| format!("{:.1},{:.1}", p.x, y(p.y)))
                    .collect();
                let tag = if *closed { "polygon" } else { "polyline" };
                let dash = if stroke.dash.is_empty() {
                    String::new()
                } else {
                    let d: Vec<String> = stroke
                        .dash
                        .iter()
                        .map(|v| format!("{:.1}", v * opts.scale))
                        .collect();
                    format!(r#" stroke-dasharray="{}""#, d.join(" "))
                };
                let _ = writeln!(
                    s,
                    r#"<{tag} points="{}" fill="none" stroke="{}" stroke-width="{:.1}" stroke-linejoin="round" stroke-linecap="round"{dash}/>"#,
                    pts.join(" "),
                    css(ink_rgb(stroke.ink, opts.dark)),
                    (stroke.width * opts.scale).max(0.5)
                );
            }
            Prim::Text {
                at,
                text,
                height,
                rotation,
                justify,
                middle,
                bold,
                ink,
            } => {
                let anchor = match justify {
                    Justify::Left => "start",
                    Justify::Center => "middle",
                    Justify::Right => "end",
                };
                let baseline = if *middle {
                    r#" dominant-baseline="middle""#
                } else {
                    ""
                };
                let weight = if *bold { r#" font-weight="bold""# } else { "" };
                let _ = writeln!(
                    s,
                    r#"<text x="{:.1}" y="{:.1}" font-family="Arial, Helvetica, sans-serif" font-size="{:.1}" text-anchor="{anchor}" fill="{}" transform="rotate({:.2} {:.1} {:.1})"{baseline}{weight}>{}</text>"#,
                    at.x,
                    y(at.y),
                    height / 0.72,
                    css(ink_rgb(*ink, opts.dark)),
                    -rotation,
                    at.x,
                    y(at.y),
                    escape(text)
                );
            }
        }
    }
    s.push_str("</svg>\n");
    s
}

/// Rasterize SVG text to a PNG no wider or taller than `max_px`.
#[cfg(feature = "png")]
pub fn svg_to_png(svg: &str, max_px: u32) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let size = tree.size();
    let k = (max_px as f32 / size.width().max(size.height())).min(8.0);
    let (w, h) = (
        (size.width() * k).ceil().max(1.0) as u32,
        (size.height() * k).ceil().max(1.0) as u32,
    );
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("image too large")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(k, k),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| e.to_string())
}
