//! Text blocks: rich-text markup, wrapping, a border and leader arrows.
//!
//! Markup inside a note: `**bold**`, `__underline__`, `[label](https://…)`
//! for a link (drawn blue and underlined) and `{#c03030}colour{/}`.

use housecraft_geom::DVec2;
use housecraft_model::{Color, Id, Justify};

use crate::{Drawing, Fill, Ink, Prim, Stroke};

/// Width of `text` as drawn: fonts are sized at height / 0.72, about
/// 0.7 of the height per character on average.
fn approx_text_width(text: &str, height: f64) -> f64 {
    text.chars().count() as f64 * height * 0.7
}

const LINK_BLUE: Color = Color([30, 90, 200, 255]);

/// A piece of a line drawn in one style.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub underline: bool,
    pub color: Option<Color>,
    pub link: Option<String>,
}

/// True when `text` uses any markup.
pub fn has_markup(text: &str) -> bool {
    text.contains("**") || text.contains("__") || text.contains("](") || text.contains("{#")
}

/// The link targets in `text`, in order.
pub fn links(text: &str) -> Vec<String> {
    text.lines()
        .flat_map(parse_line)
        .filter_map(|r| r.link)
        .collect()
}

/// Split one line of markup into styled runs.
pub fn parse_line(line: &str) -> Vec<Run> {
    let mut runs = Vec::new();
    let (mut bold, mut underline, mut color) = (false, false, None::<Color>);
    let mut buf = String::new();
    let flush = |buf: &mut String, runs: &mut Vec<Run>, bold, underline, color| {
        if !buf.is_empty() {
            runs.push(Run { text: std::mem::take(buf), bold, underline, color, link: None });
        }
    };
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        if let Some(r) = rest.strip_prefix("**") {
            flush(&mut buf, &mut runs, bold, underline, color);
            bold = !bold;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("__") {
            flush(&mut buf, &mut runs, bold, underline, color);
            underline = !underline;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("{/}") {
            flush(&mut buf, &mut runs, bold, underline, color);
            color = None;
            rest = r;
        } else if let Some(c2) = rest.strip_prefix("{#").and_then(|r| parse_hex(r.get(..6)?).filter(|_| r.get(6..7) == Some("}"))) {
            flush(&mut buf, &mut runs, bold, underline, color);
            color = Some(c2);
            rest = &rest[9..];
        } else if c == '['
            && let Some(close) = rest.find("](")
            && let Some(end) = rest[close + 2..].find(')')
        {
            flush(&mut buf, &mut runs, bold, underline, color);
            let label = &rest[1..close];
            let url = &rest[close + 2..close + 2 + end];
            runs.push(Run {
                text: if label.is_empty() { url.to_string() } else { label.to_string() },
                bold,
                underline: true,
                color: Some(color.unwrap_or(LINK_BLUE)),
                link: Some(url.to_string()),
            });
            rest = &rest[close + 3 + end..];
        } else {
            buf.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    flush(&mut buf, &mut runs, bold, underline, color);
    runs
}

fn parse_hex(s: &str) -> Option<Color> {
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]))
}

/// How a block of text is drawn.
pub struct TextBlock<'a> {
    pub at: DVec2,
    pub text: &'a str,
    /// Character height, model mm.
    pub height: f64,
    /// Degrees counter-clockwise.
    pub rotation: f64,
    pub justify: Justify,
    /// Centre the block vertically on `at`.
    pub middle: bool,
    pub bold: bool,
    pub ink: Ink,
    pub stroke: Stroke,
    pub border: bool,
    /// Wrap lines wider than this (model mm).
    pub wrap: Option<f64>,
    /// Arrow tips (model mm): a leader from the nearer side to each.
    pub leaders: &'a [DVec2],
}

fn width(run: &Run, height: f64, bold: bool) -> f64 {
    approx_text_width(&run.text, height) * if run.bold || bold { 1.08 } else { 1.0 }
}

/// Draw `b` into `out` for object `id`. Plain text with nothing extra stays
/// one text primitive, as before.
pub fn text_block(out: &mut Drawing, id: Option<Id>, b: &TextBlock) {
    if !has_markup(b.text) && !b.border && b.wrap.is_none() && b.leaders.is_empty() {
        out.push(
            id,
            Prim::Text {
                at: b.at,
                text: b.text.to_string(),
                height: b.height,
                rotation: b.rotation,
                justify: b.justify,
                middle: b.middle,
                bold: b.bold,
                ink: b.ink,
            },
        );
        return;
    }
    // Lay out lines of runs, wrapping by words.
    let mut lines: Vec<Vec<Run>> = Vec::new();
    for source in b.text.split('\n') {
        let mut line: Vec<Run> = Vec::new();
        let mut used = 0.0;
        for run in parse_line(source) {
            let words: Vec<String> = if b.wrap.is_some() && run.link.is_none() {
                run.text.split_inclusive(' ').map(str::to_string).collect()
            } else {
                vec![run.text.clone()]
            };
            for w in words {
                let piece = Run { text: w, ..run.clone() };
                let pw = width(&piece, b.height, b.bold);
                if let Some(limit) = b.wrap
                    && used > 0.0
                    && used + pw > limit
                {
                    lines.push(std::mem::take(&mut line));
                    used = 0.0;
                }
                used += pw;
                // Merge with the previous piece of the same style.
                match line.last_mut() {
                    Some(last) if last.bold == piece.bold && last.underline == piece.underline && last.color == piece.color && last.link == piece.link => {
                        last.text.push_str(&piece.text)
                    }
                    _ => line.push(piece),
                }
            }
        }
        lines.push(line);
    }
    let h = b.height;
    let line_h = h * 1.45;
    let n = lines.len().max(1) as f64;
    let y0 = if b.middle { (n - 1.0) * line_h / 2.0 - h / 2.0 } else { 0.0 };
    let (sin, cos) = b.rotation.to_radians().sin_cos();
    let place = |x: f64, y: f64| b.at + DVec2::new(x * cos - y * sin, x * sin + y * cos);
    let (mut minx, mut maxx) = (f64::MAX, f64::MIN);
    for (i, line) in lines.iter().enumerate() {
        let y = y0 - i as f64 * line_h;
        let w: f64 = line.iter().map(|r| width(r, h, b.bold)).sum();
        let mut x = match b.justify {
            Justify::Left => 0.0,
            Justify::Center => -w / 2.0,
            Justify::Right => -w,
        };
        minx = minx.min(x);
        maxx = maxx.max(x + w);
        for r in line {
            let rw = width(r, h, b.bold);
            let ink = r.color.map_or(b.ink, Ink::Color);
            out.push(
                id,
                Prim::Text {
                    at: place(x, y),
                    text: r.text.clone(),
                    height: h,
                    rotation: b.rotation,
                    justify: Justify::Left,
                    middle: false,
                    bold: r.bold || b.bold,
                    ink,
                },
            );
            if r.underline {
                let trimmed = approx_text_width(r.text.trim_end(), h) * if r.bold || b.bold { 1.08 } else { 1.0 };
                out.push(
                    id,
                    Prim::Polyline {
                        points: vec![place(x, y - h * 0.18), place(x + trimmed, y - h * 0.18)],
                        closed: false,
                        stroke: Stroke { ink, ..b.stroke },
                    },
                );
            }
            x += rw;
        }
    }
    if minx > maxx {
        (minx, maxx) = (0.0, 0.0);
    }
    let pad = h * 0.35;
    let top = y0 + h + pad;
    let bottom = y0 - (n - 1.0) * line_h - h * 0.3 - pad;
    let (left, right) = (minx - pad, maxx + pad);
    if b.border {
        out.push(
            id,
            Prim::Polyline {
                points: vec![place(left, bottom), place(right, bottom), place(right, top), place(left, top)],
                closed: true,
                stroke: b.stroke,
            },
        );
    }
    // Leaders start at the middle of whichever side faces the tip.
    let mid = (top + bottom) / 2.0;
    for tip in b.leaders {
        let d = *tip - b.at;
        let local_x = d.x * cos + d.y * sin;
        let start = if local_x < (left + right) / 2.0 { place(left, mid) } else { place(right, mid) };
        out.push(id, Prim::Polyline { points: vec![start, *tip], closed: false, stroke: b.stroke });
        let dir = (*tip - start).normalize_or_zero();
        if dir != DVec2::ZERO {
            let size = h * 0.8;
            let back = *tip - dir * size;
            let side = DVec2::new(-dir.y, dir.x) * size * 0.3;
            out.push(id, Prim::Polygon { points: vec![*tip, back + side, back - side], fill: Fill::Ink(b.stroke.ink) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_splits_into_runs() {
        let runs = parse_line("plain **bold** __under__ {#ff0000}red{/} [site](https://x.y)");
        let texts: Vec<&str> = runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["plain ", "bold", " ", "under", " ", "red", " ", "site"]);
        assert!(runs[1].bold && runs[3].underline);
        assert_eq!(runs[5].color, Some(Color([255, 0, 0, 255])));
        assert_eq!(runs[7].link.as_deref(), Some("https://x.y"));
        assert_eq!(links("a [b](c) d"), ["c"]);
        assert!(!has_markup("just a note"));
    }

    #[test]
    fn wrapping_and_leaders_add_lines() {
        let mut out = Drawing::default();
        let stroke = Stroke { ink: Ink::Default, width: 0.25, dash: &[] };
        text_block(
            &mut out,
            None,
            &TextBlock {
                at: DVec2::ZERO,
                text: "one two three four five six",
                height: 100.0,
                rotation: 0.0,
                justify: Justify::Left,
                middle: false,
                bold: false,
                ink: Ink::Default,
                stroke,
                border: true,
                wrap: Some(600.0),
                leaders: &[DVec2::new(-2000.0, 0.0)],
            },
        );
        let texts = out.items.iter().filter(|i| matches!(i.prim, Prim::Text { .. })).count();
        assert!(texts >= 3, "wrapped onto several lines: {texts}");
        // Border, leader line and arrowhead.
        assert!(out.items.iter().any(|i| matches!(&i.prim, Prim::Polyline { closed: true, .. })));
        assert!(out.items.iter().any(|i| matches!(&i.prim, Prim::Polygon { .. })));
    }
}
