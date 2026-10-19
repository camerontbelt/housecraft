//! Roof trim and accessories: ridge caps on ridges and hips, gutters along
//! the eaves, a frieze board at the top of exterior walls, and skylights.

use glam::{DVec2, DVec3};
use housecraft_model::{Document, Id, RoofPlane, Surface};

use crate::mesh::Mesh;
use crate::roof::newell;

fn outline3(p: &RoofPlane) -> Vec<DVec3> {
    p.outline
        .iter()
        .map(|q| {
            let [x, y, z] = q.mm();
            DVec3::new(x, y, z)
        })
        .collect()
}

/// Height of the top of the roof at plan point `p`, over every sloped plane
/// on `floor` (the highest), if any covers it.
pub fn roof_top_at(doc: &Document, floor: Option<Id>, p: DVec2) -> Option<f64> {
    doc.iter::<RoofPlane>()
        .filter(|r| !r.gable && floor.is_none_or(|f| r.floor == f))
        .filter_map(|r| plane_z(&outline3(r), p))
        .reduce(f64::max)
}

/// Height of a planar outline's surface at plan point `p`, if `p` is over it.
pub fn plane_z(outline: &[DVec3], p: DVec2) -> Option<f64> {
    let flat: Vec<DVec2> = outline.iter().map(|q| q.truncate()).collect();
    if !crate::math::point_in_polygon(p, &flat) {
        return None;
    }
    plane_z_any(outline, p)
}

/// The height of the plane through `outline` at `p` (inside it or not).
pub fn plane_z_any(outline: &[DVec3], p: DVec2) -> Option<f64> {
    let n = newell(outline).normalize_or_zero();
    if n.z.abs() < 1e-6 {
        return None;
    }
    let o = outline[0];
    Some(o.z - (n.x * (p.x - o.x) + n.y * (p.y - o.y)) / n.z)
}

/// The horizontal direction straight down a plane's slope.
pub fn downslope(outline: &[DVec3]) -> DVec2 {
    let mut n = newell(outline);
    if n.z < 0.0 {
        n = -n;
    }
    n.truncate().normalize_or(DVec2::Y)
}

/// A prism along `a`→`b` with the cross-section `profile` (across, up),
/// across measured to the right of the edge seen from above.
fn sweep(mesh: &mut Mesh, a: DVec3, b: DVec3, profile: &[(f64, f64)], surface: Surface) {
    let d = (b - a).truncate().normalize_or_zero();
    if d == DVec2::ZERO {
        return;
    }
    let right = DVec2::new(d.y, -d.x).extend(0.0);
    let at = |base: DVec3, (x, z): (f64, f64)| base + right * x + DVec3::Z * z;
    let n = profile.len();
    for i in 0..n {
        let j = (i + 1) % n;
        mesh.polygon(&[at(a, profile[i]), at(a, profile[j]), at(b, profile[j]), at(b, profile[i])], surface);
        mesh.polygon(&[at(a, profile[i]), at(b, profile[i]), at(b, profile[j]), at(a, profile[j])], surface);
    }
    let start: Vec<DVec3> = profile.iter().map(|p| at(a, *p)).collect();
    let end: Vec<DVec3> = profile.iter().rev().map(|p| at(b, *p)).collect();
    mesh.polygon(&start, surface);
    mesh.polygon(&end, surface);
}

/// Ridge caps on every convex edge two roof planes share (ridges and hips).
pub fn ridge_caps(doc: &Document, only_floor: Option<Id>, mesh: &mut Mesh) {
    let planes: Vec<(Id, Vec<DVec3>)> = doc
        .iter::<RoofPlane>()
        .filter(|r| !r.gable && only_floor.is_none_or(|f| r.floor == f))
        .map(|r| (r.id, outline3(r)))
        .collect();
    let key = |p: DVec3| ((p.x / 5.0).round() as i64, (p.y / 5.0).round() as i64, (p.z / 5.0).round() as i64);
    type Key = (i64, i64, i64);
    let mut done: Vec<(Key, Key)> = Vec::new();
    for (i, (_, a)) in planes.iter().enumerate() {
        for (_, b) in planes.iter().skip(i + 1) {
            for k in 0..a.len() {
                let (p, q) = (a[k], a[(k + 1) % a.len()]);
                if p.distance(q) < 100.0 {
                    continue;
                }
                let shared = (0..b.len()).any(|m| {
                    let (r, s) = (b[m], b[(m + 1) % b.len()]);
                    (r.distance(p) < 5.0 && s.distance(q) < 5.0) || (r.distance(q) < 5.0 && s.distance(p) < 5.0)
                });
                if !shared {
                    continue;
                }
                // Convex (a ridge or hip): both planes fall away from the edge.
                let mid = (p + q) / 2.0;
                let falls = |pts: &[DVec3]| {
                    let c = pts.iter().fold(DVec3::ZERO, |s, v| s + *v) / pts.len() as f64;
                    let towards = (c - mid).truncate().normalize_or_zero() * 300.0;
                    plane_z_any(pts, mid.truncate() + towards).is_some_and(|z| z < mid.z - 1.0)
                };
                if !(falls(a) && falls(b)) {
                    continue;
                }
                let e = if key(p) < key(q) { (key(p), key(q)) } else { (key(q), key(p)) };
                if done.contains(&e) {
                    continue;
                }
                done.push(e);
                let profile = [(-110.0, -20.0), (0.0, 45.0), (110.0, -20.0)];
                sweep(mesh, p, q, &profile, Surface::Roof);
            }
        }
    }
}

/// Gutters along each plane's low, level edges.
pub fn gutters(doc: &Document, only_floor: Option<Id>, fascia_depth: f64, mesh: &mut Mesh) {
    for r in doc.iter::<RoofPlane>().filter(|r| !r.gable && only_floor.is_none_or(|f| r.floor == f)) {
        let top = outline3(r);
        let low = top.iter().map(|p| p.z).fold(f64::MAX, f64::min);
        let centre = top.iter().fold(DVec2::ZERO, |s, p| s + p.truncate()) / top.len() as f64;
        for i in 0..top.len() {
            let (a, b) = (top[i], top[(i + 1) % top.len()]);
            if (a.z - low).abs() > 1.0 || (b.z - low).abs() > 1.0 || a.distance(b) < 200.0 {
                continue;
            }
            // Outward from the plane, past the fascia board.
            let d = (b - a).truncate().normalize();
            let mut out = DVec2::new(-d.y, d.x);
            if out.dot((a.truncate() + b.truncate()) / 2.0 - centre) < 0.0 {
                out = -out;
            }
            let shift = (out * 30.0).extend(0.0);
            // A K-style gutter in section: back, bottom, front lip.
            let z = -fascia_depth.max(60.0) * 0.4;
            let right_is_out = DVec2::new(d.y, -d.x).dot(out) > 0.0;
            let s = if right_is_out { 1.0 } else { -1.0 };
            let profile = [(0.0, z + 20.0), (0.0, z - 110.0), (s * 125.0, z - 110.0), (s * 140.0, z - 60.0), (s * 135.0, z + 20.0), (s * 120.0, z + 20.0), (s * 122.0, z - 55.0), (s * 112.0, z - 95.0), (s * 12.0, z - 95.0), (s * 12.0, z + 20.0)];
            sweep(mesh, a + shift, b + shift, &profile, Surface::Trim);
        }
    }
}

/// A frieze board at the top of every exterior wall face.
pub fn frieze(doc: &Document, floor: &housecraft_model::Floor, height: f64, mesh: &mut Mesh) {
    let regions = crate::rooms::detect(doc, floor.id);
    let z0 = floor.elevation.mm();
    for g in crate::walls::wall_geoms(doc, floor.id) {
        let Some(w) = doc.get::<housecraft_model::Wall>(g.id) else {
            continue;
        };
        let top = z0 + doc.wall_height(w).map_or(2438.4, |h| h.mm());
        // The exterior face is whichever side lies outside every footprint.
        let probe = |side: f64| g.center_at(g.length / 2.0) + g.left * side * (g.half + 50.0);
        let outside = |p: DVec2| !regions.footprints.iter().any(|f| crate::math::point_in_polygon(p, f));
        let side = if outside(probe(1.0)) {
            1.0
        } else if outside(probe(-1.0)) {
            -1.0
        } else {
            continue;
        };
        let (a, b) = (g.center_at(0.0) + g.left * side * g.half, g.center_at(g.length) + g.left * side * g.half);
        let out = g.left * side;
        let quad = vec![a, b, b + out * 20.0, a + out * 20.0];
        mesh.begin(g.id, crate::mesh::Category::Wall);
        mesh.prism(&crate::walls::ccw_pub(quad), top - height, top, Surface::Trim);
        mesh.end();
    }
}

/// Curb-mounted skylights on a roof plane.
pub fn skylights(plane: &RoofPlane, mesh: &mut Mesh) {
    let top = outline3(plane);
    if top.len() < 3 {
        return;
    }
    let down = downslope(&top);
    let across = DVec2::new(-down.y, down.x);
    for s in &plane.skylights {
        let c = crate::math::v2(s.center);
        let (hl, hw) = (s.length.mm() / 2.0, s.width.mm() / 2.0);
        let corners = [c - down * hl - across * hw, c - down * hl + across * hw, c + down * hl + across * hw, c + down * hl - across * hw];
        let Some(zs) = corners.iter().map(|p| plane_z_any(&top, *p)).collect::<Option<Vec<f64>>>() else {
            continue;
        };
        let curb = 110.0;
        let at = |i: usize, dz: f64| corners[i].extend(zs[i] + dz);
        for i in 0..4 {
            let j = (i + 1) % 4;
            mesh.polygon(&[at(i, 0.0), at(j, 0.0), at(j, curb), at(i, curb)], Surface::Frame);
            mesh.polygon(&[at(i, 0.0), at(i, curb), at(j, curb), at(j, 0.0)], Surface::Frame);
        }
        let glass: Vec<DVec3> = (0..4).map(|i| at(i, curb)).collect();
        let mut up = glass.clone();
        if newell(&up).z < 0.0 {
            up.reverse();
        }
        mesh.polygon(&up, Surface::Glass);
    }
}
