//! Layout pages: each box's live drawing scaled onto paper and cropped to
//! the box, with captions and a title block. Output is in paper millimetres
//! with the origin at the sheet's bottom-left.

use housecraft_api::layouts::{MARGIN, TITLE_STRIP, source_name};
use housecraft_api::schedule::{ScheduleOptions, schedule};
use housecraft_geom::DVec2;
use housecraft_model::*;
use housecraft_units::DisplayUnit;

use crate::pdf::{scale_label, standard_scale};
use crate::plan::{PlanOptions, plan, table_drawing};
use crate::{Drawing, Ink, Prim, Stroke};

fn imperial(doc: &Document) -> bool {
    matches!(
        doc.settings.display_unit,
        DisplayUnit::FeetInches { .. } | DisplayUnit::Inches { .. }
    )
}

/// The drawing a box shows, in its own units: model millimetres for plans
/// and views (`true`), paper millimetres for tables and notes (`false`).
fn source_drawing(doc: &Document, s: &LayoutSource) -> (Drawing, bool) {
    source_drawing_on(doc, s, None)
}

fn table_box(table: &housecraft_api::schedule::ScheduleTable, stroke: Stroke) -> Drawing {
    let mut d = Drawing::default();
    table_drawing(table, DVec2::ZERO, 2.5, stroke, Ink::Default, None, &mut d);
    d
}

/// As [`source_drawing`], with page macros filled for `page` = (layout, index).
fn source_drawing_on(doc: &Document, s: &LayoutSource, page: Option<(&Layout, usize)>) -> (Drawing, bool) {
    let paper_stroke = Stroke {
        ink: Ink::Default,
        width: 0.25,
        dash: &[],
    };
    match s {
        LayoutSource::Plan {
            floor,
            layer_set,
            phase,
        } => {
            let mut opts = PlanOptions::first(doc).unwrap_or(PlanOptions {
                floor: *floor,
                layer_set: Id(0),
                reference_floor: false,
                references: Vec::new(),
                phase: Phase::All,
                sun_shadows: false,
                default_set: None,
            });
            opts.floor = *floor;
            if let Some(ls) = layer_set {
                opts.layer_set = *ls;
            }
            opts.phase = *phase;
            (plan(doc, &opts), true)
        }
        LayoutSource::View { view } => match doc.get::<SectionView>(*view) {
            Some(v) => {
                // The box caption names the view; drop the drawing's own title.
                let mut d = crate::elevation::section_view(doc, v);
                let title = v.name.to_uppercase();
                d.items
                    .retain(|i| !matches!(&i.prim, Prim::Text { text, .. } if *text == title));
                (d, true)
            }
            None => (Drawing::default(), true),
        },
        LayoutSource::Detail { detail } => (crate::detail::cad_detail(doc, *detail, None), true),
        LayoutSource::Schedule {
            schedule_kind,
            floor,
        } => {
            let table = schedule(
                doc,
                *schedule_kind,
                &ScheduleOptions {
                    floor: *floor,
                    columns: Vec::new(),
                },
            );
            let mut d = Drawing::default();
            table_drawing(
                &table,
                DVec2::ZERO,
                2.5,
                paper_stroke,
                Ink::Default,
                None,
                &mut d,
            );
            (d, false)
        }
        LayoutSource::MaterialsList => {
            let table = housecraft_api::materials::materials(doc, &Default::default()).table();
            let mut d = Drawing::default();
            table_drawing(
                &table,
                DVec2::ZERO,
                2.5,
                paper_stroke,
                Ink::Default,
                None,
                &mut d,
            );
            (d, false)
        }
        LayoutSource::Revisions => (table_box(&housecraft_api::layouts::revision_table(doc), paper_stroke), false),
        LayoutSource::SheetIndex { layout } => (table_box(&housecraft_api::layouts::sheet_index(doc, *layout), paper_stroke), false),
        LayoutSource::Text { text } => {
            let mut d = Drawing::default();
            let text = match page {
                Some((l, p)) => crate::macros::expand_page(doc, l, p, text),
                None => crate::macros::expand(doc, text),
            };
            for (i, line) in text.lines().enumerate() {
                d.push(
                    None,
                    Prim::Text {
                        at: DVec2::new(0.0, -(i as f64) * 5.0),
                        text: line.to_string(),
                        height: 3.0,
                        rotation: 0.0,
                        justify: Justify::Left,
                        middle: false,
                        bold: false,
                        ink: Ink::Default,
                    },
                );
            }
            (d, false)
        }
    }
}

/// The model drawing a box shows (plans, elevations, details), in model
/// millimetres; `None` for tables and notes.
pub fn box_model_drawing(doc: &Document, s: &LayoutSource) -> Option<Drawing> {
    let (d, model) = source_drawing(doc, s);
    model.then_some(d)
}

/// A drawing as plain CAD linework and text, for a CAD detail.
pub fn to_linework(d: &Drawing) -> (Vec<DetailShape>, Vec<housecraft_api::DetailText>) {
    let pt = |p: DVec2| housecraft_units::Point2::from_mm(p.x, p.y);
    let color = |ink: Ink| match ink {
        Ink::Color(c) => Some(c),
        _ => None,
    };
    let (mut shapes, mut texts) = (Vec::new(), Vec::new());
    for item in &d.items {
        match &item.prim {
            Prim::Polyline { points, closed, stroke } if points.len() >= 2 => shapes.push(DetailShape {
                shape: Shape::Polyline { points: points.iter().map(|p| pt(*p)).collect(), closed: *closed },
                color: color(stroke.ink),
                fill: None,
            }),
            Prim::Polygon { points, fill } if points.len() >= 3 => {
                let c = match fill {
                    crate::Fill::Ink(ink) => color(*ink),
                    _ => None,
                };
                shapes.push(DetailShape {
                    shape: Shape::Polyline { points: points.iter().map(|p| pt(*p)).collect(), closed: true },
                    color: c,
                    fill: c,
                })
            }
            Prim::Text { at, text, height, rotation, justify, .. } => texts.push(housecraft_api::DetailText {
                position: pt(*at),
                text: text.clone(),
                height: Some(housecraft_units::Length::from_mm(*height)),
                rotation: *rotation,
                justify: *justify,
            }),
            _ => {}
        }
    }
    (shapes, texts)
}

/// Clip a segment to the rectangle `lo..hi` (Liang–Barsky).
fn clip_segment(a: DVec2, b: DVec2, lo: DVec2, hi: DVec2) -> Option<(DVec2, DVec2)> {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-d.x, a.x - lo.x),
        (d.x, hi.x - a.x),
        (-d.y, a.y - lo.y),
        (d.y, hi.y - a.y),
    ] {
        if p.abs() < 1e-12 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t0 <= t1).then(|| (a + d * t0, a + d * t1))
}

/// Clip a polygon to the rectangle `lo..hi` (Sutherland–Hodgman).
fn clip_polygon(poly: &[DVec2], lo: DVec2, hi: DVec2) -> Vec<DVec2> {
    let mut out = poly.to_vec();
    // Signed distance inside each edge of the rectangle.
    type Inside = fn(DVec2, DVec2, DVec2) -> f64;
    let edges: [Inside; 4] = [
        |p, lo, _| p.x - lo.x,
        |p, _, hi| hi.x - p.x,
        |p, lo, _| p.y - lo.y,
        |p, _, hi| hi.y - p.y,
    ];
    for f in edges {
        let input = std::mem::take(&mut out);
        for i in 0..input.len() {
            let (p, q) = (input[i], input[(i + 1) % input.len()]);
            let (fp, fq) = (f(p, lo, hi), f(q, lo, hi));
            if fp >= 0.0 {
                out.push(p);
            }
            if (fp >= 0.0) != (fq >= 0.0) {
                out.push(p.lerp(q, fp / (fp - fq)));
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

fn bounds(d: &Drawing) -> Option<(DVec2, DVec2)> {
    d.bounds()
}

/// The scale a box uses: its own, or the smallest standard scale that fits.
pub fn box_scale(doc: &Document, b: &LayoutBox) -> f64 {
    if b.scale > 0.0 {
        return b.scale;
    }
    let (d, model) = source_drawing(doc, &b.source);
    if !model {
        return 1.0;
    }
    let size = bounds(&d).map_or(DVec2::splat(1000.0), |(lo, hi)| {
        (hi - lo).max(DVec2::splat(1.0))
    });
    let needed = (size.x / b.width.mm()).max(size.y / b.height.mm());
    standard_scale(needed, imperial(doc))
}

/// Render page `page` of `layout`: its template page's boxes, then its own.
pub fn layout_page(doc: &Document, layout: &Layout, page: usize) -> Drawing {
    let mut out = Drawing::default();
    if let Some(t) = layout.template(page) {
        page_boxes(doc, layout, t, page, &mut out);
    }
    page_boxes(doc, layout, page, page, &mut out);
    if layout.title_block {
        title_block(doc, layout, page, &mut out);
    }
    if let Some(w) = &doc.settings.watermark {
        let g = (255.0 * (1.0 - w.darkness.clamp(0.0, 1.0))).round() as u8;
        out.push(
            None,
            Prim::Text {
                at: DVec2::new(layout.paper_width.mm(), layout.paper_height.mm()) / 2.0,
                text: crate::macros::expand_page(doc, layout, page, &w.text),
                height: w.height.mm(),
                rotation: w.angle,
                justify: Justify::Center,
                middle: true,
                bold: true,
                ink: Ink::Color(Color::rgb(g, g, g)),
            },
        );
    }
    out
}

/// Draw the boxes of page `from` onto page `page` (they differ for a
/// template page, whose page macros follow the page it is drawn under).
fn page_boxes(doc: &Document, layout: &Layout, from: usize, page: usize, out: &mut Drawing) {
    let thin = Stroke {
        ink: Ink::Default,
        width: 0.18,
        dash: &[],
    };
    for b in doc
        .iter::<LayoutBox>()
        .filter(|b| b.layout == layout.id && b.page as usize == from)
    {
        let (d, model) = source_drawing_on(doc, &b.source, Some((layout, page)));
        let lo = DVec2::new(b.x.mm(), b.y.mm());
        let hi = lo + DVec2::new(b.width.mm(), b.height.mm());
        let scale = if model { box_scale(doc, b) } else { 1.0 };
        let (dlo, dhi) = bounds(&d).unwrap_or((DVec2::ZERO, DVec2::ZERO));
        // Model drawings centre in the box; tables and notes hang from its top-left.
        let map: Box<dyn Fn(DVec2) -> DVec2> = if model {
            let c = b.center.map_or((dlo + dhi) / 2.0, housecraft_geom::v2);
            let bc = (lo + hi) / 2.0;
            Box::new(move |p| bc + (p - c) / scale)
        } else {
            let top_left = DVec2::new(lo.x + 2.0, hi.y - 2.0);
            let origin = DVec2::new(dlo.x, dhi.y);
            Box::new(move |p| top_left + (p - origin))
        };
        let id = Some(b.id);
        for item in &d.items {
            match &item.prim {
                Prim::Image {
                    corners,
                    path,
                    opacity,
                    transparent,
                } => {
                    let corners = corners.map(&map);
                    // Images aren't cut to the box: keep those inside it.
                    if corners.iter().any(|p| p.cmpge(lo).all() && p.cmple(hi).all()) {
                        out.push(
                            id,
                            Prim::Image {
                                corners,
                                path: path.clone(),
                                opacity: *opacity,
                                transparent: *transparent,
                            },
                        );
                    }
                }
                Prim::Polygon { points, fill } => {
                    let pts: Vec<DVec2> = points.iter().map(|p| map(*p)).collect();
                    let clipped = clip_polygon(&pts, lo, hi);
                    if clipped.len() >= 3 {
                        out.push(
                            id,
                            Prim::Polygon {
                                points: clipped,
                                fill: *fill,
                            },
                        );
                    }
                }
                Prim::Polyline {
                    points,
                    closed,
                    stroke,
                } => {
                    let pts: Vec<DVec2> = points.iter().map(|p| map(*p)).collect();
                    let n = pts.len();
                    let segs = if *closed { n } else { n.saturating_sub(1) };
                    for i in 0..segs {
                        if let Some((p, q)) = clip_segment(pts[i], pts[(i + 1) % n], lo, hi) {
                            out.push(
                                id,
                                Prim::Polyline {
                                    points: vec![p, q],
                                    closed: false,
                                    stroke: *stroke,
                                },
                            );
                        }
                    }
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
                    let p = map(*at);
                    if p.cmpge(lo).all() && p.cmple(hi).all() {
                        out.push(
                            id,
                            Prim::Text {
                                at: p,
                                text: text.clone(),
                                height: if model { height / scale } else { *height },
                                rotation: *rotation,
                                justify: *justify,
                                middle: *middle,
                                bold: *bold,
                                ink: *ink,
                            },
                        );
                    }
                }
            }
        }
        // Caption: number, name and scale under the box.
        let caption = b
            .label
            .as_deref()
            .map_or_else(|| source_name(doc, &b.source), |l| crate::macros::expand_page(doc, layout, page, l));
        let caption_y = lo.y - 4.0;
        let mut text_x = lo.x;
        if let Some(n) = housecraft_api::layouts::box_number(doc, b).filter(|_| from == page) {
            let c = DVec2::new(lo.x + 3.2, caption_y + 1.0);
            out.push(
                id,
                Prim::Polyline {
                    points: housecraft_geom::math::arc_points(c, 3.2, 0.0, 360.0),
                    closed: true,
                    stroke: Stroke { width: 0.35, ..thin },
                },
            );
            out.push(
                id,
                Prim::Text {
                    at: c,
                    text: n.to_string(),
                    height: 3.0,
                    rotation: 0.0,
                    justify: Justify::Center,
                    middle: true,
                    bold: true,
                    ink: Ink::Default,
                },
            );
            text_x += 8.5;
        }
        out.push(
            id,
            Prim::Text {
                at: DVec2::new(text_x, caption_y),
                text: caption.to_uppercase(),
                height: 3.0,
                rotation: 0.0,
                justify: Justify::Left,
                middle: false,
                bold: true,
                ink: Ink::Default,
            },
        );
        out.push(
            id,
            Prim::Polyline {
                points: vec![
                    DVec2::new(lo.x, caption_y - 1.2),
                    DVec2::new(hi.x.min(lo.x + 80.0), caption_y - 1.2),
                ],
                closed: false,
                stroke: Stroke {
                    width: 0.35,
                    ..thin
                },
            },
        );
        if model {
            out.push(
                id,
                Prim::Text {
                    at: DVec2::new(text_x, caption_y - 5.0),
                    text: format!("Scale: {}", scale_label(scale, imperial(doc))),
                    height: 2.2,
                    rotation: 0.0,
                    justify: Justify::Left,
                    middle: false,
                    bold: false,
                    ink: Ink::Default,
                },
            );
        }
    }
}

fn title_block(doc: &Document, layout: &Layout, page: usize, out: &mut Drawing) {
    let (pw, ph) = (layout.paper_width.mm(), layout.paper_height.mm());
    let frame = Stroke {
        ink: Ink::Default,
        width: 0.5,
        dash: &[],
    };
    let (lo, hi) = (DVec2::splat(MARGIN), DVec2::new(pw - MARGIN, ph - MARGIN));
    out.push(
        None,
        Prim::Polyline {
            points: vec![lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)],
            closed: true,
            stroke: frame,
        },
    );
    let strip = MARGIN + TITLE_STRIP;
    out.push(
        None,
        Prim::Polyline {
            points: vec![DVec2::new(lo.x, strip), DVec2::new(hi.x, strip)],
            closed: false,
            stroke: frame,
        },
    );
    let mid = MARGIN + TITLE_STRIP / 2.0;
    let text =
        |out: &mut Drawing, x: f64, y: f64, t: String, h: f64, bold: bool, justify: Justify| {
            out.push(
                None,
                Prim::Text {
                    at: DVec2::new(x, y),
                    text: t,
                    height: h,
                    rotation: 0.0,
                    justify,
                    middle: true,
                    bold,
                    ink: Ink::Default,
                },
            );
        };
    text(
        out,
        lo.x + 5.0,
        mid + 3.0,
        doc.name.to_uppercase(),
        5.0,
        true,
        Justify::Left,
    );
    text(
        out,
        lo.x + 5.0,
        mid - 4.0,
        layout.name.clone(),
        3.0,
        false,
        Justify::Left,
    );
    let page_name = layout.pages.get(page).cloned().unwrap_or_default();
    text(
        out,
        hi.x - 5.0,
        mid + 3.0,
        page_name,
        6.0,
        true,
        Justify::Right,
    );
    // The page's title and the designer, mid-strip.
    let title = layout.title(page);
    if !title.is_empty() {
        text(out, (lo.x + hi.x) / 2.0, mid + 3.0, title.to_uppercase(), 4.0, true, Justify::Center);
    }
    let p = &doc.settings.project;
    let who: Vec<&str> = [p.client.as_str(), p.designer.as_str(), p.company.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
    if !who.is_empty() {
        text(out, (lo.x + hi.x) / 2.0, mid - 4.0, who.join("  ·  "), 2.5, false, Justify::Center);
    }
    let rev = doc.settings.revisions.last().map_or(String::new(), |r| format!("  ·  Rev {}", r.number));
    text(
        out,
        hi.x - 5.0,
        mid - 5.0,
        format!(
            "Sheet {} of {}  ·  {}{rev}",
            page + 1,
            layout.pages.len(),
            crate::macros::today()
        ),
        2.5,
        false,
        Justify::Right,
    );
}

/// Every page of `layout`, in order.
pub fn layout_pages(doc: &Document, layout: &Layout) -> Vec<Drawing> {
    (0..layout.pages.len())
        .map(|p| layout_page(doc, layout, p))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping() {
        let (lo, hi) = (DVec2::ZERO, DVec2::splat(10.0));
        let (p, q) = clip_segment(DVec2::new(-5.0, 5.0), DVec2::new(15.0, 5.0), lo, hi).unwrap();
        assert_eq!((p, q), (DVec2::new(0.0, 5.0), DVec2::new(10.0, 5.0)));
        assert!(clip_segment(DVec2::new(-5.0, -5.0), DVec2::new(-1.0, 20.0), lo, hi).is_none());
        let tri = [
            DVec2::new(-5.0, 0.0),
            DVec2::new(5.0, 0.0),
            DVec2::new(5.0, 20.0),
        ];
        let c = clip_polygon(&tri, lo, hi);
        assert!(c.iter().all(|p| p.cmpge(lo).all() && p.cmple(hi).all()));
        assert!(c.len() >= 3);
    }
}
