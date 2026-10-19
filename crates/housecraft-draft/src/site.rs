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

/// A site feature's plan symbol.
pub(crate) fn feature(f: &SiteFeature, stroke: Stroke, out: &mut Drawing) {
    let pts: Vec<DVec2> = f.points.iter().map(|p| v2(*p)).collect();
    if pts.len() < 2 {
        return;
    }
    let thin = Stroke {
        width: stroke.width * 0.5,
        ..stroke
    };
    // The path's two edges `half` either side.
    let sides = |pts: &[DVec2], half: f64| -> (Vec<DVec2>, Vec<DVec2>) {
        let n = pts.len();
        (0..n)
            .map(|i| {
                let a = pts[i.saturating_sub(1)];
                let b = pts[(i + 1).min(n - 1)];
                let d = housecraft_geom::math::perp((b - a).normalize_or_zero()) * half;
                (pts[i] + d, pts[i] - d)
            })
            .unzip()
    };
    match &f.feature {
        SiteKind::RetainingWall { thickness, .. } => {
            let (l, r) = sides(&pts, thickness.mm() / 2.0);
            let mut outline = l;
            outline.extend(r.into_iter().rev());
            line(out, f.id, outline, true, stroke);
        }
        SiteKind::Road { style, width, curbs, .. } => {
            let (l, r) = sides(&pts, width.mm() / 2.0);
            line(out, f.id, l, false, stroke);
            line(out, f.id, r, false, stroke);
            if *curbs {
                let (l, r) = sides(&pts, width.mm() / 2.0 + 150.0);
                line(out, f.id, l, false, thin);
                line(out, f.id, r, false, thin);
            }
            if *style == RoadStyle::Road {
                line(out, f.id, pts, false, Stroke { dash: &[4.0, 3.0], ..thin });
            }
        }
        SiteKind::Fence { gates, .. } => {
            line(out, f.id, pts.clone(), false, stroke);
            // Posts as ticks every sample is too busy; mark gates with a swing arc.
            for g in gates {
                let samples = housecraft_geom::site::along(&pts, false, 50.0);
                let at = |d: f64| samples.iter().find(|(_, x)| *x >= d).map(|(p, _)| *p);
                if let (Some(a), Some(b)) = (at(g.at.mm()), at(g.at.mm() + g.width.mm())) {
                    let start = (b - a).to_angle().to_degrees();
                    line(out, f.id, arc_points(a, a.distance(b), start, 90.0), false, Stroke { dash: &[1.0, 0.8], ..thin });
                }
            }
        }
        SiteKind::GardenBed { .. } => {
            line(out, f.id, pts.clone(), true, stroke);
            let inner = housecraft_geom::math::inset_polygon(&pts, 50.0);
            if inner.len() >= 3 {
                line(out, f.id, inner, true, thin);
            }
        }
        SiteKind::SteppingStones { spacing, size } => {
            for (p, _) in housecraft_geom::site::along(&pts, false, spacing.mm().max(100.0)) {
                line(out, f.id, arc_points(p, size.mm() / 2.0, 0.0, 360.0), true, stroke);
            }
        }
        SiteKind::RopeLight { .. } => {
            line(out, f.id, pts.clone(), false, Stroke { dash: &[0.6, 0.6], ..stroke });
        }
        SiteKind::Grass { .. } => {
            line(out, f.id, pts.clone(), true, Stroke { dash: &[1.0, 1.0], ..thin });
            // A few tufts as a hatch.
            if let Some((lo, hi)) = housecraft_geom::math::bounds(pts.iter().copied()) {
                let step = ((hi - lo).max_element() / 12.0).max(300.0);
                let mut y = lo.y + step / 2.0;
                while y < hi.y {
                    let mut x = lo.x + step / 2.0;
                    while x < hi.x {
                        let p = DVec2::new(x, y);
                        if housecraft_geom::math::point_in_polygon(p, &pts) {
                            line(out, f.id, vec![p + DVec2::new(-60.0, 0.0), p + DVec2::new(0.0, 90.0), p + DVec2::new(60.0, 0.0)], false, thin);
                        }
                        x += step;
                    }
                    y += step;
                }
            }
        }
        SiteKind::Stream { width } => {
            let smooth = spline_points(&pts, false);
            let (l, r) = sides(&smooth, width.mm() / 2.0);
            line(out, f.id, l, false, stroke);
            line(out, f.id, r, false, stroke);
        }
    }
}

/// A plant's canopy: a scalloped circle with a centre mark and its name.
pub(crate) fn plant(p: &Plant, stroke: Stroke, ink: Ink, out: &mut Drawing) {
    let c = v2(p.at);
    let (_, spread) = p.size_now();
    let r = spread.mm().max(100.0) / 2.0;
    let lobes = match p.form {
        PlantForm::Conifer => 16,
        PlantForm::Grass => 20,
        _ => 10,
    };
    let canopy: Vec<DVec2> = (0..lobes * 6)
        .map(|i| {
            let a = i as f64 / (lobes * 6) as f64 * std::f64::consts::TAU;
            let wave = if p.form == PlantForm::Conifer {
                // Spiky.
                if i % 6 < 3 { 1.0 } else { 0.85 }
            } else {
                0.92 + 0.08 * ((i % 6) as f64 / 6.0 * std::f64::consts::PI).sin()
            };
            c + DVec2::from_angle(a) * r * wave
        })
        .collect();
    line(out, p.id, canopy, true, stroke);
    let s = (r * 0.08).clamp(40.0, 200.0);
    line(out, p.id, vec![c - DVec2::new(s, 0.0), c + DVec2::new(s, 0.0)], false, stroke);
    line(out, p.id, vec![c - DVec2::new(0.0, s), c + DVec2::new(0.0, s)], false, stroke);
    label(out, p.id, c + DVec2::new(r * 0.75, -r * 0.75), p.name.clone(), ink);
}
