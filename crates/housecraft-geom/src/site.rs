//! Site features and plants in 3D: retaining walls, roads, fences, garden
//! beds, stepping stones, streams, trees and shrubs, all sitting on the
//! ground.

use glam::{DVec2, DVec3};
use housecraft_model::{Color, Document, FenceStyle, Plant, PlantForm, SiteFeature, SiteKind, Terrain};

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

pub fn feature_mesh(f: &SiteFeature, z: &dyn Fn(DVec2) -> f64, mesh: &mut Mesh) {
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
        SiteKind::Road { style, width, thickness, curbs } => {
            let s = along(&pts, false, 500.0);
            let surface = pick(match style {
                housecraft_model::RoadStyle::Road => look(Color::rgb(70, 70, 72), 1.0, 0.9),
                _ => look(Color::rgb(200, 198, 192), 1.0, 0.85),
            });
            let t = thickness.mm().max(20.0);
            strip(&s, width.mm() / 2.0, -t, 30.0, z, surface, mesh);
            if *curbs {
                let half = width.mm() / 2.0;
                for side in [1.0, -1.0] {
                    let offset: Vec<(DVec2, f64)> = edges(&s, half + 75.0)
                        .into_iter()
                        .zip(&s)
                        .map(|((l, r), (_, d))| (if side > 0.0 { l } else { r }, *d))
                        .collect();
                    strip(&offset, 75.0, -t, 150.0, z, concrete, mesh);
                }
            }
        }
        SiteKind::Fence { style, height, post_spacing, gates } => {
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
                post(p, 45.0, z(p) - 300.0, z(p) + h + 50.0, wood, mesh);
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
                    strip(w, 20.0, h * frac - 45.0, h * frac + 45.0, z, wood, mesh);
                }
            }
            // Gates: a darker panel in each opening.
            let gate_look = look(Color::rgb(150, 130, 100), 1.0, 0.7);
            for g in gates {
                let span: Vec<(DVec2, f64)> = s
                    .iter()
                    .copied()
                    .filter(|(_, d)| *d >= g.at.mm() && *d <= g.at.mm() + g.width.mm())
                    .collect();
                if span.len() >= 2 {
                    strip(&span, 18.0, 75.0, h - 25.0, z, gate_look, mesh);
                }
            }
        }
        SiteKind::GardenBed { height } => {
            if signed_area(&pts) < 0.0 {
                pts.reverse();
            }
            let base = pts.iter().map(|p| z(*p)).fold(f64::MAX, f64::min);
            mesh.prism_look(&pts, base - 50.0, base + height.mm(), wood);
            let soil = look(Color::rgb(90, 65, 45), 1.0, 0.95);
            let inner = crate::math::inset_polygon(&pts, 50.0);
            if inner.len() >= 3 {
                mesh.prism_look(&inner, base + height.mm() - 10.0, base + height.mm() + 5.0, soil);
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
        feature_mesh(f, &z, mesh);
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
