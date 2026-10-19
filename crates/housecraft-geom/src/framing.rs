//! Automatic framing: wall studs, plates, headers, cripples and sills;
//! floor and ceiling joists with rims and stairwell headers; rafters and
//! ridges. Members are boxes along a centre line.

use glam::{DVec2, DVec3};
use housecraft_model::*;

use crate::math::{inset_polygon, perp, point_in_polygon, signed_area, v2};
use crate::mesh::{Category, Look, Mesh};
use crate::walls::{WallGeom, opening_spans, wall_geoms};

/// A member to create.
#[derive(Debug, Clone)]
pub struct Member {
    pub role: FramingRole,
    pub a: DVec3,
    pub b: DVec3,
    pub width: f64,
    pub depth: f64,
    pub rotation: f64,
    pub host: Id,
    pub floor: Id,
}

const IN: f64 = 25.4;

/// The wall's framing layer: its depth and the offset of its centre from
/// the wall's centre line toward the exterior (left) side.
fn framing_core(doc: &Document, wall: &Wall, g: &WallGeom) -> (f64, f64) {
    let Some(wt) = doc.get::<WallType>(wall.wall_type) else {
        return (3.5 * IN, 0.0);
    };
    let idx = wt
        .layers
        .iter()
        .position(|l| l.name.to_lowercase().contains("fram") || l.material.to_lowercase().contains("stud"))
        .or_else(|| {
            wt.layers
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.thickness.mm().total_cmp(&b.1.thickness.mm()))
                .map(|(i, _)| i)
        });
    let Some(idx) = idx else {
        return (3.5 * IN, 0.0);
    };
    let before: f64 = wt.layers[..idx].iter().map(|l| l.thickness.mm()).sum();
    let t = wt.layers[idx].thickness.mm();
    (t, g.half - before - t / 2.0)
}

/// Studs, plates, headers, jacks, kings, cripples and sills for one wall.
pub fn wall_framing(doc: &Document, d: &FramingDefaults, wall: &Wall, g: &WallGeom) -> Vec<Member> {
    let Some(floor) = doc.get::<Floor>(wall.floor) else {
        return Vec::new();
    };
    let Some(wt) = doc.get::<WallType>(wall.wall_type) else {
        return Vec::new();
    };
    if wt.railing.is_some() || g.length < 50.0 {
        return Vec::new();
    }
    let (core, off) = framing_core(doc, wall, g);
    let t = d.lumber_width.mm().max(10.0);
    let z0 = floor.elevation.mm();
    let h = doc.wall_height(wall).map_or(floor.ceiling_height.mm(), |h| h.mm());
    let top_plates = d.top_plates.max(1) as f64;
    let stud_top = z0 + h - top_plates * t;
    let stud_bottom = z0 + t;
    let at = |u: f64, z: f64| (g.center_at(u) + g.left * off).extend(z);
    let across = g.left.to_angle().to_degrees();
    let mut out = Vec::new();
    let mut push = |role, a: DVec3, b: DVec3, width: f64, depth: f64, rotation: f64| {
        if a.distance(b) > 5.0 {
            out.push(Member { role, a, b, width, depth, rotation, host: wall.id, floor: wall.floor });
        }
    };
    let len = g.length;
    let openings: Vec<&Opening> = doc.openings_in(wall.id).filter(|o| o.renovation != Renovation::Demolish).collect();
    let spans: Vec<(f64, f64, &Opening)> = openings
        .iter()
        .filter_map(|o| opening_spans(g, &[*o]).first().map(|(a, b)| (a.min(*b), a.max(*b), *o)))
        .collect();
    // Plates: the bottom plate stops at door openings.
    let mut bottom_runs = vec![(0.0, len)];
    for (a, b, o) in &spans {
        if o.opening_kind.is_door() {
            bottom_runs = bottom_runs
                .into_iter()
                .flat_map(|(s, e)| {
                    let mut v = Vec::new();
                    if *a > s {
                        v.push((s, a.min(e)));
                    }
                    if *b < e {
                        v.push((b.max(s), e));
                    }
                    v
                })
                .collect();
        }
    }
    for (s, e) in bottom_runs {
        push(FramingRole::BottomPlate, at(s, z0 + t / 2.0), at(e, z0 + t / 2.0), t, core, 90.0);
    }
    for k in 0..d.top_plates.max(1) {
        let z = z0 + h - t * (k as f64 + 0.5);
        push(FramingRole::TopPlate, at(0.0, z), at(len, z), t, core, 90.0);
    }
    // Where each opening's framing goes: no common studs inside it.
    let blocked = |u: f64| spans.iter().any(|(a, b, _)| u > a - 2.0 * t && u < b + 2.0 * t);
    let mut studs: Vec<f64> = Vec::new();
    let spacing = d.stud_spacing.mm().max(100.0);
    studs.push(t / 2.0);
    let mut k = 1;
    while (k as f64) * spacing < len - t / 2.0 {
        studs.push(k as f64 * spacing);
        k += 1;
    }
    studs.push(len - t / 2.0);
    // Corners: extra studs where another wall meets an end.
    let joined = |p: DVec2| {
        wall_geoms(doc, wall.floor)
            .iter()
            .any(|o| o.id != wall.id && (o.start.distance(p) < g.half * 3.0 || o.end.distance(p) < g.half * 3.0))
    };
    let extra = match d.corners {
        CornerFraming::Standard => 2,
        CornerFraming::TwoStud => 1,
        CornerFraming::Ladder => 0,
    };
    for (end, sign) in [(0.0, 1.0), (len, -1.0)] {
        let p = g.center_at(end);
        if !joined(p) {
            continue;
        }
        for k in 1..=extra {
            studs.push(end + sign * (t / 2.0 + k as f64 * t));
        }
        if d.corners == CornerFraming::Ladder {
            let mut z = z0 + 24.0 * IN;
            while z < stud_top - t {
                let u = end + sign * (t + 2.0 * IN);
                push(FramingRole::Blocking, at(end + sign * t, z), at(u, z), t, core, 90.0);
                z += 24.0 * IN;
            }
        }
    }
    studs.sort_by(|a, b| a.total_cmp(b));
    studs.dedup_by(|a, b| (*a - *b).abs() < t * 0.9);
    for u in studs.iter().copied().filter(|u| !blocked(*u)) {
        push(FramingRole::Stud, at(u, stud_bottom), at(u, stud_top), t, core, across);
    }
    if d.wall_blocking {
        let z = (stud_bottom + stud_top) / 2.0;
        let kept: Vec<f64> = studs.iter().copied().filter(|u| !blocked(*u)).collect();
        for w in kept.windows(2) {
            push(FramingRole::Blocking, at(w[0] + t / 2.0, z), at(w[1] - t / 2.0, z), t, core, 90.0);
        }
    }
    // Openings: kings, jacks, a header sized by span, cripples and sills.
    for (a, b, o) in &spans {
        let head = z0 + o.sill.mm() + o.height.mm();
        let span = b - a;
        let hd = if span <= 36.0 * IN {
            5.5
        } else if span <= 60.0 * IN {
            7.25
        } else if span <= 96.0 * IN {
            9.25
        } else {
            11.25
        } * IN;
        let header_top = (head + hd).min(stud_top);
        for (jack, king) in [(a - t / 2.0, a - 1.5 * t), (b + t / 2.0, b + 1.5 * t)] {
            push(FramingRole::JackStud, at(jack, stud_bottom), at(jack, head), t, core, across);
            push(FramingRole::KingStud, at(king, stud_bottom), at(king, stud_top), t, core, across);
        }
        push(
            FramingRole::Header,
            at(a - t, (head + header_top) / 2.0),
            at(b + t, (head + header_top) / 2.0),
            core,
            header_top - head,
            0.0,
        );
        let inside: Vec<f64> = {
            let mut v = Vec::new();
            let mut u = (a / spacing).ceil() * spacing;
            while u < *b {
                v.push(u);
                u += spacing;
            }
            v
        };
        if stud_top - header_top > 2.0 * IN {
            for u in &inside {
                push(FramingRole::Cripple, at(*u, header_top), at(*u, stud_top), t, core, across);
            }
        }
        if !o.opening_kind.is_door() && o.sill.mm() > 3.0 * IN {
            let sill = z0 + o.sill.mm();
            push(FramingRole::Sill, at(*a, sill - t / 2.0), at(*b, sill - t / 2.0), t, core, 90.0);
            for u in &inside {
                push(FramingRole::Cripple, at(*u, stud_bottom), at(*u, sill - t), t, core, across);
            }
        }
    }
    out
}

/// Where a line through `o` along `dir` is inside `poly`: (t0, t1) pairs.
fn clip_line(poly: &[DVec2], o: DVec2, dir: DVec2) -> Vec<(f64, f64)> {
    let n = poly.len();
    let mut ts = Vec::new();
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let e = q - p;
        let den = crate::math::cross(dir, e);
        if den.abs() < 1e-12 {
            continue;
        }
        let w = p - o;
        let t = crate::math::cross(w, e) / den;
        let s = crate::math::cross(w, dir) / den;
        if (0.0..1.0).contains(&s) {
            ts.push(t);
        }
    }
    ts.sort_by(|a, b| a.total_cmp(b));
    ts.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect()
}

/// Remove the parts of (t0, t1) along the line inside any hole.
fn minus_holes(segs: Vec<(f64, f64)>, holes: &[Vec<DVec2>], o: DVec2, dir: DVec2) -> Vec<(f64, f64)> {
    let mut segs = segs;
    for h in holes {
        let cuts = clip_line(h, o, dir);
        segs = segs
            .into_iter()
            .flat_map(|(s, e)| {
                let mut pieces = vec![(s, e)];
                for (c0, c1) in &cuts {
                    pieces = pieces
                        .into_iter()
                        .flat_map(|(a, b)| {
                            let mut v = Vec::new();
                            if *c0 > a {
                                v.push((a, c0.min(b)));
                            }
                            if *c1 < b {
                                v.push((c1.max(a), b));
                            }
                            v.into_iter().filter(|(x, y)| y > x)
                        })
                        .collect();
                }
                pieces
            })
            .collect();
    }
    segs
}

/// The joist direction set for a region, or its short way.
fn joist_axis(doc: &Document, floor: Id, ceiling: bool, poly: &[DVec2]) -> DVec2 {
    if let Some(j) = doc
        .iter::<JoistDirection>()
        .find(|j| j.floor == floor && j.ceiling == ceiling && point_in_polygon(v2(j.at), poly))
    {
        return DVec2::from_angle(j.angle.to_radians());
    }
    let (lo, hi) = crate::math::bounds(poly.iter().copied()).unwrap_or((DVec2::ZERO, DVec2::ONE));
    let size = hi - lo;
    if size.x <= size.y { DVec2::X } else { DVec2::Y }
}

/// Joists across each building footprint at `z` (their centre), with rim
/// joists round the edge and headers round holes.
#[allow(clippy::too_many_arguments)]
fn joists(
    doc: &Document,
    floor: Id,
    ceiling: bool,
    z: f64,
    spacing: f64,
    t: f64,
    depth: f64,
    holes: &[Vec<DVec2>],
    out: &mut Vec<Member>,
) {
    let role = if ceiling { FramingRole::CeilingJoist } else { FramingRole::FloorJoist };
    for fp in crate::rooms::detect(doc, floor).footprints {
        let mut fp = fp;
        if signed_area(&fp) < 0.0 {
            fp.reverse();
        }
        let rim_line = inset_polygon(&fp, t / 2.0);
        let inner = inset_polygon(&fp, t);
        if inner.len() < 3 {
            continue;
        }
        let axis = joist_axis(doc, floor, ceiling, &fp);
        let n = perp(axis);
        let proj: Vec<f64> = inner.iter().map(|p| p.dot(n)).collect();
        let (lo, hi) = (proj.iter().copied().fold(f64::MAX, f64::min), proj.iter().copied().fold(f64::MIN, f64::max));
        let mut s = lo + t / 2.0;
        let mut k = 0;
        while s <= hi - t / 2.0 + 1e-6 {
            let o = n * s;
            let segs = minus_holes(clip_line(&inner, o, axis), holes, o, axis);
            for (t0, t1) in segs {
                if t1 - t0 > 100.0 {
                    let (a, b) = (o + axis * t0, o + axis * t1);
                    out.push(Member { role, a: a.extend(z), b: b.extend(z), width: t, depth, rotation: 0.0, host: floor, floor });
                }
            }
            // The last joist sits against the far edge.
            k += 1;
            let next = lo + k as f64 * spacing;
            s = if s < hi - t / 2.0 - 1.0 && next > hi - t / 2.0 { hi - t / 2.0 } else { next };
        }
        if !ceiling {
            let k = rim_line.len();
            for i in 0..k {
                let (a, b) = (rim_line[i], rim_line[(i + 1) % k]);
                out.push(Member { role: FramingRole::RimJoist, a: a.extend(z), b: b.extend(z), width: t, depth, rotation: 0.0, host: floor, floor });
            }
        }
        // Headers across the joists at each hole.
        for h in holes {
            let k = h.len();
            for i in 0..k {
                let (a, b) = (h[i], h[(i + 1) % k]);
                if ((b - a).normalize_or_zero().dot(axis)).abs() < 0.3 && point_in_polygon((a + b) / 2.0, &fp) {
                    out.push(Member { role: FramingRole::Header, a: a.extend(z), b: b.extend(z), width: 2.0 * t, depth, rotation: 0.0, host: floor, floor });
                }
            }
        }
    }
}

/// Stairwell holes in `floor`'s platform (stairs below with a stairwell).
fn stair_holes(doc: &Document, floor: Id) -> Vec<Vec<DVec2>> {
    doc.iter::<Stair>()
        .filter(|s| s.stairwell && doc.floor_above(s.floor).map(|f| f.id) == Some(floor))
        .flat_map(|s| crate::stairs::geometry(doc, s).footprint())
        .collect()
}

/// Floor joists under a floor that sits on framing (not a slab).
pub fn floor_framing(doc: &Document, d: &FramingDefaults, floor: &Floor) -> Vec<Member> {
    let floors = doc.floors_by_elevation();
    let Some(i) = floors.iter().position(|f| f.id == floor.id) else {
        return Vec::new();
    };
    if i == 0 || floor.foundation.is_some() {
        return Vec::new();
    }
    let below = floors[i - 1];
    if below.foundation.as_ref().is_some_and(|f| f.kind == FoundationKind::Slab) {
        return Vec::new();
    }
    let depth = d.floor_joist_depth.mm();
    let z = floor.elevation.mm() - 19.0 - depth / 2.0;
    let mut out = Vec::new();
    joists(doc, floor.id, false, z, d.joist_spacing.mm().max(100.0), d.lumber_width.mm(), depth, &stair_holes(doc, floor.id), &mut out);
    out
}

/// Ceiling joists on the top floor's walls.
pub fn ceiling_framing(doc: &Document, d: &FramingDefaults, floor: &Floor) -> Vec<Member> {
    if doc.floor_above(floor.id).is_some() || floor.foundation.is_some() {
        return Vec::new();
    }
    let depth = d.ceiling_joist_depth.mm();
    let z = floor.elevation.mm() + floor.ceiling_height.mm() + depth / 2.0;
    let mut out = Vec::new();
    joists(doc, floor.id, true, z, d.ceiling_joist_spacing.mm().max(100.0), d.lumber_width.mm(), depth, &[], &mut out);
    out
}

/// Rafters down a roof plane and a ridge along its top edge.
pub fn roof_framing(d: &FramingDefaults, plane: &RoofPlane, ridges: &mut Vec<(DVec2, DVec2)>) -> Vec<Member> {
    if plane.gable || plane.outline.len() < 3 {
        return Vec::new();
    }
    let pts: Vec<DVec3> = plane.outline.iter().map(|p| DVec3::from_array(p.mm())).collect();
    let flat: Vec<DVec2> = pts.iter().map(|p| p.truncate()).collect();
    // The plane's height function from three spread-out corners.
    let (p0, p1) = (pts[0], pts[1]);
    let p2 = *pts
        .iter()
        .max_by(|a, b| {
            let area = |p: &DVec3| (p1 - p0).cross(*p - p0).length();
            area(a).total_cmp(&area(b))
        })
        .expect("points");
    let normal = (p1 - p0).cross(p2 - p0);
    if normal.z.abs() < 1e-9 {
        return Vec::new();
    }
    let height = |q: DVec2| p0.z - (normal.x * (q.x - p0.x) + normal.y * (q.y - p0.y)) / normal.z;
    let grad = DVec2::new(-normal.x / normal.z, -normal.y / normal.z);
    if grad.length() < 1e-6 {
        return Vec::new();
    }
    let down = -grad.normalize();
    let cos = 1.0 / (1.0 + grad.length_squared()).sqrt();
    let t = d.lumber_width.mm();
    let depth = d.rafter_depth.mm();
    let drop = (19.0 + depth / 2.0) / cos;
    let mut flat_ccw = flat.clone();
    if signed_area(&flat_ccw) < 0.0 {
        flat_ccw.reverse();
    }
    let n = perp(down);
    let proj: Vec<f64> = flat_ccw.iter().map(|p| p.dot(n)).collect();
    let (lo, hi) = (proj.iter().copied().fold(f64::MAX, f64::min), proj.iter().copied().fold(f64::MIN, f64::max));
    let spacing = d.rafter_spacing.mm().max(100.0);
    let mut out = Vec::new();
    let mut s = lo + t;
    while s < hi - t / 2.0 {
        let o = n * s;
        for (t0, t1) in clip_line(&flat_ccw, o, down) {
            let (a, b) = (o + down * (t0 + t), o + down * t1);
            if a.distance(b) > 100.0 {
                out.push(Member {
                    role: FramingRole::Rafter,
                    a: a.extend(height(a) - drop),
                    b: b.extend(height(b) - drop),
                    width: t,
                    depth,
                    rotation: 0.0,
                    host: plane.id,
                    floor: plane.floor,
                });
            }
        }
        s += spacing;
    }
    // The ridge: the highest level edge, once per pair of planes.
    let top = pts.iter().map(|p| p.z).fold(f64::MIN, f64::max);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        if (a.z - top).abs() < 1.0 && (b.z - top).abs() < 1.0 && a.truncate().distance(b.truncate()) > 300.0 {
            let (fa, fb) = (a.truncate(), b.truncate());
            let seen = ridges
                .iter()
                .any(|(x, y)| (x.distance(fa) < 50.0 && y.distance(fb) < 50.0) || (x.distance(fb) < 50.0 && y.distance(fa) < 50.0));
            if !seen {
                ridges.push((fa, fb));
                let rd = depth + 2.0 * IN;
                out.push(Member {
                    role: FramingRole::Ridge,
                    a: fa.extend(top - 19.0 / cos - rd / 2.0),
                    b: fb.extend(top - 19.0 / cos - rd / 2.0),
                    width: t,
                    depth: rd,
                    rotation: 0.0,
                    host: plane.id,
                    floor: plane.floor,
                });
            }
        }
    }
    out
}

/// A member's eight corners: four round `a`, then four round `b`.
pub fn corners(m: &FramingMember) -> [DVec3; 8] {
    let a = DVec3::from_array(m.a.mm());
    let b = DVec3::from_array(m.b.mm());
    let axis = (b - a).normalize_or(DVec3::Z);
    let (dd, wd) = if axis.z.abs() > 0.99 {
        let d = DVec2::from_angle(m.rotation.to_radians()).extend(0.0);
        (d, axis.cross(d).normalize_or(DVec3::X))
    } else {
        let up = (DVec3::Z - axis * axis.z).normalize();
        let side = axis.cross(up);
        let r = m.rotation.to_radians();
        let d = up * r.cos() + side * r.sin();
        (d, axis.cross(d).normalize())
    };
    let (dh, wh) = (dd * m.depth.mm() / 2.0, wd * m.width.mm() / 2.0);
    let ring = |e: DVec3| [e - dh - wh, e + dh - wh, e + dh + wh, e - dh + wh];
    let (ra, rb) = (ring(a), ring(b));
    [ra[0], ra[1], ra[2], ra[3], rb[0], rb[1], rb[2], rb[3]]
}

pub fn member_mesh(m: &FramingMember, mesh: &mut Mesh) {
    let c = corners(m);
    let center = c.iter().copied().sum::<DVec3>() / 8.0;
    let wood = mesh.painted_whole().unwrap_or(match m.material {
        FramingMaterial::Steel => Look { color: [0.72, 0.74, 0.76, 1.0], pbr: [0.35, 0.9, 0.0, 1.0], surface: None, tex: None },
        FramingMaterial::Lumber => Look { color: [0.86, 0.72, 0.5, 1.0], pbr: [0.8, 0.0, 0.0, 1.0], surface: None, tex: None },
        _ => Look { color: [0.78, 0.6, 0.38, 1.0], pbr: [0.7, 0.0, 0.0, 1.0], surface: None, tex: None },
    });
    let mut faces: Vec<Vec<DVec3>> = vec![vec![c[0], c[1], c[2], c[3]], vec![c[4], c[5], c[6], c[7]]];
    for i in 0..4 {
        let j = (i + 1) % 4;
        faces.push(vec![c[i], c[j], c[j + 4], c[i + 4]]);
    }
    for mut f in faces {
        // Wind each face to face outward.
        let n = (f[1] - f[0]).cross(f[2] - f[0]);
        let fc = f.iter().copied().sum::<DVec3>() / f.len() as f64;
        if n.dot(fc - center) < 0.0 {
            f.reverse();
        }
        mesh.polygon_look(&f, wood);
    }
}

/// Every framing member (on `only_floor` when given).
pub fn build(doc: &Document, only_floor: Option<Id>, mesh: &mut Mesh) {
    for m in doc.iter::<FramingMember>() {
        if only_floor.is_some_and(|f| f != m.floor) {
            continue;
        }
        mesh.begin(m.id, Category::Framing);
        member_mesh(m, mesh);
        mesh.end();
    }
}

/// A member's outline in plan (its footprint's convex hull).
pub fn plan_outline(m: &FramingMember) -> Vec<DVec2> {
    let mut pts: Vec<DVec2> = corners(m).iter().map(|p| p.truncate()).collect();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| a.distance(*b) < 0.01);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| crate::math::cross(a - o, b - o);
    let mut lower: Vec<DVec2> = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0.0 {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper: Vec<DVec2> = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0.0 {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_units::{Length, Point2};

    #[test]
    fn a_wall_with_a_window_frames_round_it() {
        let mut doc = Document::new("t");
        let floor = doc.floors_by_elevation()[0].id;
        let d = doc.active_defaults().clone();
        let p = |x: f64, y: f64| Point2(Length::from_feet(x), Length::from_feet(y));
        let wall = doc.insert(Wall {
            id: Id(0),
            floor,
            start: p(0.0, 0.0),
            end: p(12.0, 0.0),
            wall_type: d.exterior_wall_type,
            height: None,
            layer: d.layers.walls,
            footing: None,
            roof: Default::default(),
            renovation: Default::default(),
        });
        doc.insert(Opening {
            id: Id(0),
            wall,
            opening_kind: OpeningKind::Window(WindowStyle::DoubleHung),
            position: Length::from_feet(6.0),
            width: Length::from_feet(3.0),
            height: Length::from_feet(4.0),
            sill: Length::from_feet(3.0),
            hinge: Hinge::Left,
            swing_in: true,
            open_percent: 0.0,
            plan_swing: 90.0,
            renovation: Default::default(),
            layer: d.layers.windows,
            details: Default::default(),
        });
        let w = doc.get::<Wall>(wall).unwrap().clone();
        let g = wall_geoms(&doc, floor).into_iter().find(|g| g.id == wall).unwrap();
        let ms = wall_framing(&doc, &FramingDefaults::default(), &w, &g);
        let count = |r| ms.iter().filter(|m| m.role == r).count();
        assert_eq!(count(FramingRole::BottomPlate), 1);
        assert_eq!(count(FramingRole::TopPlate), 2);
        assert_eq!(count(FramingRole::Header), 1);
        assert_eq!(count(FramingRole::KingStud), 2);
        assert_eq!(count(FramingRole::JackStud), 2);
        assert_eq!(count(FramingRole::Sill), 1);
        assert!(count(FramingRole::Cripple) >= 2);
        // 12' at 16" centres: 10 studs less the ones the window displaces.
        let studs = count(FramingRole::Stud);
        assert!((6..=9).contains(&studs), "{studs} studs");
        // No stud inside the window.
        let (a, b) = (Length::from_feet(4.5).mm(), Length::from_feet(7.5).mm());
        assert!(ms.iter().filter(|m| m.role == FramingRole::Stud).all(|m| m.a.x < a || m.a.x > b));
    }
}
