//! Site features and plants in 3D: retaining walls, roads, fences, garden
//! beds, stepping stones, streams, trees and shrubs, all sitting on the
//! ground.

use glam::{DVec2, DVec3};
use housecraft_model::{Color, Document, FenceStyle, GateStyle, Plant, PlantForm, RoadStyle, SiteFeature, SiteKind, Terrain};

use crate::math::{perp, signed_area, v2};
use crate::mesh::{Category, Look, Mesh};

/// Ground height (mm) anywhere: the terrain's surface, or grade without one.
pub fn ground(doc: &Document) -> impl Fn(DVec2) -> f64 + '_ {
    let surface = doc
        .iter::<Terrain>()
        .next()
        .and_then(|t| crate::terrain::surface(doc, t));
    let grade = crate::sun::grade(doc).unwrap_or(0.0);
    move |p| surface.as_ref().map_or(grade, |s| s.height_at(p))
}

fn look(c: Color, shade: f32, rough: f32) -> Look {
    let [r, g, b, _] = c.0;
    let f = |v: u8| v as f32 / 255.0 * shade;
    Look {
        color: [f(r), f(g), f(b), 1.0],
        pbr: [rough, 0.0, 0.0, 1.0],
        surface: None,
        tex: None,
    }
}

/// Points along a path about `step` apart, with the distance to each.
pub fn along(points: &[DVec2], closed: bool, step: f64) -> Vec<(DVec2, f64)> {
    let n = points.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    let mut out = Vec::new();
    let mut dist = 0.0;
    for i in 0..count {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let len = a.distance(b);
        let k = ((len / step).ceil() as usize).max(1);
        for s in 0..k {
            let t = s as f64 / k as f64;
            out.push((a.lerp(b, t), dist + len * t));
        }
        dist += len;
    }
    if let Some(last) = points.get(if closed { 0 } else { n.saturating_sub(1) }) {
        out.push((*last, dist));
    }
    out
}

/// The path's left and right edges `half` either side at each sample.
fn edges(samples: &[(DVec2, f64)], half: f64) -> Vec<(DVec2, DVec2)> {
    (0..samples.len())
        .map(|i| {
            let a = samples[i.saturating_sub(1)].0;
            let b = samples[(i + 1).min(samples.len() - 1)].0;
            let n = perp((b - a).normalize_or_zero()) * half;
            (samples[i].0 + n, samples[i].0 - n)
        })
        .collect()
}

/// A strip `half` wide either side of the path, from `z(p) + lo` up to
/// `z(p) + hi`, following the ground.
fn strip(samples: &[(DVec2, f64)], half: f64, lo: f64, hi: f64, z: &dyn Fn(DVec2) -> f64, look: Look, mesh: &mut Mesh) {
    let e = edges(samples, half);
    for i in 0..e.len().saturating_sub(1) {
        let (l0, r0) = e[i];
        let (l1, r1) = e[i + 1];
        // Each edge follows the ground under it (two triangles, as the
        // four corners needn't be level).
        let (zl0, zl1, zr0, zr1) = (z(l0), z(l1), z(r0), z(r1));
        mesh.polygon_look(&[r0.extend(zr0 + hi), r1.extend(zr1 + hi), l1.extend(zl1 + hi)], look);
        mesh.polygon_look(&[r0.extend(zr0 + hi), l1.extend(zl1 + hi), l0.extend(zl0 + hi)], look);
        // Sides.
        mesh.polygon_look(&[l0.extend(zl0 + lo), l0.extend(zl0 + hi), l1.extend(zl1 + hi), l1.extend(zl1 + lo)], look);
        mesh.polygon_look(&[r1.extend(zr1 + lo), r1.extend(zr1 + hi), r0.extend(zr0 + hi), r0.extend(zr0 + lo)], look);
    }
}

pub fn post(c: DVec2, half: f64, z0: f64, z1: f64, look: Look, mesh: &mut Mesh) {
    let sq = [
        c + DVec2::new(-half, -half),
        c + DVec2::new(half, -half),
        c + DVec2::new(half, half),
        c + DVec2::new(-half, half),
    ];
    mesh.prism_look(&sq, z0, z1, look);
}

/// Where driveways and sidewalks meet road `f`'s curbs: points on its centre
/// line and how far either side of each the curb is cut away.
pub fn curb_cuts(doc: &Document, f: &SiteFeature) -> Vec<(DVec2, f64)> {
    let SiteKind::Road { width: road_width, curbs: true, .. } = &f.feature else {
        return Vec::new();
    };
    let centre: Vec<DVec2> = f.points.iter().map(|p| v2(*p)).collect();
    let reach = road_width.mm() / 2.0 + 2000.0;
    let mut cuts = Vec::new();
    for g in doc.iter::<SiteFeature>().filter(|g| g.id != f.id && g.floor == f.floor) {
        let SiteKind::Road { style, width, flare, .. } = &g.feature else { continue };
        if *style == RoadStyle::Road {
            continue;
        }
        for end in [g.points.first(), g.points.last()].into_iter().flatten() {
            let p = v2(*end);
            if let Some(q) = centre
                .windows(2)
                .map(|w| crate::math::closest_on_segment(p, w[0], w[1]).0)
                .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
                .filter(|q| q.distance(p) < reach)
            {
                cuts.push((q, width.mm() / 2.0 + flare.mm()));
            }
        }
    }
    cuts
}

/// Runs of `samples` that stay clear of every cut.
pub fn uncut_runs(samples: &[(DVec2, f64)], centre: &[(DVec2, f64)], cuts: &[(DVec2, f64)]) -> Vec<Vec<(DVec2, f64)>> {
    let mut runs = vec![Vec::new()];
    for (k, s) in samples.iter().enumerate() {
        let c = centre.get(k).map_or(s.0, |c| c.0);
        if cuts.iter().any(|(q, half)| q.distance(c) < *half) {
            if runs.last().is_some_and(|r| !r.is_empty()) {
                runs.push(Vec::new());
            }
        } else if let Some(r) = runs.last_mut() {
            r.push(*s);
        }
    }
    runs.retain(|r| r.len() >= 2);
    runs
}

pub fn feature_mesh(f: &SiteFeature, z: &dyn Fn(DVec2) -> f64, floor_z: f64, cuts: &[(DVec2, f64)], mesh: &mut Mesh) {
    let mut pts: Vec<DVec2> = f.points.iter().map(|p| v2(*p)).collect();
    if pts.len() < 2 {
        return;
    }
    let painted = mesh.painted_whole();
    let pick = |l: Look| painted.unwrap_or(l);
    let concrete = pick(look(Color::rgb(185, 182, 175), 1.0, 0.85));
    let wood = pick(look(Color::rgb(232, 228, 215), 1.0, 0.7));
    match &f.feature {
        SiteKind::RetainingWall { top, thickness } => {
            let s = along(&pts, false, 300.0);
            let half = thickness.mm() / 2.0;
            let e = edges(&s, half);
            for i in 0..e.len() - 1 {
                let (l0, r0) = e[i];
                let (l1, r1) = e[i + 1];
                let bottom = z(s[i].0).min(z(s[i + 1].0)) - 300.0;
                let quad = vec![r0, r1, l1, l0];
                let quad = if signed_area(&quad) < 0.0 { quad.into_iter().rev().collect() } else { quad };
                mesh.prism_look(&quad, bottom, top.mm().max(bottom + 10.0), concrete);
            }
        }
        SiteKind::Road { style, width, thickness, curbs, median, cul_de_sac, stripes, flare, .. } => {
            let s = along(&pts, false, 500.0);
            let surface = pick(match style {
                housecraft_model::RoadStyle::Road => look(Color::rgb(70, 70, 72), 1.0, 0.9),
                _ => look(Color::rgb(200, 198, 192), 1.0, 0.85),
            });
            let t = thickness.mm().max(20.0);
            strip(&s, width.mm() / 2.0, -t, 30.0, z, surface, mesh);
            // A raised median down the middle.
            if median.mm() > 10.0 {
                strip(&s, median.mm() / 2.0, 30.0, 180.0, z, concrete, mesh);
            }
            // A round turnaround at the far end.
            let r = cul_de_sac.mm();
            if r > width.mm() / 2.0
                && let Some(end) = pts.last().copied()
            {
                let disc: Vec<DVec2> = (0..32).map(|i| end + DVec2::from_angle(i as f64 / 32.0 * std::f64::consts::TAU) * r).collect();
                let zc = z(end);
                mesh.prism_look(&disc, zc - t, zc + 30.0, surface);
            }
            // A flared end: a wedge either side, widest at the end.
            if let Some(apron) = road_flare(&pts, width.mm(), flare.mm()) {
                for wedge in apron {
                    let zc = z(wedge[0]);
                    let wedge = if signed_area(&wedge) < 0.0 { wedge.into_iter().rev().collect() } else { wedge };
                    mesh.prism_look(&wedge, zc - t, zc + 30.0, surface);
                }
            }
            // Painted stripes just above the surface.
            let paint = look(Color::rgb(245, 240, 220), 1.0, 0.6);
            for off in stripes {
                let line: Vec<(DVec2, f64)> = edges(&s, off.mm().abs())
                    .into_iter()
                    .zip(&s)
                    .map(|((l, r), (_, d))| (if off.mm() >= 0.0 { l } else { r }, *d))
                    .collect();
                strip(&line, 50.0, 25.0, 35.0, z, paint, mesh);
            }
            if *curbs {
                let half = width.mm() / 2.0;
                for side in [1.0, -1.0] {
                    let offset: Vec<(DVec2, f64)> = edges(&s, half + 75.0)
                        .into_iter()
                        .zip(&s)
                        .map(|((l, r), (_, d))| (if side > 0.0 { l } else { r }, *d))
                        .collect();
                    for run in uncut_runs(&offset, &s, cuts) {
                        strip(&run, 75.0, -t, 150.0, z, concrete, mesh);
                    }
                }
            }
        }
        SiteKind::Curb { height, width } => {
            let s = along(&pts, false, 300.0);
            strip(&s, width.mm() / 2.0, -150.0, height.mm(), z, concrete, mesh);
        }
        SiteKind::Fence { style, height, post_spacing, gates, post_size, rail_size } => {
            let (post_half, rail_half) = (post_size.mm() / 2.0, rail_size.mm() / 2.0);
            let h = height.mm();
            let s = along(&pts, false, 100.0);
            let in_gate = |d: f64| gates.iter().any(|g| d > g.at.mm() && d < g.at.mm() + g.width.mm());
            let total = s.last().map_or(0.0, |x| x.1);
            // Posts at the spacing and at every gate side.
            let mut post_at: Vec<f64> = Vec::new();
            let step = post_spacing.mm().max(600.0);
            let mut d = 0.0;
            while d < total {
                post_at.push(d);
                d += step;
            }
            post_at.push(total);
            for g in gates {
                post_at.extend([g.at.mm(), g.at.mm() + g.width.mm()]);
            }
            let point_at = |d: f64| {
                let i = s.iter().position(|(_, x)| *x >= d).unwrap_or(s.len() - 1);
                s[i].0
            };
            for d in &post_at {
                let p = point_at(*d);
                post(p, post_half, z(p) - 300.0, z(p) + h + 50.0, wood, mesh);
            }
            match style {
                FenceStyle::Picket => {
                    // A picket at every sample (100 mm apart).
                    for (p, dist) in &s {
                        if in_gate(*dist) {
                            continue;
                        }
                        post(*p, 20.0, z(*p) + 75.0, z(*p) + h, wood, mesh);
                    }
                }
                FenceStyle::Privacy => {
                    for w in s.windows(2) {
                        if in_gate((w[0].1 + w[1].1) / 2.0) {
                            continue;
                        }
                        strip(w, 12.0, 50.0, h, z, wood, mesh);
                    }
                }
                FenceStyle::Rail => {}
                FenceStyle::IronPole => {
                    // Thin dark poles about 4" apart.
                    let iron = look(Color::rgb(40, 40, 42), 1.0, 0.5);
                    for (p, dist) in &s {
                        if !in_gate(*dist) {
                            post(*p, 8.0, z(*p) + 75.0, z(*p) + h - 25.0, iron, mesh);
                        }
                    }
                }
            }
            // Rails.
            for frac in match style {
                FenceStyle::Rail => vec![0.35, 0.8],
                _ => vec![0.2, 0.85],
            } {
                for w in s.windows(2) {
                    if in_gate((w[0].1 + w[1].1) / 2.0) {
                        continue;
                    }
                    strip(w, 20.0, h * frac - rail_half, h * frac + rail_half, z, wood, mesh);
                }
            }
            // Gates: a darker panel, pickets or an arched panel in each opening.
            let gate_look = look(Color::rgb(150, 130, 100), 1.0, 0.7);
            for g in gates {
                let gh = g.height.map_or(h, |l| l.mm()).max(200.0);
                let lo = g.clearance.mm().clamp(0.0, gh - 100.0);
                let (g0, g1) = (g.at.mm(), g.at.mm() + g.width.mm());
                let span: Vec<(DVec2, f64)> = s.iter().copied().filter(|(_, d)| *d >= g0 && *d <= g1).collect();
                if span.len() < 2 {
                    continue;
                }
                match g.style {
                    GateStyle::Panel => strip(&span, 18.0, lo, gh - 25.0, z, gate_look, mesh),
                    GateStyle::Picket => {
                        for frac in [0.2, 0.8] {
                            strip(&span, 18.0, gh * frac - 40.0, gh * frac + 40.0, z, gate_look, mesh);
                        }
                        for (p, _) in &span {
                            post(*p, 18.0, z(*p) + lo, z(*p) + gh - 25.0, gate_look, mesh);
                        }
                    }
                    GateStyle::Arched => {
                        // Rises a sixth of the width at the middle.
                        let rise = (g1 - g0) / 6.0;
                        for w in span.windows(2) {
                            let mid = ((w[0].1 + w[1].1) / 2.0 - g0) / (g1 - g0).max(1.0);
                            let top = gh - 25.0 + rise * (std::f64::consts::PI * mid).sin();
                            strip(w, 18.0, lo, top, z, gate_look, mesh);
                        }
                    }
                }
            }
        }
        SiteKind::GardenBed { height } => {
            if signed_area(&pts) < 0.0 {
                pts.reverse();
            }
            let soil = pick(mesh.look(housecraft_model::Surface::Dirt));
            if height.mm() < 1.0 {
                // A flat bed (mulch) lying on the ground.
                let mut edge: Vec<DVec2> = along(&pts, true, 600.0).into_iter().map(|(p, _)| p).collect();
                edge.pop();
                let on = |p: DVec2| DVec3::new(p.x, p.y, z(p) + 25.0);
                for [a, b, c] in crate::triangulate::triangulate(&edge) {
                    mesh.polygon_look(&[on(edge[a]), on(edge[b]), on(edge[c])], soil);
                }
            } else {
                let base = pts.iter().map(|p| z(*p)).fold(f64::MAX, f64::min);
                mesh.prism_look(&pts, base - 50.0, base + height.mm(), wood);
                let inner = crate::math::inset_polygon(&pts, 50.0);
                if inner.len() >= 3 {
                    mesh.prism_look(&inner, base + height.mm() - 10.0, base + height.mm() + 5.0, soil);
                }
            }
        }
        SiteKind::SteppingStones { spacing, size } => {
            let stone = pick(look(Color::rgb(150, 145, 135), 1.0, 0.9));
            for (p, _) in along(&pts, false, spacing.mm().max(100.0)) {
                let r = size.mm() / 2.0;
                let disc: Vec<DVec2> = (0..10)
                    .map(|i| p + DVec2::from_angle(i as f64 / 10.0 * std::f64::consts::TAU) * r * (0.9 + 0.1 * ((i * 7 % 3) as f64)))
                    .collect();
                mesh.prism_look(&disc, z(p) - 20.0, z(p) + 40.0, stone);
            }
        }
        SiteKind::RopeLight { height, lumens_per_foot, kelvin } => {
            let glow = Look { color: [1.0, 0.9, 0.7, 1.0], pbr: [0.5, 0.0, 1.0, 1.0], surface: Some(housecraft_model::Surface::Lamp), tex: None };
            let zr = floor_z + height.mm();
            let s = along(&pts, false, 300.0);
            strip(&s, 8.0, 0.0, 16.0, &|_| zr, glow, mesh);
            // A light every 2 m (the view uses the nearest few).
            let settings = housecraft_model::LightSettings { on: true, lumens: 0.0, dimmer: 1.0, kelvin: *kelvin, shadows: false };
            let [r, g, b] = settings.rgb();
            let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
            let n = ((total / 2000.0).ceil() as usize).max(1);
            let per = lumens_per_foot * total / 304.8 / n as f64;
            for (p, _) in along(&pts, false, total / n as f64).into_iter().take(n) {
                mesh.lights.push(crate::mesh::MeshLight {
                    object: f.id,
                    position: [(p.x / 1000.0) as f32, (p.y / 1000.0) as f32, (zr / 1000.0) as f32],
                    color: [r as f32, g as f32, b as f32],
                    intensity: (per / 800.0) as f32,
                    shadows: false,
                    spread: 0,
                });
            }
        }
        SiteKind::Grass { length, density, color } => {
            if signed_area(&pts) < 0.0 {
                pts.reverse();
            }
            let blade = look(*color, 1.0, 0.95);
            let tip = look(*color, 1.25, 0.95);
            let (lo, hi) = crate::math::bounds(pts.iter().copied()).expect("points");
            let area_m2 = signed_area(&pts).abs() / 1e6;
            // Keep the count sane on big lawns.
            let count = (area_m2 * density).min(60_000.0) as usize;
            let h = length.mm();
            let mut k = 0u64;
            let mut placed = 0;
            while placed < count && k < count as u64 * 4 {
                k += 1;
                let fx = ((k as f64 * 0.754_877_666).fract(), (k as f64 * 0.569_840_29).fract());
                let p = DVec2::new(lo.x + fx.0 * (hi.x - lo.x), lo.y + fx.1 * (hi.y - lo.y));
                if !crate::math::point_in_polygon(p, &pts) {
                    continue;
                }
                placed += 1;
                let a = (k as f64 * 2.399_963).rem_euclid(std::f64::consts::TAU);
                let side = DVec2::from_angle(a) * 6.0;
                let lean = DVec2::from_angle(a + 1.3) * h * 0.25;
                let base = z(p);
                let tall = h * (0.7 + 0.3 * ((k as f64 * 0.31).fract()));
                let (b0, b1, t) = ((p - side).extend(base), (p + side).extend(base), (p + lean).extend(base + tall));
                mesh.polygon_look(&[b0, b1, t], if k.is_multiple_of(3) { tip } else { blade });
                mesh.polygon_look(&[b1, b0, t], blade);
            }
        }
        SiteKind::Stream { width } => {
            let water = Look {
                color: [0.25, 0.45, 0.6, 1.0],
                pbr: [0.08, 0.0, 0.0, 1.0],
                surface: None,
                tex: None,
            };
            let s = along(&crate::math::spline_points(&pts, false), false, 400.0);
            strip(&s, width.mm() / 2.0, -50.0, 15.0, z, water, mesh);
        }
    }
}

/// A lathe: `profile` (radius, height) turned about a vertical axis at `c`.
fn lathe(c: DVec2, z0: f64, profile: &[(f64, f64)], sides: usize, look: Look, mesh: &mut Mesh) {
    let at = |r: f64, h: f64, i: usize| {
        let d = DVec2::from_angle(i as f64 / sides as f64 * std::f64::consts::TAU) * r;
        DVec3::new(c.x + d.x, c.y + d.y, z0 + h)
    };
    for k in 0..profile.len() - 1 {
        let ((r0, h0), (r1, h1)) = (profile[k], profile[k + 1]);
        for i in 0..sides {
            let mut quad = vec![at(r0, h0, i), at(r0, h0, i + 1), at(r1, h1, i + 1), at(r1, h1, i)];
            quad.dedup_by(|a, b| a.distance(*b) < 0.01);
            if quad.len() >= 3 {
                mesh.polygon_look(&quad, look);
            }
        }
    }
}

pub fn plant_mesh(p: &Plant, z: &dyn Fn(DVec2) -> f64, mesh: &mut Mesh) {
    let c = v2(p.at);
    let base = z(c);
    let (h, w) = p.size_now();
    let (h, r) = (h.mm().max(100.0), w.mm().max(100.0) / 2.0);
    let leaves = mesh.painted_whole().unwrap_or(look(p.color, 1.0, 0.9));
    let bark = look(Color::rgb(95, 70, 50), 1.0, 0.95);
    match p.form {
        PlantForm::Tree => {
            let trunk = (h * 0.025).clamp(40.0, 250.0);
            lathe(c, base, &[(trunk, 0.0), (trunk * 0.8, h * 0.4)], 8, bark, mesh);
            // A rounded crown from 35% to the top.
            let (cz, cr) = (h * 0.35 + (h * 0.65) / 2.0, h * 0.65 / 2.0);
            let profile: Vec<(f64, f64)> = (0..=8)
                .map(|i| {
                    let a = -std::f64::consts::FRAC_PI_2 + i as f64 / 8.0 * std::f64::consts::PI;
                    (r * a.cos(), cz + cr * a.sin())
                })
                .collect();
            lathe(c, base, &profile, 12, leaves, mesh);
        }
        PlantForm::Conifer => {
            let trunk = (h * 0.02).clamp(40.0, 200.0);
            lathe(c, base, &[(trunk, 0.0), (trunk, h * 0.15)], 8, bark, mesh);
            lathe(c, base, &[(0.0, h * 0.12), (r, h * 0.15), (0.0, h)], 12, leaves, mesh);
        }
        PlantForm::Shrub => {
            let profile: Vec<(f64, f64)> = (0..=6)
                .map(|i| {
                    let a = i as f64 / 6.0 * std::f64::consts::FRAC_PI_2;
                    (r * a.cos(), h * a.sin())
                })
                .chain([(0.0, h)])
                .collect();
            let mut full = vec![(0.0, 0.0)];
            full.extend(profile);
            lathe(c, base, &full, 10, leaves, mesh);
        }
        PlantForm::Grass => {
            for i in 0..7 {
                let d = DVec2::from_angle(i as f64 / 7.0 * std::f64::consts::TAU) * r * 0.6;
                let tip = (c + d).extend(base + h);
                let b0 = (c + d * 0.1 + perp(d.normalize_or_zero()) * 30.0).extend(base);
                let b1 = (c + d * 0.1 - perp(d.normalize_or_zero()) * 30.0).extend(base);
                mesh.polygon_look(&[b0, b1, tip], leaves);
                mesh.polygon_look(&[b1, b0, tip], leaves);
            }
        }
    }
}

/// Every site feature and plant (on `only_floor` when given).
pub fn build(doc: &Document, only_floor: Option<housecraft_model::Id>, mesh: &mut Mesh) {
    let has = doc.count::<SiteFeature>() + doc.count::<Plant>() > 0;
    if !has {
        return;
    }
    let z = ground(doc);
    for f in doc.iter::<SiteFeature>() {
        if only_floor.is_some_and(|o| o != f.floor) {
            continue;
        }
        mesh.begin(f.id, Category::Terrain);
        let floor_z = doc.get::<housecraft_model::Floor>(f.floor).map_or(0.0, |fl| fl.elevation.mm());
        feature_mesh(f, &z, floor_z, &curb_cuts(doc, f), mesh);
        mesh.end();
    }
    for p in doc.iter::<Plant>() {
        if only_floor.is_some_and(|o| o != p.floor) {
            continue;
        }
        mesh.begin(p.id, Category::Terrain);
        plant_mesh(p, &z, mesh);
        mesh.end();
    }
}

/// The two wedges of a flared road end (each a triangle beside the road's
/// last `flare` of length, widest at the end), or `None` for a square end.
pub fn road_flare(pts: &[DVec2], width: f64, flare: f64) -> Option<[Vec<DVec2>; 2]> {
    let n = pts.len();
    if flare < 1.0 || n < 2 {
        return None;
    }
    let (a, end) = (pts[n - 2], pts[n - 1]);
    let dir = (end - a).normalize_or_zero();
    let run = flare.min(a.distance(end));
    let side = crate::math::perp(dir);
    let half = width / 2.0;
    let wedge = |k: f64| vec![end - dir * run + side * half * k, end + side * half * k, end + side * (half + flare) * k];
    Some([wedge(1.0), wedge(-1.0)])
}
