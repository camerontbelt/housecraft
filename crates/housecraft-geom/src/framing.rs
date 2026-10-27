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
    let mut h = doc.wall_height(wall).map_or(floor.ceiling_height.mm(), |h| h.mm());
    if d.wall_style == WallFraming::PostAndGirt {
        return post_and_girt(doc, d, wall, g, core, off);
    }
    // Balloon framing: a wall with one stacked on it carries its studs up
    // through the floor; the wall above frames only its openings.
    let (above, below) = if d.balloon { (stacked(doc, wall, true), stacked(doc, wall, false)) } else { (None, None) };
    let upper_of_balloon = below.is_some();
    let mut ribbon = None;
    if let Some(up) = &above
        && let Some(uf) = doc.get::<Floor>(up.floor)
    {
        let uh = doc.wall_height(up).map_or(uf.ceiling_height.mm(), |h| h.mm());
        ribbon = Some(uf.elevation.mm() - 19.0 - d.floor_joist_depth.mm() - 1.75 * IN);
        h = uf.elevation.mm() + uh - z0;
    }
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
    let mut spans: Vec<(f64, f64, &Opening)> = openings
        .iter()
        .filter_map(|o| opening_spans(g, &[*o]).first().map(|(a, b)| (a.min(*b), a.max(*b), *o)))
        .collect();
    spans.sort_by(|x, y| x.0.total_cmp(&y.0));
    // Openings close together with their heads level share one header.
    let combine = d.combine_headers.mm();
    let head_of = |o: &Opening| o.sill.mm() + o.height.mm();
    let mut header_groups: Vec<(f64, f64, f64)> = Vec::new(); // (from, to, head)
    for (a, b, o) in &spans {
        match header_groups.last_mut() {
            Some(last) if combine > 0.0 && *a - last.1 < combine && (head_of(o) - last.2).abs() < 25.0 => {
                last.1 = last.1.max(*b);
                last.2 = last.2.max(head_of(o));
            }
            _ => header_groups.push((*a, *b, head_of(o))),
        }
    }
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
    if !upper_of_balloon {
        for (s, e) in bottom_runs {
            push(FramingRole::BottomPlate, at(s, z0 + t / 2.0), at(e, z0 + t / 2.0), t, core, 90.0);
        }
        if wall.shape.top.is_empty() || above.is_some() {
            for k in 0..d.top_plates.max(1) {
                let z = z0 + h - t * (k as f64 + 0.5);
                push(FramingRole::TopPlate, at(0.0, z), at(len, z), t, core, 90.0);
            }
        } else {
            // Plates along the raked top, piece by piece between its points.
            let mut us: Vec<f64> = wall.shape.top.iter().map(|p| p.along.mm().clamp(0.0, len)).collect();
            us.push(0.0);
            us.push(len);
            us.sort_by(f64::total_cmp);
            us.dedup_by(|a, b| (*a - *b).abs() < 1.0);
            let top = |u: f64| z0 + wall.shape.top_at(u, h);
            for w in us.windows(2) {
                for k in 0..d.top_plates.max(1) {
                    let dz = t * (k as f64 + 0.5);
                    push(FramingRole::TopPlate, at(w[0], top(w[0] + 0.5) - dz), at(w[1], top(w[1] - 0.5) - dz), t, core, 90.0);
                }
            }
        }
    }
    if let Some(z) = ribbon {
        // Let into the interior face of the studs.
        let inner = (g.center_at(0.0) + g.left * (off - core / 2.0 + 0.375 * IN)).extend(z);
        let inner_end = (g.center_at(len) + g.left * (off - core / 2.0 + 0.375 * IN)).extend(z);
        push(FramingRole::Ribbon, inner, inner_end, 0.75 * IN, 3.5 * IN, 0.0);
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
    // Raked and stepped tops (knee walls under a roof): studs stop under
    // the top, and the top plates follow it.
    let base_h = doc.wall_height(wall).map_or(floor.ceiling_height.mm(), |h| h.mm());
    let raked = !wall.shape.top.is_empty() && !upper_of_balloon && above.is_none();
    let stud_top_at = |u: f64| if raked { z0 + wall.shape.top_at(u, base_h) - top_plates * t } else { stud_top };
    if !upper_of_balloon {
        for u in studs.iter().copied().filter(|u| !blocked(*u)) {
            push(FramingRole::Stud, at(u, stud_bottom), at(u, stud_top_at(u)), t, core, across);
        }
    }
    if d.wall_blocking && !upper_of_balloon {
        let z = (stud_bottom + stud_top) / 2.0;
        let kept: Vec<f64> = studs.iter().copied().filter(|u| !blocked(*u)).collect();
        for w in kept.windows(2) {
            push(FramingRole::Blocking, at(w[0] + t / 2.0, z), at(w[1] - t / 2.0, z), t, core, 90.0);
        }
    }
    // Openings: kings, jacks, a header sized by span, cripples and sills.
    // (Headers stay under this storey's ceiling even in balloon walls.)
    let stud_top = stud_top.min(z0 + doc.wall_height(wall).map_or(floor.ceiling_height.mm(), |h| h.mm()) - top_plates * t);
    for (a, b, o) in &spans {
        let head = z0 + o.sill.mm() + o.height.mm();
        let span = b - a;
        let spec = o.details.header.unwrap_or_default();
        let hd = spec.depth.map(|d| d.mm()).unwrap_or(
            if span <= 36.0 * IN {
                5.5
            } else if span <= 60.0 * IN {
                7.25
            } else if span <= 96.0 * IN {
                9.25
            } else {
                11.25
            } * IN,
        );
        // A flush header fills up to the plates.
        let header_top = if spec.flat {
            (head + 2.0 * t).min(stud_top)
        } else if spec.flush {
            stud_top
        } else {
            (head + hd).min(stud_top)
        };
        for (jack, king) in [(a - t / 2.0, a - 1.5 * t), (b + t / 2.0, b + 1.5 * t)] {
            push(FramingRole::JackStud, at(jack, stud_bottom), at(jack, head), t, core, across);
            push(FramingRole::KingStud, at(king, stud_bottom), at(king, stud_top), t, core, across);
        }
        // One header over a group of openings, drawn with its first one.
        if let Some(group) = header_groups.iter().find(|g| (g.0 - a).abs() < 0.5) {
            let (ga, gb) = (group.0, group.1);
            let head = z0 + group.2;
            push(
                FramingRole::Header,
                at(ga - t, (head + header_top) / 2.0),
                at(gb + t, (head + header_top) / 2.0),
                core,
                header_top - head,
                0.0,
            );
        }
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
    // Furring and other extra framed layers (a 2x4 wall inside a
    // basement's concrete): plates and studs of their own.
    for (lcore, loff) in extra_framing_layers(doc, wall, g) {
        let at = |u: f64, z: f64| (g.center_at(u) + g.left * loff).extend(z);
        let mut push = |role, a: DVec3, b: DVec3, width: f64, depth: f64, rotation: f64| {
            if a.distance(b) > 5.0 {
                out.push(Member { role, a, b, width, depth, rotation, host: wall.id, floor: wall.floor });
            }
        };
        let top = z0 + doc.wall_height(wall).map_or(floor.ceiling_height.mm(), |h| h.mm());
        push(FramingRole::BottomPlate, at(0.0, z0 + t / 2.0), at(len, z0 + t / 2.0), t, lcore, 90.0);
        push(FramingRole::TopPlate, at(0.0, top - t / 2.0), at(len, top - t / 2.0), t, lcore, 90.0);
        let mut u = t / 2.0;
        while u < len {
            if !spans.iter().any(|(a, b, _)| u > a - t && u < b + t) {
                push(FramingRole::Stud, at(u, z0 + t), at(u, top - t), t, lcore, across);
            }
            u = if u + spacing > len - t / 2.0 && u < len - t { len - t / 2.0 } else { u + spacing };
        }
    }
    out
}

/// The wall stacked directly above (`up`) or below this one: same ends.
fn stacked<'a>(doc: &'a Document, wall: &Wall, up: bool) -> Option<&'a Wall> {
    let other = if up {
        doc.floor_above(wall.floor)?.id
    } else {
        let floors = doc.floors_by_elevation();
        let i = floors.iter().position(|f| f.id == wall.floor)?;
        if i == 0 {
            return None;
        }
        let f = floors[i - 1];
        if f.foundation.is_some() {
            return None;
        }
        f.id
    };
    let (a, b) = (v2(wall.start), v2(wall.end));
    doc.walls_on(other).find(|w| {
        let (c, e) = (v2(w.start), v2(w.end));
        (c.distance(a) < 50.0 && e.distance(b) < 50.0) || (c.distance(b) < 50.0 && e.distance(a) < 50.0)
    })
}

/// Layers framed besides the main one (furring): their depth and offset
/// from the wall's centre line toward its exterior side.
fn extra_framing_layers(doc: &Document, wall: &Wall, g: &WallGeom) -> Vec<(f64, f64)> {
    let Some(wt) = doc.get::<WallType>(wall.wall_type) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut before = 0.0;
    for l in &wt.layers {
        let th = l.thickness.mm();
        let name = format!("{} {}", l.name, l.material).to_lowercase();
        if name.contains("furring") && th > 10.0 {
            out.push((th, g.half - before - th / 2.0));
        }
        before += th;
    }
    out
}

/// Pole-building walls: posts, girts between them, a top plate and
/// headers over openings.
fn post_and_girt(doc: &Document, d: &FramingDefaults, wall: &Wall, g: &WallGeom, core: f64, off: f64) -> Vec<Member> {
    let Some(floor) = doc.get::<Floor>(wall.floor) else {
        return Vec::new();
    };
    let z0 = floor.elevation.mm();
    let h = doc.wall_height(wall).map_or(floor.ceiling_height.mm(), |h| h.mm());
    let t = d.lumber_width.mm().max(10.0);
    let post = 5.5 * IN;
    let len = g.length;
    let across = g.left.to_angle().to_degrees();
    let at = |u: f64, z: f64, o: f64| (g.center_at(u) + g.left * o).extend(z);
    let mut out = Vec::new();
    let mut push = |role, a: DVec3, b: DVec3, width: f64, depth: f64, rotation: f64| {
        if a.distance(b) > 5.0 {
            out.push(Member { role, a, b, width, depth, rotation, host: wall.id, floor: wall.floor });
        }
    };
    let spans: Vec<(f64, f64, f64, f64)> = doc
        .openings_in(wall.id)
        .filter_map(|o| opening_spans(g, &[o]).first().map(|(a, b)| (a.min(*b), a.max(*b), z0 + o.sill.mm(), z0 + o.sill.mm() + o.height.mm())))
        .collect();
    // Posts at the ends and every post spacing, inside the main layer.
    let spacing = d.post_spacing.mm().max(600.0);
    let mut posts = vec![post / 2.0];
    let mut u = spacing;
    while u < len - post {
        posts.push(u);
        u += spacing;
    }
    posts.push(len - post / 2.0);
    for u in &posts {
        push(FramingRole::Post, at(*u, z0, off), at(*u, z0 + h - t, off), post, post, across);
    }
    push(FramingRole::TopPlate, at(0.0, z0 + h - t / 2.0, off), at(len, z0 + h - t / 2.0, off), t, post, 90.0);
    // Girts on the outside face of the posts, broken at openings.
    let face = off + post / 2.0 + t / 2.0;
    let girt = d.girt_spacing.mm().max(300.0);
    let mut z = z0 + girt / 2.0;
    while z < z0 + h - t {
        let mut runs = vec![(0.0, len)];
        for (a, b, lo, hi) in &spans {
            if z > *lo - t && z < *hi + t {
                runs = runs
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
        for (s, e) in runs {
            push(FramingRole::Girt, at(s, z, face), at(e, z, face), t, 5.5 * IN, 90.0);
        }
        z += girt;
    }
    for (a, b, _, hi) in &spans {
        push(FramingRole::Header, at(*a, hi + 3.6 * IN, face), at(*b, hi + 3.6 * IN, face), t, 7.25 * IN, 0.0);
    }
    let _ = core;
    out
}

/// Where a line through `o` along `dir` is inside `poly`: (t0, t1) pairs.
pub fn clip_line(poly: &[DVec2], o: DVec2, dir: DVec2) -> Vec<(f64, f64)> {
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
    splice: JoistSplice,
    out: &mut Vec<Member>,
) {
    let start = out.len();
    joists_inner(doc, floor, ceiling, z, spacing, t, depth, holes, out);
    if splice == JoistSplice::Continuous {
        return;
    }
    // Bearing walls under the joists: this floor's (ceiling joists) or the
    // floor below's (floor joists).
    let wall_floor = if ceiling {
        Some(floor)
    } else {
        let floors = doc.floors_by_elevation();
        floors.iter().position(|f| f.id == floor).and_then(|i| i.checked_sub(1)).map(|i| floors[i].id)
    };
    let Some(wall_floor) = wall_floor else { return };
    let bearing: Vec<(DVec2, DVec2)> = doc.walls_on(wall_floor).filter(|w| w.shape.bearing).map(|w| (v2(w.start), v2(w.end))).collect();
    if bearing.is_empty() {
        return;
    }
    let lap = 12.0 * IN;
    let mut spliced = Vec::new();
    for m in out.drain(start..) {
        let role_ok = matches!(m.role, FramingRole::FloorJoist | FramingRole::CeilingJoist);
        let (a, b) = (m.a.truncate(), m.b.truncate());
        let len = a.distance(b);
        let dir = (b - a) / len.max(1e-9);
        // Where the joist crosses each bearing wall, along it.
        let mut cuts: Vec<f64> = bearing
            .iter()
            .filter_map(|(p, q)| {
                let e = *q - *p;
                let den = dir.perp_dot(e);
                if den.abs() < 1e-6 {
                    return None;
                }
                let s = (p - a).perp_dot(e) / den;
                let u = (p - a).perp_dot(dir) / den;
                (s > lap && s < len - lap && (0.0..=1.0).contains(&u)).then_some(s)
            })
            .collect();
        if !role_ok || cuts.is_empty() {
            spliced.push(m);
            continue;
        }
        cuts.sort_by(f64::total_cmp);
        let at = |s: f64, side: f64| (a + dir * s + perp(dir) * side).extend(m.a.z);
        let mut from = 0.0;
        for (k, c) in cuts.iter().chain(std::iter::once(&len)).enumerate() {
            let last = k == cuts.len();
            let (s0, s1, side) = match splice {
                JoistSplice::Lap => (if k == 0 { from } else { from - lap / 2.0 }, if last { *c } else { c + lap / 2.0 }, if k % 2 == 1 { t } else { 0.0 }),
                _ => (from, *c, 0.0),
            };
            spliced.push(Member { a: at(s0, side), b: at(s1, side), ..m.clone() });
            from = *c;
        }
    }
    out.extend(spliced);
}

#[allow(clippy::too_many_arguments)]
fn joists_inner(
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
    joists(doc, floor.id, false, z, floor.joist_spacing.unwrap_or(d.joist_spacing).mm().max(100.0), d.lumber_width.mm(), depth, &stair_holes(doc, floor.id), d.joist_splice, &mut out);
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
    joists(doc, floor.id, true, z, d.ceiling_joist_spacing.mm().max(100.0), d.lumber_width.mm(), depth, &[], d.joist_splice, &mut out);
    tray_framing(doc, floor, d, &mut out);
    out
}

/// Tray ceilings: the joists inside a tray step up by its rise, and a
/// ring of blocking frames the step.
fn tray_framing(doc: &Document, floor: &Floor, d: &FramingDefaults, out: &mut Vec<Member>) {
    let regions = crate::rooms::detect(doc, floor.id);
    let t = d.lumber_width.mm();
    for room in doc.rooms_on(floor.id) {
        let CeilingStyle::Tray { inset, rise, .. } = room.ceiling else { continue };
        let Some(i) = regions.room_at(v2(room.anchor)) else { continue };
        let mut poly = regions.rooms[i].polygon.clone();
        if signed_area(&poly) < 0.0 {
            poly.reverse();
        }
        let inner = inset_polygon(&poly, inset.mm());
        if inner.len() < 3 {
            continue;
        }
        let rise = rise.mm();
        let mut split = Vec::new();
        for m in out.drain(..) {
            if m.role != FramingRole::CeilingJoist {
                split.push(m);
                continue;
            }
            let (a, b) = (m.a.truncate(), m.b.truncate());
            let len = a.distance(b);
            let dir = (b - a) / len.max(1e-9);
            let inside: Vec<(f64, f64)> = clip_line(&inner, a, dir).into_iter().map(|(t0, t1)| (t0.max(0.0), t1.min(len))).filter(|(t0, t1)| t1 > t0).collect();
            if inside.is_empty() {
                split.push(m);
                continue;
            }
            let z = m.a.z;
            let mut from = 0.0;
            for (t0, t1) in inside {
                if t0 > from {
                    split.push(Member { a: (a + dir * from).extend(z), b: (a + dir * t0).extend(z), ..m.clone() });
                }
                split.push(Member { a: (a + dir * t0).extend(z + rise), b: (a + dir * t1).extend(z + rise), ..m.clone() });
                from = t1;
            }
            if from < len {
                split.push(Member { a: (a + dir * from).extend(z), b: b.extend(z), ..m.clone() });
            }
        }
        *out = split;
        // Blocking round the tray's edge, top and bottom of the step.
        let z = floor.elevation.mm() + floor.ceiling_height.mm() + d.ceiling_joist_depth.mm() / 2.0;
        let n = inner.len();
        for k in 0..n {
            let (a, b) = (inner[k], inner[(k + 1) % n]);
            for zz in [z, z + rise] {
                out.push(Member { role: FramingRole::Blocking, a: a.extend(zz), b: b.extend(zz), width: t, depth: d.ceiling_joist_depth.mm(), rotation: 0.0, host: room.id, floor: floor.id });
            }
        }
    }
}

/// Rafters down a roof plane and a ridge along its top edge.
/// `overhang` is the roof's eave overhang (for frieze blocking over the wall line).
pub fn roof_framing(d: &FramingDefaults, plane: &RoofPlane, overhang: f64, ridges: &mut Vec<(DVec2, DVec2)>) -> Vec<Member> {
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
    let spacing = plane.rafter_spacing.unwrap_or(d.rafter_spacing).mm().max(100.0);
    let mut out = Vec::new();
    // Purlins: flat 2x4s across the rafters, every so far down the slope.
    let purlins = d.purlin_spacing.mm();
    if purlins > 50.0 {
        let along: Vec<f64> = flat_ccw.iter().map(|p| p.dot(down)).collect();
        let (dlo, dhi) = (along.iter().copied().fold(f64::MAX, f64::min), along.iter().copied().fold(f64::MIN, f64::max));
        let step = purlins * cos; // plan distance for that slope distance
        let mut u = dlo + step / 2.0;
        while u < dhi - t {
            let o = down * u;
            for (t0, t1) in clip_line(&flat_ccw, o, n) {
                let (a, b) = (o + n * t0, o + n * t1);
                if a.distance(b) > 100.0 {
                    let z = |q: DVec2| height(q) - (19.0 + 0.75 * IN) / cos;
                    out.push(Member { role: FramingRole::Purlin, a: a.extend(z(a)), b: b.extend(z(b)), width: t, depth: 3.5 * IN, rotation: 90.0, host: plane.id, floor: plane.floor });
                }
            }
            u += step;
        }
    }
    let low = pts.iter().map(|p| p.z).fold(f64::MAX, f64::min);
    let m = flat_ccw.len();
    for i in 0..m {
        let (a, b) = (flat_ccw[i], flat_ccw[(i + 1) % m]);
        let e = b - a;
        let len = e.length();
        if len < 300.0 {
            continue;
        }
        let ed = e / len;
        // Eaves: level edges at the bottom. Frieze blocking over the wall line.
        if d.eave_blocking && (height(a) - low).abs() < 2.0 && (height(b) - low).abs() < 2.0 {
            let inward = perp(ed); // the outline is counter-clockwise
            let (p, q) = (a + inward * overhang, b + inward * overhang);
            let z = |x: DVec2| height(x) - drop;
            out.push(Member { role: FramingRole::Blocking, a: p.extend(z(p)), b: q.extend(z(q)), width: t, depth, rotation: 0.0, host: plane.id, floor: plane.floor });
        }
        // Rakes run down the slope: lookouts every 24" across the overhang.
        if d.lookouts && ed.dot(down).abs() > 0.97 {
            let inward = perp(ed);
            let mut u = 12.0 * IN;
            while u < len - 6.0 * IN {
                let p = a + ed * u;
                let q = p + inward * 36.0 * IN;
                let z = |x: DVec2| height(x) - (19.0 + 0.75 * IN) / cos;
                out.push(Member { role: FramingRole::Blocking, a: p.extend(z(p)), b: q.extend(z(q)), width: t, depth: 3.5 * IN, rotation: 90.0, host: plane.id, floor: plane.floor });
                u += 24.0 * IN;
            }
        }
    }
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
    let (dh, wh) = (dd * m.depth.mm() / 2.0, wd * m.width.mm() * m.plies.max(1) as f64 / 2.0);
    let ring = |e: DVec3| [e - dh - wh, e + dh - wh, e + dh + wh, e - dh + wh];
    let (ra, mut rb) = (ring(a), ring(b));
    // A plumb (or undercut) end: slide the end corners along the member
    // onto the vertical plane through `b`.
    let ah = axis.truncate();
    if m.tail != TailCut::Square && ah.length() > 0.05 && axis.z.abs() > 0.01 {
        let ah_n = ah.normalize();
        let k = if m.tail == TailCut::Bevel { 2.0 } else { 1.0 };
        for c in rb.iter_mut() {
            let t = -(*c - b).truncate().dot(ah_n) / ah.dot(ah_n);
            *c += axis * t * k;
        }
    }
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
    hull(corners(m).iter().map(|p| p.truncate()).collect())
}

/// The convex hull of points (counter-clockwise).
pub fn hull(mut pts: Vec<DVec2>) -> Vec<DVec2> {
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

/// A roof plane's height at a plan point, when the point is under it.
fn plane_height(plane: &RoofPlane) -> Option<impl Fn(DVec2) -> Option<f64> + '_> {
    let pts: Vec<DVec3> = plane.outline.iter().map(|p| DVec3::from_array(p.mm())).collect();
    if plane.gable || pts.len() < 3 {
        return None;
    }
    let (p0, p1) = (pts[0], pts[1]);
    let p2 = *pts.iter().max_by(|a, b| {
        let area = |p: &DVec3| (p1 - p0).cross(*p - p0).length();
        area(a).total_cmp(&area(b))
    })?;
    let n = (p1 - p0).cross(p2 - p0);
    if n.z.abs() < 1e-9 {
        return None;
    }
    let flat: Vec<DVec2> = pts.iter().map(|p| p.truncate()).collect();
    Some(move |q: DVec2| {
        point_in_polygon(q, &flat).then(|| p0.z - (n.x * (q.x - p0.x) + n.y * (q.y - p0.y)) / n.z)
    })
}

/// Trusses under the roof planes on `floor`: across the building at the
/// truss spacing, each a bottom chord at the ceiling, top chords under the
/// roof (flat-topped where hips step them down) and Fink webs.
pub fn roof_trusses(doc: &Document, d: &FramingDefaults, floor: &Floor) -> Vec<Member> {
    let planes: Vec<&RoofPlane> = doc.iter::<RoofPlane>().filter(|p| p.floor == floor.id && !p.gable).collect();
    let heights: Vec<_> = planes.iter().filter_map(|p| plane_height(p)).collect();
    if heights.is_empty() {
        return Vec::new();
    }
    let roof = |q: DVec2| heights.iter().filter_map(|h| h(q)).reduce(f64::min);
    let t = d.lumber_width.mm();
    let chord = 3.5 * IN;
    let z_bottom = floor.elevation.mm() + floor.ceiling_height.mm() + chord / 2.0;
    // The trusses run across the longest ridge (or the building's long way).
    let ridge = planes
        .iter()
        .flat_map(|p| {
            let pts: Vec<DVec3> = p.outline.iter().map(|q| DVec3::from_array(q.mm())).collect();
            let top = pts.iter().map(|q| q.z).fold(f64::MIN, f64::max);
            (0..pts.len())
                .map(move |i| (pts[i], pts[(i + 1) % pts.len()]))
                .filter(move |(a, b)| (a.z - top).abs() < 1.0 && (b.z - top).abs() < 1.0)
                .map(|(a, b)| b.truncate() - a.truncate())
                .collect::<Vec<_>>()
        })
        .max_by(|a, b| a.length().total_cmp(&b.length()));
    let mut out = Vec::new();
    for fp in crate::rooms::detect(doc, floor.id).footprints {
        let mut fp = fp;
        if signed_area(&fp) < 0.0 {
            fp.reverse();
        }
        let r = match ridge.filter(|r| r.length() > 300.0) {
            Some(r) => r.normalize(),
            None => {
                let (lo, hi) = crate::math::bounds(fp.iter().copied()).expect("footprint");
                if hi.x - lo.x >= hi.y - lo.y { DVec2::X } else { DVec2::Y }
            }
        };
        let n = perp(r);
        let proj: Vec<f64> = fp.iter().map(|p| p.dot(r)).collect();
        let (lo, hi) = (proj.iter().copied().fold(f64::MAX, f64::min), proj.iter().copied().fold(f64::MIN, f64::max));
        let spacing = d.truss_spacing.mm().max(300.0);
        let mut stations = vec![lo + t];
        let mut k = 1;
        while lo + k as f64 * spacing < hi - t {
            stations.push(lo + k as f64 * spacing);
            k += 1;
        }
        stations.push(hi - t);
        let last_station = stations.len().saturating_sub(1);
        for (si, s) in stations.into_iter().enumerate() {
            let start = out.len();
            let o = r * s;
            for (t0, t1) in clip_line(&fp, o, n) {
                let (e0, e1) = (o + n * t0, o + n * t1);
                let drop = 19.0 + chord * 0.6;
                // Trim the truss to where it fits under the roof (low eaves
                // and hip ends leave no room for the chords).
                let fits = |f: f64| roof(e0.lerp(e1, f)).is_some_and(|z| z - drop >= z_bottom + chord * 0.6);
                let n_steps = 64;
                let Some(first) = (0..=n_steps).find(|i| fits(*i as f64 / n_steps as f64)) else {
                    continue;
                };
                let last = (0..=n_steps).rev().find(|i| fits(*i as f64 / n_steps as f64)).unwrap_or(first);
                let (e0, e1) = (e0.lerp(e1, first as f64 / n_steps as f64), e0.lerp(e1, last as f64 / n_steps as f64));
                let span = e0.distance(e1);
                if span < 600.0 {
                    continue;
                }
                // The top chord's line: under the roof along the truss.
                let at = |f: f64| {
                    let p = e0.lerp(e1, f);
                    // Where the roof comes down to the ceiling the top chord
                    // meets the bottom chord rather than poking up through it.
                    roof(p).map(|z| p.extend((z - drop).max(z_bottom)))
                };
                let samples: Vec<DVec3> = (0..=32).filter_map(|i| at(i as f64 / 32.0)).collect();
                if samples.len() < 33 {
                    continue;
                }
                // Keep the corners of the profile.
                let mut top = vec![samples[0]];
                for w in samples.windows(3) {
                    let (a, b, c) = (w[0], w[1], w[2]);
                    let s1 = (b.z - a.z) / a.truncate().distance(b.truncate()).max(1e-6);
                    let s2 = (c.z - b.z) / b.truncate().distance(c.truncate()).max(1e-6);
                    if (s1 - s2).abs() > 0.01 {
                        top.push(b);
                    }
                }
                top.push(samples[32]);
                let push = |out: &mut Vec<Member>, role, a: DVec3, b: DVec3, rotation: f64| {
                    if a.distance(b) > 30.0 {
                        out.push(Member { role, a, b, width: t, depth: chord, rotation, host: floor.id, floor: floor.id });
                    }
                };
                let top_at = |f: f64| at(f).unwrap_or(e0.lerp(e1, f).extend(z_bottom));
                // Scissor: the bottom chord rises halfway to the top at mid-span.
                let peak = top_at(0.5).z;
                let bottom_z = |f: f64| match d.truss_type {
                    TrussType::Scissor => z_bottom + (peak - z_bottom - chord * 1.5).max(0.0) * 0.5 * (1.0 - (2.0 * f - 1.0).abs()),
                    _ => z_bottom,
                };
                if d.truss_type == TrussType::Scissor {
                    for (f0, f1) in [(0.0, 0.5), (0.5, 1.0)] {
                        push(&mut out, FramingRole::Chord, e0.lerp(e1, f0).extend(bottom_z(f0)), e0.lerp(e1, f1).extend(bottom_z(f1)), 0.0);
                    }
                } else {
                    push(&mut out, FramingRole::Chord, e0.extend(z_bottom), e1.extend(z_bottom), 0.0);
                }
                for w in top.windows(2) {
                    push(&mut out, FramingRole::Chord, w[0], w[1], 0.0);
                }
                let across = n.to_angle().to_degrees();
                // Heels where the top chord stands above the bottom chord.
                for end in [top[0], *top.last().expect("top")] {
                    if end.z - z_bottom > chord * 1.5 {
                        push(&mut out, FramingRole::Web, end.truncate().extend(z_bottom + chord / 2.0), end, across);
                    }
                }
                let bottom_at = |f: f64| e0.lerp(e1, f).extend(bottom_z(f) + chord / 2.0);
                match d.truss_type {
                    // Fink webs.
                    TrussType::Common => {
                        for (b, tp) in [(1.0 / 3.0, 0.25), (1.0 / 3.0, 0.5), (2.0 / 3.0, 0.5), (2.0 / 3.0, 0.75)] {
                            push(&mut out, FramingRole::Web, bottom_at(b), top_at(tp), 0.0);
                        }
                    }
                    // A king post and two struts off the rising chord.
                    TrussType::Scissor => {
                        push(&mut out, FramingRole::Web, bottom_at(0.5), top_at(0.5), 0.0);
                        for (b, tp) in [(0.25, 0.4), (0.75, 0.6)] {
                            push(&mut out, FramingRole::Web, bottom_at(b), top_at(tp), 0.0);
                        }
                    }
                    // A room 1/4–3/4 across: posts up to a ceiling tie about
                    // 7'6" over the floor (or under the roof), webs outside.
                    TrussType::Attic => {
                        let tie_z = (z_bottom + 90.0 * IN).min(top_at(0.3).z - chord);
                        for f in [0.25, 0.75] {
                            let p = e0.lerp(e1, f);
                            if top_at(f).z - z_bottom > chord * 2.0 {
                                push(&mut out, FramingRole::Web, p.extend(z_bottom + chord / 2.0), top_at(f), 0.0);
                            }
                        }
                        if tie_z > z_bottom + 60.0 * IN {
                            let (a, b) = (e0.lerp(e1, 0.3), e0.lerp(e1, 0.7));
                            push(&mut out, FramingRole::Chord, a.extend(tie_z), b.extend(tie_z), 0.0);
                        }
                        for (b, tp) in [(0.1, 0.18), (0.9, 0.82)] {
                            push(&mut out, FramingRole::Web, bottom_at(b), top_at(tp), 0.0);
                        }
                    }
                }
            }
            // Girder trusses: a second truss beside the first and last.
            if d.girder_ends && (si == 0 || si == last_station) {
                let shift = r * t * if si == 0 { 1.0 } else { -1.0 };
                let copies: Vec<Member> = out[start..]
                    .iter()
                    .map(|m| Member { a: m.a + shift.extend(0.0), b: m.b + shift.extend(0.0), ..m.clone() })
                    .collect();
                out.extend(copies);
            }
        }
    }
    out
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
            shape: Default::default(),
            invisible: false,
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
