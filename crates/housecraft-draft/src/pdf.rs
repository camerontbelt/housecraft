//! Vector PDF output of a [`Drawing`] on one page, to scale.

use housecraft_geom::DVec2;
use housecraft_model::Justify;
use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str};

use crate::svg::{fill_rgb, ink_rgb};
use crate::{Drawing, Ink, Prim, approx_text_width};

const PT_PER_MM: f64 = 72.0 / 25.4;

/// Paper sizes in mm, portrait.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paper {
    Letter,
    Legal,
    Tabloid,
    ArchC,
    ArchD,
    A4,
    A3,
    A2,
    A1,
}

impl Paper {
    pub fn size_mm(self) -> (f64, f64) {
        match self {
            Paper::Letter => (215.9, 279.4),
            Paper::Legal => (215.9, 355.6),
            Paper::Tabloid => (279.4, 431.8),
            Paper::ArchC => (457.2, 609.6),
            Paper::ArchD => (609.6, 914.4),
            Paper::A4 => (210.0, 297.0),
            Paper::A3 => (297.0, 420.0),
            Paper::A2 => (420.0, 594.0),
            Paper::A1 => (594.0, 841.0),
        }
    }

    pub fn parse(s: &str) -> Option<Paper> {
        Some(
            match s.to_ascii_lowercase().replace(['-', ' '], "").as_str() {
                "letter" => Paper::Letter,
                "legal" => Paper::Legal,
                "tabloid" | "ledger" | "11x17" => Paper::Tabloid,
                "archc" | "c" => Paper::ArchC,
                "archd" | "d" => Paper::ArchD,
                "a4" => Paper::A4,
                "a3" => Paper::A3,
                "a2" => Paper::A2,
                "a1" => Paper::A1,
                _ => return None,
            },
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scale {
    /// Largest standard scale that fits.
    Fit,
    /// Model mm per paper mm (48 = 1/4" = 1'-0", 100 = 1:100).
    Ratio(f64),
}

#[derive(Debug, Clone)]
pub struct PdfOptions {
    pub paper: Paper,
    pub landscape: bool,
    pub scale: Scale,
    /// Title-block text (document, floor).
    pub title: String,
    /// Label scales in feet-and-inches (1/4" = 1'-0") instead of 1:n.
    pub imperial: bool,
}

const IMPERIAL_SCALES: &[f64] = &[
    12.0, 16.0, 24.0, 32.0, 48.0, 64.0, 96.0, 128.0, 192.0, 384.0,
];
const METRIC_SCALES: &[f64] = &[
    10.0, 20.0, 25.0, 50.0, 75.0, 100.0, 200.0, 250.0, 500.0, 1000.0,
];

/// "1/4\" = 1'-0\"" for 48, or "1:48".
pub fn scale_label(scale: f64, imperial: bool) -> String {
    if imperial {
        let inches_per_foot = 12.0 / scale;
        for den in [1u32, 2, 4, 8, 16, 32, 64] {
            let num = inches_per_foot * den as f64;
            if (num - num.round()).abs() < 1e-6 && num >= 1.0 {
                let num = num.round() as u32;
                let g = gcd(num, den);
                let (n, d) = (num / g, den / g);
                return if d == 1 {
                    format!("{n}\" = 1'-0\"")
                } else {
                    format!("{n}/{d}\" = 1'-0\"")
                };
            }
        }
    }
    format!("1:{}", scale.round())
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn latin1(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
        .collect()
}

/// Render the drawing as a one-page PDF. Returns the file bytes and the scale used.
pub fn to_pdf(drawing: &Drawing, opts: &PdfOptions) -> (Vec<u8>, f64) {
    let (pw, ph) = {
        let (w, h) = opts.paper.size_mm();
        if opts.landscape { (h, w) } else { (w, h) }
    };
    let margin = 12.0;
    let title_h = 14.0;
    let avail = DVec2::new(pw - 2.0 * margin, ph - 2.0 * margin - title_h);
    let (lo, hi) = drawing
        .bounds()
        .unwrap_or((DVec2::ZERO, DVec2::splat(1000.0)));
    let size = (hi - lo).max(DVec2::splat(1.0));
    let scale = match opts.scale {
        Scale::Ratio(r) => r,
        Scale::Fit => {
            let needed = (size.x / avail.x).max(size.y / avail.y);
            let list = if opts.imperial {
                IMPERIAL_SCALES
            } else {
                METRIC_SCALES
            };
            list.iter()
                .copied()
                .find(|s| *s >= needed)
                .unwrap_or(needed)
        }
    };
    let center = (lo + hi) / 2.0;
    let page_center = DVec2::new(pw / 2.0, margin + title_h + avail.y / 2.0);
    let map = |p: DVec2| {
        let mm = (p - center) / scale + page_center;
        (mm * PT_PER_MM).as_vec2()
    };

    let mut c = Content::new();
    draw_items(&mut c, drawing, map, scale);

    // Title block.
    c.set_dash_pattern(std::iter::empty::<f32>(), 0.0);
    c.set_stroke_rgb(0.0, 0.0, 0.0);
    c.set_line_width(0.6);
    let frame = |x: f64, y: f64| (DVec2::new(x, y) * PT_PER_MM).as_vec2();
    let a = frame(margin, margin);
    let b = frame(pw - margin, ph - margin);
    c.rect(a.x, a.y, b.x - a.x, b.y - a.y);
    c.stroke();
    let line_y = frame(0.0, margin + title_h).y;
    c.move_to(a.x, line_y);
    c.line_to(b.x, line_y);
    c.stroke();
    let label = format!(
        "{}    Scale: {}",
        opts.title,
        scale_label(scale, opts.imperial)
    );
    text_at(
        &mut c,
        frame(margin + 4.0, margin + title_h / 2.0),
        &label,
        10.0,
        0.0,
        Justify::Left,
        true,
        true,
        Ink::Default,
    );
    text_at(
        &mut c,
        frame(pw - margin - 4.0, margin + title_h / 2.0),
        "HouseCraft",
        8.0,
        0.0,
        Justify::Right,
        true,
        false,
        Ink::Faded,
    );

    let catalog = Ref::new(1);
    let pages = Ref::new(2);
    let page = Ref::new(3);
    let contents = Ref::new(4);
    let regular = Ref::new(5);
    let bold = Ref::new(6);
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(pages);
    pdf.pages(pages).kids([page]).count(1);
    {
        let mut p = pdf.page(page);
        p.media_box(Rect::new(
            0.0,
            0.0,
            (pw * PT_PER_MM) as f32,
            (ph * PT_PER_MM) as f32,
        ));
        p.parent(pages);
        p.contents(contents);
        p.resources()
            .fonts()
            .pair(Name(b"F1"), regular)
            .pair(Name(b"F2"), bold);
    }
    pdf.type1_font(regular)
        .base_font(Name(b"Helvetica"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));
    pdf.type1_font(bold)
        .base_font(Name(b"Helvetica-Bold"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));
    let data = c.finish();
    pdf.stream(contents, &data);
    (pdf.finish(), scale)
}

/// Draw every item of `drawing` into `c`, mapping model points to PDF points
/// with `map`; text heights are model millimetres divided by `scale`.
fn draw_items(c: &mut Content, drawing: &Drawing, map: impl Fn(DVec2) -> glam::Vec2, scale: f64) {
    c.set_line_cap(pdf_writer::types::LineCapStyle::RoundCap);
    c.set_line_join(pdf_writer::types::LineJoinStyle::RoundJoin);
    for item in &drawing.items {
        match &item.prim {
            Prim::Polygon { points, fill } => {
                if points.len() < 3 {
                    continue;
                }
                let (r, g, b, _) = fill_rgb(*fill, false);
                c.set_fill_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                let p0 = map(points[0]);
                c.move_to(p0.x, p0.y);
                for p in &points[1..] {
                    let q = map(*p);
                    c.line_to(q.x, q.y);
                }
                c.close_path();
                c.fill_nonzero();
            }
            Prim::Polyline {
                points,
                closed,
                stroke,
            } => {
                if points.len() < 2 {
                    continue;
                }
                let (r, g, b, _) = ink_rgb(stroke.ink, false);
                c.set_stroke_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                c.set_line_width((stroke.width * PT_PER_MM) as f32);
                c.set_dash_pattern(stroke.dash.iter().map(|d| (d * PT_PER_MM) as f32), 0.0);
                let p0 = map(points[0]);
                c.move_to(p0.x, p0.y);
                for p in &points[1..] {
                    let q = map(*p);
                    c.line_to(q.x, q.y);
                }
                if *closed {
                    c.close_path();
                }
                c.stroke();
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
                let size = (height / 0.72 / scale * PT_PER_MM) as f32;
                if size < 0.5 {
                    continue;
                }
                text_at(
                    c,
                    map(*at),
                    text,
                    size,
                    *rotation,
                    *justify,
                    *middle,
                    *bold,
                    *ink,
                );
            }
        }
    }
}

/// Render pages already laid out in paper millimetres (origin bottom-left)
/// as one multi-page PDF of `width` x `height` mm.
pub fn pages_to_pdf(pages: &[Drawing], width: f64, height: f64) -> Vec<u8> {
    let map = |p: DVec2| (p * PT_PER_MM).as_vec2();
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let regular = Ref::new(3);
    let bold = Ref::new(4);
    let page_ids: Vec<Ref> = (0..pages.len())
        .map(|i| Ref::new(5 + 2 * i as i32))
        .collect();
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(page_ids.iter().copied())
        .count(pages.len() as i32);
    for (i, drawing) in pages.iter().enumerate() {
        let (page, contents) = (page_ids[i], Ref::new(6 + 2 * i as i32));
        {
            let mut p = pdf.page(page);
            p.media_box(Rect::new(
                0.0,
                0.0,
                (width * PT_PER_MM) as f32,
                (height * PT_PER_MM) as f32,
            ));
            p.parent(tree);
            p.contents(contents);
            p.resources()
                .fonts()
                .pair(Name(b"F1"), regular)
                .pair(Name(b"F2"), bold);
        }
        let mut c = Content::new();
        draw_items(&mut c, drawing, map, 1.0);
        pdf.stream(contents, &c.finish());
    }
    pdf.type1_font(regular)
        .base_font(Name(b"Helvetica"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));
    pdf.type1_font(bold)
        .base_font(Name(b"Helvetica-Bold"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));
    pdf.finish()
}

/// The smallest standard drawing scale (model mm per paper mm) at or above `needed`.
pub fn standard_scale(needed: f64, imperial: bool) -> f64 {
    let list = if imperial {
        IMPERIAL_SCALES
    } else {
        METRIC_SCALES
    };
    list.iter()
        .copied()
        .find(|s| *s >= needed)
        .unwrap_or(needed)
}

#[allow(clippy::too_many_arguments)]
fn text_at(
    c: &mut Content,
    at: glam::Vec2,
    text: &str,
    size: f32,
    rotation: f64,
    justify: Justify,
    middle: bool,
    bold: bool,
    ink: Ink,
) {
    let (r, g, b, _) = ink_rgb(ink, false);
    let width = approx_text_width(text, size as f64 * 0.72) as f32;
    let shift_x = match justify {
        Justify::Left => 0.0,
        Justify::Center => -width / 2.0,
        Justify::Right => -width,
    };
    let shift_y = if middle { -size * 0.35 } else { 0.0 };
    let (sin, cos) = (rotation.to_radians() as f32).sin_cos();
    let x = at.x + shift_x * cos - shift_y * sin;
    let y = at.y + shift_x * sin + shift_y * cos;
    c.set_fill_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    c.begin_text();
    c.set_font(Name(if bold { b"F2" } else { b"F1" }), size);
    c.set_text_matrix([cos, sin, -sin, cos, x, y]);
    c.show(Str(&latin1(text)));
    c.end_text();
}
