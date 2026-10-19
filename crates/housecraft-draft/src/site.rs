//! Terrain and elevation data in plan.

use housecraft_geom::math::{arc_points, spline_points};
use housecraft_geom::terrain::{contours, surface};
use housecraft_geom::{DVec2, v2};
use housecraft_model::*;
use housecraft_units::Length;

use crate::{Drawing, Ink, Prim, Stroke};

const LABEL: f64 = 150.0;

fn label(out: &mut Drawing, id: Id, at: DVec2, text: String, ink: Ink) {
    out.push(
        Some(id),
        Prim::Text {
            at,
            text,
            height: LABEL,
            rotation: 0.0,
            justify: Justify::Left,
            middle: true,
            bold: false,
            ink,
        },
    );
}

fn line(out: &mut Drawing, id: Id, points: Vec<DVec2>, closed: bool, stroke: Stroke) {
    out.push(Some(id), Prim::Polyline { points, closed, stroke });
}

/// The terrain's outline and contour lines (major ones heavier and labelled).
pub(crate) fn terrain(doc: &Document, t: &Terrain, stroke: Stroke, ink: Ink, out: &mut Drawing) {
    let outline: Vec<DVec2> = t.perimeter.iter().map(|p| v2(*p)).collect();
    line(
        out,
        t.id,
        outline,
        true,
        Stroke {
            dash: &[4.0, 1.0, 1.0, 1.0],
            ..stroke
        },
    );
    let Some(s) = surface(doc, t) else {
        return;
    };
    let interval = t.contour_interval.mm();
    let minor = Stroke {
        width: stroke.width * 0.4,
        ink: Ink::Faded,
        ..stroke
    };
    let major = Stroke {
        width: stroke.width * 0.8,
        ..stroke
    };
    let every = t.major_every.max(1) as i64;
    let mut labelled = Vec::new();
    for (a, b, level) in contours(&s, interval) {
        let k = (level / interval).round() as i64;
        let is_major = k % every == 0;
        line(out, t.id, vec![a, b], false, if is_major { major } else { minor });
        if is_major && !labelled.contains(&k) {
            labelled.push(k);
            label(
                out,
                t.id,
                (a + b) / 2.0,
                Length::from_mm(level).format(doc.settings.display_unit),
                ink,
            );
        }
    }
}

/// Elevation points, lines, regions, hills and valleys with their heights.
pub(crate) fn elevation_data(doc: &Document, e: &ElevationData, stroke: Stroke, ink: Ink, out: &mut Drawing) {
    let fmt = |l: Length| l.format(doc.settings.display_unit);
    let dashed = Stroke {
        dash: &[2.0, 1.0],
        ..stroke
    };
    match &e.shape {
        ElevationShape::Point { at, elevation } => {
            let c = v2(*at);
            let s = 120.0;
            line(out, e.id, vec![c - DVec2::new(s, s), c + DVec2::new(s, s)], false, stroke);
            line(out, e.id, vec![c - DVec2::new(s, -s), c + DVec2::new(s, -s)], false, stroke);
            label(out, e.id, c + DVec2::new(s * 1.5, 0.0), fmt(*elevation), ink);
        }
        ElevationShape::Line { points, elevation, smooth } => {
            let pts: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
            let pts = if *smooth { spline_points(&pts, false) } else { pts };
            if let Some(first) = pts.first().copied() {
                label(out, e.id, first + DVec2::new(100.0, 100.0), fmt(*elevation), ink);
            }
            line(out, e.id, pts, false, dashed);
        }
        ElevationShape::Region { points, elevation } => {
            let pts: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
            // Labelled inside its first corner (its middle is often a building).
            if pts.len() >= 3 {
                let c = housecraft_geom::math::centroid(&pts);
                let at = pts[0] + (c - pts[0]).normalize_or_zero() * 400.0;
                label(out, e.id, at, format!("Flat {}", fmt(*elevation)), ink);
            }
            line(out, e.id, pts, true, stroke);
        }
        ElevationShape::Hill { center, radius, height } | ElevationShape::Valley { center, radius, depth: height } => {
            let c = v2(*center);
            line(out, e.id, arc_points(c, radius.mm(), 0.0, 360.0), true, dashed);
            let what = if matches!(e.shape, ElevationShape::Hill { .. }) { "Hill +" } else { "Valley -" };
            label(out, e.id, c, format!("{what}{}", fmt(*height)), ink);
        }
    }
}
