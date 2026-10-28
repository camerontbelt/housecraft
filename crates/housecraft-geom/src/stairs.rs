//! Stair geometry: risers from the floor-to-floor height, flights and landings
//! for straight, L and U stairs, plus walk line and rail paths.

use glam::{DVec2, DVec3};
use housecraft_model::{Document, Stair, StairSection, StairShape, Turn};

use crate::math::{perp, v2};

/// One tread (or landing) as a convex plan quad with its top height.
#[derive(Debug, Clone)]
pub struct Step {
    pub poly: Vec<DVec2>,
    /// Top surface height, absolute mm.
    pub z: f64,
    /// Riser number (1 = first step up).
    pub index: u32,
    pub landing: bool,
    /// The tread's nosing strip projecting past the riser (empty if none).
    pub nosing: Vec<DVec2>,
    /// Ramps: the height at the bottom edge (the top surface slopes from
    /// there up to `z` at the far edge).
    pub low_z: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct StairGeom {
    pub risers: u32,
    pub riser_height: f64,
    pub base_z: f64,
    pub steps: Vec<Step>,
    /// Centre walk line from bottom to top, with heights at each point.
    pub walk: Vec<DVec3>,
    /// Handrail paths (at rail height) for each enabled side of each flight.
    pub rails: Vec<Vec<DVec3>>,
    /// Treads per flight, for labels.
    pub flights: Vec<u32>,
    /// Top edges of the side stringers, one per side of each flight.
    pub stringers: Vec<[DVec3; 2]>,
}

impl StairGeom {
    /// Convex pieces covering the stair in plan (for stairwell openings).
    pub fn footprint(&self) -> Vec<Vec<DVec2>> {
        self.steps.iter().map(|s| s.poly.clone()).collect()
    }
}

fn quad(origin: DVec2, dir: DVec2, from: f64, to: f64, left: f64, right: f64) -> Vec<DVec2> {
    let side = perp(dir);
    vec![
        origin + dir * from - side * right,
        origin + dir * to - side * right,
        origin + dir * to + side * left,
        origin + dir * from + side * left,
    ]
}

/// Treads in a stair with `risers` risers (the top riser lands on the floor above).
pub fn tread_count(risers: u32, shape: &StairShape) -> u32 {
    let treads = risers.saturating_sub(1);
    match shape {
        StairShape::Straight | StairShape::Curved { .. } => treads,
        // The landing takes one tread's place.
        _ => treads.saturating_sub(1),
    }
}

pub fn geometry(doc: &Document, stair: &Stair) -> StairGeom {
    geometry_with(doc, stair, true)
}

/// Where a stair starting at `p` on `floor` begins: on another stair's
/// landing (auto-connect), its height.
fn landing_under(doc: &Document, stair: &Stair) -> Option<f64> {
    let p = v2(stair.start);
    doc.iter::<Stair>()
        .filter(|o| o.id != stair.id && o.floor == stair.floor)
        .flat_map(|o| geometry_with(doc, o, false).steps.into_iter().filter(|s| s.landing))
        .filter(|s| crate::math::point_in_polygon(p, &s.poly))
        .map(|s| s.z)
        .reduce(f64::max)
}

/// Risers a sectioned stair has: its treads, its landings (two for a split
/// one) and the step onto the floor above (none for landings alone).
fn section_risers(sections: &[StairSection]) -> (u32, u32) {
    let mut treads = 0;
    let mut landings = 0;
    for s in sections {
        match s {
            StairSection::Flight { treads: n, .. } => treads += n,
            StairSection::Landing { split, .. } => landings += if *split { 2 } else { 1 },
        }
    }
    (treads, landings)
}

/// Where a stair down to grade starts at the top: the floor, or the top of
/// a deck whose edge it starts on.
pub fn grade_top(doc: &Document, stair: &Stair) -> f64 {
    let floor_z = doc.get::<housecraft_model::Floor>(stair.floor).map_or(0.0, |f| f.elevation.mm());
    let p = v2(stair.start);
    doc.iter::<housecraft_model::Deck>()
        .filter(|d| d.floor == stair.floor)
        .filter(|d| {
            let outline: Vec<DVec2> = d.outline.iter().map(|q| v2(*q)).collect();
            crate::math::distance_to_polyline(p, &outline, true) < 300.0
        })
        .map(|d| floor_z + d.height.mm())
        .reduce(f64::max)
        .unwrap_or(floor_z)
}

/// A stair down to grade laid out as an ordinary one climbing from the
/// ground: the same stair moved to its bottom, with the rise it needs, and
/// the height of its bottom.
fn to_grade(doc: &Document, stair: &Stair) -> Option<(Stair, f64)> {
    if !stair.extras.to_grade || stair.shape != StairShape::Straight {
        return None;
    }
    let top = grade_top(doc, stair);
    let ground = crate::site::ground(doc);
    let dir = DVec2::from_angle(stair.direction.to_radians());
    let start = v2(stair.start);
    let (mut run, mut rise) = (0.0, 0.0);
    // The ground under the bottom step sets the rise, which sets how far out
    // the bottom is: a few rounds settle it.
    for _ in 0..4 {
        rise = top - ground(start - dir * run);
        if rise < 50.0 {
            return None;
        }
        let risers = ((rise / stair.max_riser.mm()).ceil() as u32).max(1);
        run = tread_count(risers, &stair.shape) as f64 * stair.tread_depth.mm();
    }
    let mut s = stair.clone();
    s.start = crate::math::pt(start - dir * run);
    s.rise = Some(housecraft_units::Length::from_mm(rise));
    s.extras.to_grade = false;
    Some((s, top - rise))
}

pub fn geometry_with(doc: &Document, stair: &Stair, connect: bool) -> StairGeom {
    let graded = to_grade(doc, stair);
    let stair = graded.as_ref().map_or(stair, |(s, _)| s);
    let floor_z = doc
        .get::<housecraft_model::Floor>(stair.floor)
        .map_or(0.0, |f| f.elevation.mm());
    // A stair that starts on another stair's landing climbs from there.
    let landing = if connect && graded.is_none() { landing_under(doc, stair) } else { None };
    let base_z = graded.as_ref().map(|g| g.1).or(landing).unwrap_or(floor_z);
    let rise = match (landing, stair.rise) {
        (Some(_), None) => (doc.stair_rise(stair).mm() - (base_z - floor_z)).max(1.0),
        _ => doc.stair_rise(stair).mm().max(1.0),
    };
    let risers = if stair.shape == StairShape::Sections {
        let (t, l) = section_risers(&stair.sections);
        (t + l + u32::from(t > 0)).max(1)
    } else {
        ((rise / stair.max_riser.mm()).ceil() as u32).max(1)
    };
    let r = rise / risers as f64;
    let d = stair.tread_depth.mm();
    let w = stair.width.mm();
    let half = w / 2.0;
    let dir = DVec2::from_angle(stair.direction.to_radians());
    let origin = v2(stair.start);
    let rail_h = stair.railing_height.mm();
    let inset = 50.0;

    let mut steps = Vec::new();
    let mut rails = Vec::new();
    let mut walk = vec![DVec3::new(origin.x, origin.y, base_z)];
    let mut flights = Vec::new();
    let mut stringers = Vec::new();
    let mut riser = 0u32;
    let overhang = stair.tread_overhang.mm().max(0.0);
    let stringer_inset = 19.0;

    // Lay out one flight of `n` treads starting at `start`, travelling `fdir`.
    let mut flight = |start: DVec2,
                      fdir: DVec2,
                      n: u32,
                      fhalf: f64,
                      riser: &mut u32,
                      steps: &mut Vec<Step>,
                      walk: &mut Vec<DVec3>| {
        let half = fhalf;
        let first = *riser;
        for i in 0..n {
            *riser += 1;
            steps.push(Step {
                poly: quad(start, fdir, i as f64 * d, (i + 1) as f64 * d, half, half),
                z: base_z + *riser as f64 * r,
                index: *riser,
                landing: false,
                low_z: None,
                nosing: if overhang > 0.0 {
                    quad(
                        start,
                        fdir,
                        i as f64 * d - overhang,
                        i as f64 * d,
                        half,
                        half,
                    )
                } else {
                    Vec::new()
                },
            });
        }
        let end = start + fdir * n as f64 * d;
        walk.push(DVec3::new(end.x, end.y, base_z + *riser as f64 * r));
        if stair.stringers && n > 0 {
            for sign in [1.0, -1.0] {
                let off = perp(fdir) * (half - stringer_inset) * sign;
                let (a, b) = (start + off, end + off);
                stringers.push([
                    DVec3::new(a.x, a.y, base_z + (first + 1) as f64 * r),
                    DVec3::new(b.x, b.y, base_z + *riser as f64 * r),
                ]);
            }
        }
        // Rails rise with the nosings: from the first riser to the last tread.
        for (enabled, sign) in [(stair.railing_left, 1.0), (stair.railing_right, -1.0)] {
            if enabled && n > 0 {
                let off = perp(fdir) * (half - inset) * sign;
                let a = start + off;
                let b = end + off;
                rails.push(vec![
                    DVec3::new(a.x, a.y, base_z + first as f64 * r + r + rail_h),
                    DVec3::new(b.x, b.y, base_z + *riser as f64 * r + rail_h),
                ]);
            }
        }
        end
    };

    let total = tread_count(risers, &stair.shape);
    if stair.extras.ramp {
        return ramp(stair, rise, base_z, origin, dir, half);
    }
    match stair.shape {
        StairShape::Straight => {
            flight(origin, dir, total, half, &mut riser, &mut steps, &mut walk);
            flights.push(total);
        }
        StairShape::Sections => {
            let (mut pos, mut fdir, mut fw) = (origin, dir, w);
            for sec in &stair.sections {
                match *sec {
                    StairSection::Flight { treads, width } => {
                        fw = width.map_or(w, |l| l.mm().max(300.0));
                        pos = flight(pos, fdir, treads, fw / 2.0, &mut riser, &mut steps, &mut walk);
                        flights.push(treads);
                    }
                    StairSection::Landing { turn, u_turn, gap, depth, split } => {
                        let hw = fw / 2.0;
                        let dl = depth.map_or(fw, |l| l.mm().max(300.0));
                        riser += 1;
                        let z1 = base_z + riser as f64 * r;
                        let mut land = |poly: Vec<DVec2>, z: f64, index: u32| {
                            steps.push(Step { poly, z, index, landing: true, nosing: Vec::new(), low_z: None });
                        };
                        match turn {
                            // Straight on: split into near and far halves.
                            None => {
                                if split {
                                    land(quad(pos, fdir, 0.0, dl / 2.0, hw, hw), z1, riser);
                                    riser += 1;
                                    land(quad(pos, fdir, dl / 2.0, dl, hw, hw), base_z + riser as f64 * r, riser);
                                } else {
                                    land(quad(pos, fdir, 0.0, dl, hw, hw), z1, riser);
                                }
                                pos += fdir * dl;
                                walk.push(pos.extend(base_z + riser as f64 * r));
                            }
                            Some(t) => {
                                let side = if t == Turn::Left { 1.0 } else { -1.0 };
                                let sp = perp(fdir) * side;
                                if u_turn {
                                    // Turning back: the landing spans both flights.
                                    let g = gap.mm().max(0.0);
                                    let (l, rr) = if side > 0.0 { (hw + g + fw, hw) } else { (hw, hw + g + fw) };
                                    if split {
                                        land(quad(pos, fdir, 0.0, dl, hw, hw), z1, riser);
                                        riser += 1;
                                        let (l2, r2) = if side > 0.0 { (l, -hw) } else { (-hw, rr) };
                                        land(quad(pos, fdir, 0.0, dl, l2, r2), base_z + riser as f64 * r, riser);
                                    } else {
                                        land(quad(pos, fdir, 0.0, dl, l, rr), z1, riser);
                                    }
                                    let shift = sp * (fw + g);
                                    for q in [pos + fdir * dl / 2.0, pos + fdir * dl / 2.0 + shift] {
                                        walk.push(q.extend(base_z + riser as f64 * r));
                                    }
                                    pos += shift;
                                    fdir = -fdir;
                                } else {
                                    // A quarter turn: split along the diagonal from
                                    // the far outside corner to the near inside one.
                                    let a = pos - sp * hw;
                                    let b = pos + fdir * dl - sp * hw;
                                    let c = pos + fdir * dl + sp * hw;
                                    let e = pos + sp * hw;
                                    if split {
                                        land(vec![a, b, e], z1, riser);
                                        riser += 1;
                                        land(vec![b, c, e], base_z + riser as f64 * r, riser);
                                    } else {
                                        land(vec![a, b, c, e], z1, riser);
                                    }
                                    let center = pos + fdir * dl / 2.0;
                                    walk.push(center.extend(base_z + riser as f64 * r));
                                    pos = center + sp * hw;
                                    fdir = sp;
                                }
                            }
                        }
                    }
                }
            }
        }
        StairShape::Curved { turn, radius } => {
            // Wedge treads round a centre on the turning side; the tread depth
            // is measured on the walk line (the middle of the stair).
            let side = if turn == Turn::Left { 1.0 } else { -1.0 };
            let inner = radius.mm().max(100.0);
            let centre = origin + perp(dir) * side * (inner + half);
            let a0 = (origin - centre).to_angle();
            let step_a = d / (inner + half) * side;
            let at = |a: f64, rr: f64| centre + DVec2::from_angle(a) * rr;
            let n = total;
            for i in 0..n {
                riser += 1;
                let (a, b) = (a0 + step_a * i as f64, a0 + step_a * (i + 1) as f64);
                steps.push(Step {
                    poly: vec![at(a, inner), at(a, inner + w), at(b, inner + w), at(b, inner)],
                    z: base_z + riser as f64 * r,
                    index: riser,
                    landing: false,
                    nosing: Vec::new(),
                    low_z: None,
                });
            }
            walk.clear();
            for k in 0..=n * 4 {
                let a = a0 + step_a * k as f64 / 4.0;
                walk.push(at(a, inner + half).extend(base_z + (k as f64 / 4.0).min(n as f64) * r));
            }
            for (enabled, rr) in [(stair.railing_left, if side > 0.0 { inner + inset } else { inner + w - inset }), (stair.railing_right, if side > 0.0 { inner + w - inset } else { inner + inset })] {
                if enabled && n > 0 {
                    rails.push((0..=n * 4).map(|k| at(a0 + step_a * k as f64 / 4.0, rr).extend(base_z + r + (k as f64 / 4.0) * r + rail_h)).collect());
                }
            }
            flights.push(n);
        }
        StairShape::L { turn, first_treads }
        | StairShape::U {
            turn, first_treads, ..
        } => {
            let k = first_treads.min(total);
            let end = flight(origin, dir, k, half, &mut riser, &mut steps, &mut walk);
            flights.push(k);
            let side = if turn == Turn::Left { 1.0 } else { -1.0 };
            riser += 1;
            let landing_z = base_z + riser as f64 * r;
            let rest = total - k;
            match stair.shape {
                StairShape::L { .. } if stair.extras.winders => {
                    // Three winders fanning from the inside corner of the turn.
                    let pivot = end + perp(dir) * side * half;
                    let corners = [end - perp(dir) * side * half, end + dir * w - perp(dir) * side * half, end + dir * w + perp(dir) * side * half];
                    // Points on the outer boundary at 0°, 30°, 60° and 90° round the pivot.
                    let boundary_at = |k: usize| -> DVec2 {
                        let ang = std::f64::consts::FRAC_PI_6 * k as f64;
                        let ray = -perp(dir) * side * ang.cos() + dir * ang.sin();
                        let mut best = corners[0];
                        for e in [(corners[0], corners[1]), (corners[1], corners[2])] {
                            if let Some(p) = crate::math::line_intersection(pivot, ray, e.0, e.1 - e.0) {
                                let t = (p - e.0).dot(e.1 - e.0) / (e.1 - e.0).length_squared();
                                if (-1e-6..=1.0 + 1e-6).contains(&t) && (p - pivot).dot(ray) > 0.0 {
                                    best = p;
                                }
                            }
                        }
                        best
                    };
                    for k in 0..3 {
                        let (p0, p1) = (boundary_at(k), boundary_at(k + 1));
                        let mut poly = vec![pivot, p0];
                        if k == 1 {
                            poly.push(corners[1]);
                        }
                        poly.push(p1);
                        poly.retain(|_| true);
                        let z = base_z + riser as f64 * r;
                        steps.push(Step { poly, z, index: riser, landing: false, nosing: Vec::new(), low_z: None });
                        if k < 2 {
                            riser += 1;
                        }
                    }
                    let center = end + dir * half;
                    walk.push(DVec3::new(center.x, center.y, base_z + riser as f64 * r));
                    let dir2 = perp(dir) * side;
                    let start2 = center + dir2 * half;
                    flight(start2, dir2, rest.saturating_sub(2), half, &mut riser, &mut steps, &mut walk);
                }
                StairShape::L { .. } => {
                    steps.push(Step {
                        poly: quad(end, dir, 0.0, w, half, half),
                        z: landing_z,
                        index: riser,
                        landing: true,
                        nosing: Vec::new(),
                        low_z: None,
                    });
                    let center = end + dir * half;
                    walk.push(DVec3::new(center.x, center.y, landing_z));
                    let dir2 = perp(dir) * side;
                    let start2 = center + dir2 * half;
                    flight(start2, dir2, rest, half, &mut riser, &mut steps, &mut walk);
                }
                StairShape::U { gap, .. } => {
                    let g = gap.mm().max(0.0);
                    let (left, right) = if side > 0.0 {
                        (half + g + w, half)
                    } else {
                        (half, half + g + w)
                    };
                    steps.push(Step {
                        poly: quad(end, dir, 0.0, w, left, right),
                        z: landing_z,
                        index: riser,
                        landing: true,
                        nosing: Vec::new(),
                        low_z: None,
                    });
                    let shift = perp(dir) * side * (w + g);
                    // Walk across the landing: up the first flight's axis, over, then back.
                    for p in [end + dir * half, end + dir * half + shift] {
                        walk.push(DVec3::new(p.x, p.y, landing_z));
                    }
                    flight(end + shift, -dir, rest, half, &mut riser, &mut steps, &mut walk);
                }
                StairShape::Straight | StairShape::Curved { .. } | StairShape::Sections => unreachable!(),
            }
            flights.push(rest);
        }
    }
    // Wrap-around: the bottom steps continue round both sides, each a
    // tread deeper than the one above (a pyramid of steps at the foot).
    let wrap = stair.extras.wrap.min(steps.iter().take_while(|s| !s.landing).count() as u32);
    if wrap > 0 {
        let mut extra = Vec::new();
        for (i, step) in steps.iter_mut().take(wrap as usize).enumerate() {
            let ext = (wrap as usize - i) as f64 * d;
            let inner = (wrap as usize - i - 1) as f64 * d;
            let (z, index) = (step.z, step.index);
            step.poly = quad(origin, dir, i as f64 * d, (i + 1) as f64 * d, half + ext, half + ext);
            step.nosing.clear();
            let (from, to) = ((i + 1) as f64 * d, wrap as f64 * d);
            if to - from > 1.0 {
                for (l, rr) in [(half + ext, -(half + inner)), (-(half + inner), half + ext)] {
                    extra.push(Step { poly: quad(origin, dir, from, to, l, rr), z, index, landing: false, nosing: Vec::new(), low_z: None });
                }
            }
        }
        steps.extend(extra);
    }
    // A starter step: the bottom tread wider, with round ends.
    if stair.extras.starter
        && let Some(first) = steps.first_mut()
        && !first.landing
        && first.poly.len() == 4
    {
        let (a, b, c, e) = (first.poly[0], first.poly[1], first.poly[2], first.poly[3]);
        let across = (e - a).normalize_or_zero();
        let along = (b - a).normalize_or_zero();
        let depth = a.distance(b);
        let grow = 150.0;
        let mut poly = Vec::new();
        // Right end: a half round beyond the stringer.
        let rc = (a + b) / 2.0 - across * grow;
        for k in 0..=6 {
            let ang = std::f64::consts::PI * k as f64 / 6.0;
            poly.push(rc + along * (-depth / 2.0 * ang.cos()) - across * (depth / 2.0 * ang.sin()));
        }
        let lc = (e + c) / 2.0 + across * grow;
        for k in 0..=6 {
            let ang = std::f64::consts::PI * k as f64 / 6.0;
            poly.push(lc + along * (depth / 2.0 * ang.cos()) + across * (depth / 2.0 * ang.sin()));
        }
        first.poly = poly;
        first.nosing.clear();
    }
    let mut g = StairGeom {
        risers,
        riser_height: r,
        base_z,
        steps,
        walk,
        rails,
        flights,
        stringers,
    };
    rail_ends(stair, &mut g.rails);
    g
}

/// Level extensions past each end of the rails, and returns turning down.
fn rail_ends(stair: &Stair, rails: &mut [Vec<DVec3>]) {
    let ext = stair.extras.rail_extension.mm();
    for rail in rails.iter_mut() {
        if rail.len() < 2 {
            continue;
        }
        if ext > 1.0 {
            let (a, b) = (rail[0], rail[1]);
            let back = (a - b).truncate().normalize_or_zero();
            rail.insert(0, (a.truncate() + back * ext).extend(a.z));
            let (y, z) = (rail[rail.len() - 2], rail[rail.len() - 1]);
            let on = (z - y).truncate().normalize_or_zero();
            rail.push((z.truncate() + on * ext).extend(z.z));
        }
        if stair.extras.rail_return {
            let first = rail[0];
            rail.insert(0, first - DVec3::Z * 120.0);
            let last = rail[rail.len() - 1];
            rail.push(last - DVec3::Z * 120.0);
        }
    }
}

/// A ramp: runs at 1:12 rising at most 30" each, with 5' landings between,
/// rails both sides.
fn ramp(stair: &Stair, rise: f64, base_z: f64, origin: DVec2, dir: DVec2, half: f64) -> StairGeom {
    let max_rise = 762.0;
    let landing = 1524.0;
    let runs = (rise / max_rise).ceil().max(1.0) as u32;
    let each = rise / runs as f64;
    let mut steps = Vec::new();
    let mut at = 0.0;
    let mut z = base_z;
    let mut walk = vec![origin.extend(base_z)];
    let mut rail_pts: Vec<(f64, f64)> = vec![(0.0, base_z)];
    for k in 0..runs {
        let run = each * 12.0;
        steps.push(Step {
            poly: quad(origin, dir, at, at + run, half, half),
            z: z + each,
            index: k * 2 + 1,
            landing: false,
            nosing: Vec::new(),
            low_z: Some(z),
        });
        at += run;
        z += each;
        rail_pts.push((at, z));
        walk.push((origin + dir * at).extend(z));
        if k + 1 < runs {
            steps.push(Step { poly: quad(origin, dir, at, at + landing, half, half), z, index: k * 2 + 2, landing: true, nosing: Vec::new(), low_z: None });
            at += landing;
            rail_pts.push((at, z));
            walk.push((origin + dir * at).extend(z));
        }
    }
    let rail_h = stair.railing_height.mm().max(864.0);
    let mut rails = Vec::new();
    for (enabled, sign) in [(stair.railing_left, 1.0), (stair.railing_right, -1.0)] {
        if enabled {
            let off = perp(dir) * (half - 50.0) * sign;
            rails.push(rail_pts.iter().map(|(u, zz)| (origin + dir * *u + off).extend(zz + rail_h)).collect());
        }
    }
    let mut g = StairGeom { risers: runs, riser_height: each, base_z, steps, walk, rails, flights: vec![runs], stringers: Vec::new() };
    rail_ends(stair, &mut g.rails);
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::{Document, Id};
    use housecraft_units::{Length, Point2};

    fn stair(shape: StairShape) -> (Document, Stair) {
        let doc = Document::new("t");
        let d = doc.active_defaults().clone();
        let s = Stair {
            id: Id(0),
            floor: doc.floors_by_elevation()[0].id,
            start: Point2::from_mm(0.0, 0.0),
            direction: 90.0,
            width: d.stair.width,
            shape,
            rise: Some(Length::from_inches(108.0)),
            max_riser: d.stair.max_riser,
            tread_depth: d.stair.tread_depth,
            railing_left: true,
            railing_right: true,
            railing_height: d.stair.railing_height,
            tread_overhang: d.stair.tread_overhang,
            stringers: true,
            stairwell: true,
            layer: d.layers.walls,
            baluster: None,
            newel: None,
            newel_style: Default::default(),
            newel_size: Length::from_inches(3.5),
            extras: Default::default(),
            sections: Vec::new(),
        };
        (doc, s)
    }

    #[test]
    fn straight_stair_counts_risers() {
        let (doc, s) = stair(StairShape::Straight);
        let g = geometry(&doc, &s);
        // 108" / 7.75" max → 14 risers of 7.714", 13 treads.
        assert_eq!(g.risers, 14);
        assert_eq!(g.steps.len(), 13);
        assert!((g.riser_height - 108.0 * 25.4 / 14.0).abs() < 1e-9);
        let top = g.steps.last().unwrap();
        assert!((top.z - 13.0 * g.riser_height).abs() < 1e-9);
        // Runs north 13 treads x 10".
        let max_y = top.poly.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        assert!((max_y - 130.0 * 25.4).abs() < 1e-6);
        assert_eq!(g.rails.len(), 2);
    }

    #[test]
    fn l_and_u_stairs_have_landings() {
        let (doc, s) = stair(StairShape::L {
            turn: Turn::Right,
            first_treads: 6,
        });
        let g = geometry(&doc, &s);
        assert_eq!(g.steps.iter().filter(|s| s.landing).count(), 1);
        assert_eq!(g.flights, vec![6, 6]);
        // The second flight heads east (a right turn from north).
        let last = g.steps.last().unwrap();
        assert!(last.poly.iter().all(|p| p.x > 0.0));
        let (doc, s) = stair(StairShape::U {
            turn: Turn::Left,
            first_treads: 6,
            gap: Length::ZERO,
        });
        let g = geometry(&doc, &s);
        let last = g.steps.last().unwrap();
        // Second flight runs back south beside the first, on the left (west).
        assert!(last.poly.iter().all(|p| p.x < 0.0));
        assert!(last.poly.iter().map(|p| p.y).fold(f64::MAX, f64::min) < 1.0);
    }

    #[test]
    fn curved_winders_ramps_and_rail_ends() {
        let (doc, s) = stair(StairShape::Curved { turn: Turn::Left, radius: Length::from_feet(3.0) });
        let g = geometry(&doc, &s);
        assert_eq!(g.steps.len(), 13);
        // Each wedge is wider outside than inside.
        let w = &g.steps[0].poly;
        assert!(w[1].distance(w[2]) > w[0].distance(w[3]));
        let (doc, mut s) = stair(StairShape::L { turn: Turn::Right, first_treads: 5 });
        s.extras.winders = true;
        let g = geometry(&doc, &s);
        assert_eq!(g.steps.iter().filter(|s| s.landing).count(), 0, "winders, not a landing");
        assert_eq!(g.steps.last().unwrap().index, g.risers - 1, "every riser still used");
        let (doc, mut s) = stair(StairShape::Straight);
        s.extras.ramp = true;
        s.extras.rail_extension = Length::from_inches(12.0);
        let g = geometry(&doc, &s);
        // 108" of rise: four runs of 27" with three landings between.
        assert_eq!(g.steps.iter().filter(|s| s.low_z.is_some()).count(), 4);
        assert_eq!(g.steps.iter().filter(|s| s.landing).count(), 3);
        assert!(g.rails.iter().all(|r| r.len() >= 9), "landings, runs and extensions");
    }
}
