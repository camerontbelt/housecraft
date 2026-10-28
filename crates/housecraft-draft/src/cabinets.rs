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
            let end = if a.reversed { -1.0 } else { 1.0 };
            line(out, id, circle(f.world(DVec2::new(end * (w / 2.0 - 8.0 * IN), d / 2.0)), 1.0 * IN), true, stroke);
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
        K::QueenBed => {
            // Headboard, pillows and the turned-down sheet.
            line(out, id, vec![f.world(DVec2::new(-w / 2.0, 3.0 * IN)), f.world(DVec2::new(w / 2.0, 3.0 * IN))], false, stroke);
            for u in [-w / 4.0, w / 4.0] {
                let r = [DVec2::new(u - 11.0 * IN, 5.0 * IN), DVec2::new(u + 11.0 * IN, 5.0 * IN), DVec2::new(u + 11.0 * IN, 20.0 * IN), DVec2::new(u - 11.0 * IN, 20.0 * IN)];
                line(out, id, r.iter().map(|p| f.world(*p)).collect(), true, stroke);
            }
            line(out, id, vec![f.world(DVec2::new(-w / 2.0, 28.0 * IN)), f.world(DVec2::new(w / 2.0, 28.0 * IN))], false, stroke);
        }
        K::Sofa | K::ArmChair => {
            let arm = 6.0 * IN;
            let inner = [DVec2::new(-w / 2.0 + arm, d), DVec2::new(-w / 2.0 + arm, 9.0 * IN), DVec2::new(w / 2.0 - arm, 9.0 * IN), DVec2::new(w / 2.0 - arm, d)];
            line(out, id, inner.iter().map(|p| f.world(*p)).collect(), false, stroke);
        }
        K::DiningChair => {
            line(out, id, vec![f.world(DVec2::new(-w / 2.0, 1.5 * IN)), f.world(DVec2::new(w / 2.0, 1.5 * IN))], false, stroke);
        }
        K::Vase => {
            line(out, id, circle(centroid(&outline), w / 2.0), true, stroke);
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

pub(crate) fn solid(x: &Solid, stroke: Stroke, out: &mut Drawing) {
    let outline = housecraft_geom::solid::footprint(x);
    if outline.len() >= 3 {
        line(out, x.id, outline, true, stroke);
    }
    if let SolidShape::Revolve { center, profile, .. } = &x.shape {
        // Any inner radius (a ring or a hollow) shows as a second circle.
        let inner = profile.iter().map(|p| p[0].mm()).fold(f64::MAX, f64::min);
        if inner > 1.0 {
            let c = v2(*center);
            let ring: Vec<DVec2> = (0..32)
                .map(|i| c + DVec2::from_angle(i as f64 / 32.0 * std::f64::consts::TAU) * inner)
                .collect();
            line(out, x.id, ring, true, stroke);
        }
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

pub(crate) fn molding_polyline(m: &MoldingPolyline, stroke: Stroke, out: &mut Drawing) {
    let path: Vec<DVec2> = m.points.iter().map(|p| v2(*p)).collect();
    if path.len() < 2 {
        return;
    }
    let depth = m.stack.iter().map(|s| s.depth.mm()).fold(0.0, f64::max);
    let n = path.len();
    let offset: Vec<DVec2> = (0..n)
        .map(|i| {
            let (a, c) = if m.closed {
                (path[(i + n - 1) % n], path[(i + 1) % n])
            } else {
                (path[i.saturating_sub(1)], path[(i + 1).min(n - 1)])
            };
            path[i] - housecraft_geom::math::perp((c - a).normalize_or_zero()) * depth
        })
        .collect();
    line(out, m.id, path, m.closed, stroke);
    line(out, m.id, offset, m.closed, dashed(stroke));
}

pub(crate) fn material_region(doc: &Document, r: &MaterialRegion, stroke: Stroke, out: &mut Drawing) {
    match &r.target {
        RegionTarget::Floor { points } => {
            let pts: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
            line(out, r.id, pts.clone(), true, stroke);
            // A patterned floor shows its tile joints (a vector hatch).
            let Some(p) = doc.get::<Material>(r.material).and_then(|m| m.pattern) else {
                return;
            };
            let Some((lo, hi)) = housecraft_geom::math::bounds(pts.iter().copied()) else {
                return;
            };
            let thin = Stroke { width: stroke.width * 0.5, ..stroke };
            let (sx, sy) = (p.tile_width.mm() + p.grout.mm(), p.tile_height.mm() + p.grout.mm());
            let clip = |a: DVec2, b: DVec2| -> Vec<[DVec2; 2]> {
                // Keep the parts of a joint line inside the outline.
                let mut ts = vec![0.0, 1.0];
                for i in 0..pts.len() {
                    let (c, d) = (pts[i], pts[(i + 1) % pts.len()]);
                    if let Some((_, t, _)) = housecraft_geom::math::segment_intersection(a, b, c, d) {
                        ts.push(t);
                    }
                }
                ts.sort_by(f64::total_cmp);
                ts.windows(2)
                    .filter_map(|w| {
                        let mid = a.lerp(b, (w[0] + w[1]) / 2.0);
                        housecraft_geom::math::point_in_polygon(mid, &pts)
                            .then(|| [a.lerp(b, w[0]), a.lerp(b, w[1])])
                    })
                    .collect()
            };
            let mut y = lo.y + sy;
            while y < hi.y && sy > 1.0 {
                for [a, b] in clip(DVec2::new(lo.x, y), DVec2::new(hi.x, y)) {
                    line(out, r.id, vec![a, b], false, thin);
                }
                y += sy;
            }
            let mut x = lo.x + sx;
            while x < hi.x && sx > 1.0 {
                for [a, b] in clip(DVec2::new(x, lo.y), DVec2::new(x, hi.y)) {
                    line(out, r.id, vec![a, b], false, thin);
                }
                x += sx;
            }
        }
        RegionTarget::Wall { wall, exterior, u0, u1, .. } => {
            if let Some(g) = housecraft_geom::walls::wall_geoms(doc, r.floor).into_iter().find(|g| g.id == *wall) {
                let side = if *exterior { g.left } else { -g.left };
                let p = |u: f64| g.start + g.dir * u + side * (g.half + 30.0);
                line(out, r.id, vec![p(u0.mm()), p(u1.mm())], false, dashed(stroke));
            }
        }
    }
}
