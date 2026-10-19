//! Plan symbols for cabinets, countertops, appliances and fixtures.

use housecraft_geom::cabinets::{
    appliance_frame, appliance_outline, countertop_outline, front_line, hole_outline,
    outline_with_fillers, sink_hole,
};
use housecraft_geom::math::{centroid, inset_polygon};
use housecraft_geom::{DVec2, v2};
use housecraft_model::*;

use crate::{Drawing, Ink, Prim, Stroke};

const IN: f64 = 25.4;
const HIDDEN: &[f64] = &[1.5, 1.0];

fn line(out: &mut Drawing, id: Id, points: Vec<DVec2>, closed: bool, stroke: Stroke) {
    out.push(Some(id), Prim::Polyline { points, closed, stroke });
}

fn dashed(stroke: Stroke) -> Stroke {
    Stroke { dash: HIDDEN, ..stroke }
}

fn circle(c: DVec2, r: f64) -> Vec<DVec2> {
    (0..20)
        .map(|i| c + DVec2::from_angle(i as f64 / 20.0 * std::f64::consts::TAU) * r)
        .collect()
}

/// A sink bowl: rounded outline and drain.
fn sink(out: &mut Drawing, id: Id, hole: &[DVec2], stroke: Stroke) {
    line(out, id, hole.to_vec(), true, stroke);
    let inner = inset_polygon(hole, 1.0 * IN);
    if inner.len() == hole.len() {
        line(out, id, inner, true, Stroke { width: stroke.width * 0.6, ..stroke });
    }
    line(out, id, circle(centroid(hole), 1.5 * IN), true, stroke);
}

/// Four burners in a rectangle.
fn burners(out: &mut Drawing, id: Id, outline: &[DVec2], stroke: Stroke) {
    let c = centroid(outline);
    let (lo, hi) = housecraft_geom::math::bounds(outline.iter().copied()).unwrap_or((c, c));
    let s = (hi - lo) * 0.22;
    for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        line(
            out,
            id,
            circle(c + DVec2::new(dx * s.x, dy * s.y), s.x.min(s.y) * 0.7),
            true,
            stroke,
        );
    }
}

pub(crate) fn cabinet(c: &Cabinet, stroke: Stroke, label: Option<(f64, Ink)>, out: &mut Drawing) {
    let id = c.id;
    // Doors shown open: swing lines from the hinges.
    let open = c.open.clamp(0.0, 1.0);
    let front = front_line(c);
    if open > 0.0 && front.len() == 2 {
        let t = (front[1] - front[0]).normalize_or_zero();
        let n = housecraft_geom::math::perp(t);
        for cell in housecraft_geom::cabinets::cells(c) {
            if !matches!(cell.item, FaceItem::DoorLeft | FaceItem::DoorRight | FaceItem::GlassDoor) {
                continue;
            }
            let (hinge, w, sign) = if cell.hinge_left {
                (cell.s0, cell.s1 - cell.s0, 1.0)
            } else {
                (cell.s1, cell.s1 - cell.s0, -1.0)
            };
            let p = front[0] + t * hinge + n * 0.75 * IN;
            let a = sign * open * 95f64.to_radians();
            let dir = DVec2::from_angle(a).rotate(t * sign);
            line(out, id, vec![p, p + dir * w], false, stroke);
        }
    }
    if let Some((h, ink)) = label {
        let at = centroid(&outline_with_fillers(c));
        out.push(
            Some(id),
            Prim::Text {
                at,
                text: c.code(),
                height: h,
                rotation: 0.0,
                justify: Justify::Center,
                middle: true,
                bold: false,
                ink,
            },
        );
    }
    let outline = outline_with_fillers(c);
    match c.cabinet_kind {
        CabinetKind::Base => {
            if let Some(top) = countertop_outline(c) {
                line(out, id, top, true, stroke);
                // The box front, hidden under the counter.
                line(out, id, front_line(c), false, dashed(stroke));
            } else {
                line(out, id, outline.clone(), true, stroke);
            }
            if let Some(hole) = sink_hole(c) {
                sink(out, id, &hole, stroke);
            }
        }
        CabinetKind::Wall | CabinetKind::Full => {
            let s = if c.cabinet_kind == CabinetKind::Wall {
                dashed(stroke)
            } else {
                stroke
            };
            line(out, id, outline.clone(), true, s);
            // Corner-to-corner cross marks a cabinet above or full height.
            let n = outline.len();
            if n >= 4 {
                line(out, id, vec![outline[0], outline[n / 2]], false, s);
                line(out, id, vec![outline[1], outline[(n / 2 + 1) % n]], false, s);
            }
        }
    }
}

pub(crate) fn countertop(t: &Countertop, stroke: Stroke, out: &mut Drawing) {
    let id = t.id;
    line(out, id, t.points.iter().map(|p| v2(*p)).collect(), true, stroke);
    for h in &t.holes {
        let o = hole_outline(h);
        match h.hole_kind {
            HoleKind::Sink => sink(out, id, &o, stroke),
            HoleKind::Cooktop => {
                line(out, id, o.clone(), true, stroke);
                burners(out, id, &o, stroke);
            }
            HoleKind::Round => line(out, id, o, true, stroke),
        }
    }
}

pub(crate) fn appliance(a: &Appliance, stroke: Stroke, text: f64, ink: Ink, out: &mut Drawing) {
    use ApplianceKind as K;
    let id = a.id;
    let outline = appliance_outline(a);
    let f = appliance_frame(a);
    // Above the counter or under it: hidden lines.
    let hidden = matches!(a.appliance_kind, K::Dishwasher | K::Microwave | K::RangeHood | K::WallOven);
    let s = if hidden { dashed(stroke) } else { stroke };
    line(out, id, outline.clone(), true, s);
    let (w, d) = (a.width.mm(), a.depth.mm());
    match a.appliance_kind {
        K::Range | K::Cooktop => burners(out, id, &outline, stroke),
        K::Bathtub => {
            let inner = inset_polygon(&outline, 3.0 * IN);
            if inner.len() == outline.len() {
                line(out, id, inner, true, stroke);
            }
            line(out, id, circle(f.world(DVec2::new(w / 2.0 - 8.0 * IN, d / 2.0)), 1.0 * IN), true, stroke);
        }
        K::Shower => {
            line(out, id, vec![outline[0], outline[2]], false, stroke);
            line(out, id, vec![outline[1], outline[3]], false, stroke);
            line(out, id, circle(centroid(&outline), 1.0 * IN), true, stroke);
        }
        K::VanitySink | K::Pedestal => {
            let c = f.world(DVec2::new(0.0, d / 2.0));
            let ring: Vec<DVec2> = (0..24)
                .map(|i| {
                    let t = i as f64 / 24.0 * std::f64::consts::TAU;
                    f.world(DVec2::new(w * 0.35 * t.cos(), d / 2.0 + d * 0.3 * t.sin()))
                })
                .collect();
            line(out, id, ring, true, stroke);
            line(out, id, circle(c, 0.75 * IN), true, stroke);
        }
        K::WaterHeater => {
            line(out, id, circle(centroid(&outline), w / 2.0 * 0.9), true, stroke);
        }
        _ => {}
    }
    let label = a.appliance_kind.short();
    if !label.is_empty() {
        // Things mounted above the counter label near the wall, so they
        // don't sit on top of what is below them.
        let at = if matches!(a.appliance_kind, K::Microwave | K::RangeHood) {
            f.world(DVec2::new(0.0, d * 0.2))
        } else {
            centroid(&outline)
        };
        out.push(
            Some(id),
            Prim::Text {
                at,
                text: label.to_string(),
                height: text,
                rotation: 0.0,
                justify: Justify::Center,
                middle: true,
                bold: false,
                ink,
            },
        );
    }
}

pub(crate) fn backsplash(b: &Backsplash, stroke: Stroke, out: &mut Drawing) {
    let path: Vec<DVec2> = b.points.iter().map(|p| v2(*p)).collect();
    if path.len() < 2 {
        return;
    }
    // The face line, offset by the thickness to the right of the path.
    let t = b.thickness.mm();
    let offset: Vec<DVec2> = (0..path.len())
        .map(|i| {
            let a = path[i.saturating_sub(1)];
            let c = path[(i + 1).min(path.len() - 1)];
            let n = -housecraft_geom::math::perp((c - a).normalize_or_zero());
            path[i] + n * t
        })
        .collect();
    line(out, b.id, path, false, stroke);
    line(out, b.id, offset, false, stroke);
}
