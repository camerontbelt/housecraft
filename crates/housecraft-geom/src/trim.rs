//! Trim geometry: room moldings, molding polylines, siding cladding and
//! corner trim.

use glam::{DVec2, DVec3};
use housecraft_model::{
    Cladding, CornerTrim, Document, Floor, Id, MoldingPolyline, MoldingProfile, MoldingSpec,
    ProfileRef, RoomMoldingPlace, Surface, Wall, WallType,
};

use crate::math::{signed_area, v2};
use crate::mesh::Mesh;
use crate::sweep::{oriented, profile, sweep};
use crate::walls::WallGeom;

const IN: f64 = 25.4;

/// The (out, up) cross-section of a molding spec, in mm.
pub fn spec_profile(doc: &Document, spec: &MoldingSpec) -> Vec<DVec2> {
    let (h, d) = (spec.height.mm().max(1.0), spec.depth.mm().max(1.0));
    let mut pts: Vec<DVec2> = match spec.profile {
        ProfileRef::Shape { shape } => profile(shape, h, d),
        ProfileRef::Custom { profile: id } => match doc.get::<MoldingProfile>(id) {
            Some(p) if p.points.len() >= 3 => {
                // Scale the drawn profile to the spec's size.
                let raw: Vec<DVec2> = p.points.iter().map(|q| DVec2::new(q[0], q[1])).collect();
                let lo = raw.iter().copied().reduce(DVec2::min).expect("points");
                let hi = raw.iter().copied().reduce(DVec2::max).expect("points");
                let size = (hi - lo).max(DVec2::splat(1e-6));
                raw.iter()
                    .map(|q| (*q - lo) / size * DVec2::new(d, h))
                    .collect()
            }
            _ => profile(housecraft_model::MoldingShape::Flat, h, d),
        },
    };
    if spec.rotate {
        // Turn 90°: what stood up now lies along the surface.
        let (w, hh) = (d, h);
        pts = pts.iter().map(|q| DVec2::new(q.y / hh * w, (w - q.x) / w * hh)).collect();
    }
    if spec.flip {
        pts = pts.iter().map(|q| DVec2::new(q.x, h - q.y)).collect();
    }
    if spec.center {
        pts = pts.iter().map(|q| DVec2::new(q.x - d / 2.0, q.y)).collect();
    }
    if signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    pts
}

/// Split a closed outline (traversed so its inside is on the right) into
/// open runs, leaving out the parts across door openings.
fn runs_around(outline: &[DVec2], doors: &[(DVec2, DVec2)]) -> Vec<Vec<DVec2>> {
    let n = outline.len();
    // For each edge, the intervals (0..1) taken by doors.
    let mut pieces: Vec<Vec<DVec2>> = Vec::new();
    let mut current: Vec<DVec2> = Vec::new();
    for i in 0..n {
        let (a, b) = (outline[i], outline[(i + 1) % n]);
        let len = a.distance(b);
        if len < 1.0 {
            continue;
        }
        let dir = (b - a) / len;
        let mut gaps: Vec<(f64, f64)> = doors
            .iter()
            .filter_map(|(p, q)| {
                let off = |x: DVec2| (x - a).perp_dot(dir).abs();
                if off(*p) > 150.0 || off(*q) > 150.0 {
                    return None;
                }
                let (t0, t1) = ((*p - a).dot(dir), (*q - a).dot(dir));
                let (t0, t1) = (t0.min(t1).max(0.0), t0.max(t1).min(len));
                (t1 > t0 + 1.0).then_some((t0, t1))
            })
            .collect();
        gaps.sort_by(|x, y| x.0.total_cmp(&y.0));
        if current.is_empty() {
            current.push(a);
        }
        for (t0, t1) in gaps {
            current.push(a + dir * t0);
            if current.len() >= 2 {
                pieces.push(std::mem::take(&mut current));
            }
            current = vec![a + dir * t1];
        }
        current.push(b);
    }
    if current.len() >= 2 {
        // The last run joins the first when no door broke the outline there.
        if let Some(first) = pieces.first_mut()
            && first.first().is_some_and(|p| p.distance(*current.last().expect("points")) < 1.0)
        {
            current.pop();
            current.extend(first.iter().copied());
            *first = current;
        } else if pieces.is_empty() {
            // No doors at all: a closed loop.
            current.pop();
            return vec![current];
        } else {
            pieces.push(current);
        }
    }
    pieces
}

/// Door openings on `floor`, as their two jamb points on each wall face.
fn door_jambs(doc: &Document, geoms: &[WallGeom]) -> Vec<(DVec2, DVec2)> {
    let mut out = Vec::new();
    for g in geoms {
        for o in doc.openings_in(g.id).filter(|o| o.opening_kind.is_door()) {
            let (u0, u1) = (
                o.position.mm() - o.width.mm() / 2.0,
                o.position.mm() + o.width.mm() / 2.0,
            );
            out.push((g.left_at(u0), g.left_at(u1)));
            out.push((g.right_at(u0), g.right_at(u1)));
        }
    }
    out
}

/// Base, chair and crown moldings around each room on `floor`.
pub fn room_moldings(doc: &Document, floor: &Floor, mesh: &mut Mesh) {
    let regions = crate::rooms::detect(doc, floor.id);
    let geoms = crate::walls::wall_geoms(doc, floor.id);
    let doors = door_jambs(doc, &geoms);
    for room in doc.rooms_on(floor.id) {
        let moldings = room.moldings.clone().unwrap_or_else(|| {
            doc.get::<housecraft_model::RoomType>(room.room_type)
                .map(|t| t.moldings.clone())
                .unwrap_or_default()
        });
        if moldings.is_empty() {
            continue;
        }
        let Some(i) = regions.room_at(v2(room.anchor)) else {
            continue;
        };
        // Clockwise, so the room is on the right (where sweeps put the profile).
        let mut outline = regions.rooms[i].polygon.clone();
        if signed_area(&outline) > 0.0 {
            outline.reverse();
        }
        let ceiling = doc
            .room_ceiling_height(room)
            .unwrap_or(floor.ceiling_height)
            .mm();
        mesh.begin(room.id, crate::mesh::Category::Fixture);
        for m in &moldings {
            let prof = spec_profile(doc, &m.spec);
            let h = m.spec.height.mm();
            let z = floor.elevation.mm()
                + match m.place {
                    RoomMoldingPlace::Base => 0.0,
                    RoomMoldingPlace::Chair => m.offset.mm(),
                    RoomMoldingPlace::Crown => ceiling - h,
                };
            if m.place == RoomMoldingPlace::Crown {
                sweep(mesh, &outline, true, &prof, z, Surface::Trim);
            } else {
                for run in runs_around(&outline, &doors) {
                    let closed = run.len() == outline.len() && doors.is_empty();
                    sweep(mesh, &run, closed, &prof, z, Surface::Trim);
                }
            }
        }
        mesh.end();
    }
}

/// A molding polyline: its profiles stacked from the bottom up.
pub fn molding_polyline_mesh(doc: &Document, m: &MoldingPolyline, mesh: &mut Mesh) {
    let Some(floor) = doc.get::<Floor>(m.floor) else {
        return;
    };
    let path: Vec<DVec2> = m.points.iter().map(|p| v2(*p)).collect();
    let mut z = floor.elevation.mm() + m.elevation.mm();
    for spec in &m.stack {
        let prof = spec_profile(doc, spec);
        sweep(mesh, &path, m.closed, &prof, z, Surface::Trim);
        z += spec.height.mm();
    }
}

/// A box on a wall face: from `u0` to `u1` along the face, `z0` to `z1`,
/// standing `t0`–`t1` off it.
#[allow(clippy::too_many_arguments)]
fn face_box(mesh: &mut Mesh, g: &WallGeom, u: (f64, f64), z: (f64, f64), t: (f64, f64), tilt: f64, surface: Surface) {
    if u.1 - u.0 < 0.5 || z.1 - z.0 < 0.5 {
        return;
    }
    let p = |uu: f64, zz: f64, tt: f64| -> DVec3 {
        (g.start + g.dir * uu + g.left * (g.half + tt)).extend(zz)
    };
    // `tilt` brings the top edge back toward the wall (lap siding).
    let top_t = (t.1 - tilt).max(t.0 + 0.5);
    let out = g.left.extend(0.0);
    if tilt > 0.0 {
        // Lapped boards: the lower edge reads darker, like the shadow line
        // under each course.
        let zm = z.0 + (z.1 - z.0) * 0.25;
        let tm = t.1 - tilt * 0.25;
        let lower = [p(u.0, z.0, t.1), p(u.1, z.0, t.1), p(u.1, zm, tm), p(u.0, zm, tm)];
        let mut look = mesh.look(surface);
        for c in &mut look.color[..3] {
            *c *= 0.82;
        }
        let pts: Vec<DVec3> = if crate::roof::newell(&lower).dot(out) < 0.0 {
            lower.iter().rev().copied().collect()
        } else {
            lower.to_vec()
        };
        mesh.polygon_look(&pts, look);
        let upper = [p(u.0, zm, tm), p(u.1, zm, tm), p(u.1, z.1, top_t), p(u.0, z.1, top_t)];
        oriented(mesh, &upper, out, surface);
    } else {
        let front = [p(u.0, z.0, t.1), p(u.1, z.0, t.1), p(u.1, z.1, top_t), p(u.0, z.1, top_t)];
        oriented(mesh, &front, out, surface);
    }
    oriented(mesh, &[p(u.0, z.0, t.0), p(u.1, z.0, t.0), p(u.1, z.0, t.1), p(u.0, z.0, t.1)], -DVec3::Z, surface);
    oriented(mesh, &[p(u.0, z.1, t.0), p(u.1, z.1, t.0), p(u.1, z.1, top_t), p(u.0, z.1, top_t)], DVec3::Z, surface);
    oriented(mesh, &[p(u.0, z.0, t.0), p(u.0, z.0, t.1), p(u.0, z.1, top_t), p(u.0, z.1, t.0)], -g.dir.extend(0.0), surface);
    oriented(mesh, &[p(u.1, z.0, t.0), p(u.1, z.0, t.1), p(u.1, z.1, top_t), p(u.1, z.1, t.0)], g.dir.extend(0.0), surface);
}

/// `span` minus the openings that cross height band `z`.
fn cut(span: (f64, f64), holes: &[(f64, f64, f64, f64)], z: (f64, f64)) -> Vec<(f64, f64)> {
    let mut pieces = vec![span];
    for &(u0, u1, h0, h1) in holes {
        if h1 <= z.0 || h0 >= z.1 {
            continue;
        }
        pieces = pieces
            .into_iter()
            .flat_map(|(a, b)| {
                let mut v = Vec::new();
                if u0 > a {
                    v.push((a, u0.min(b)));
                }
                if u1 < b {
                    v.push((u1.max(a), b));
                }
                v.into_iter().filter(|(x, y)| y > x)
            })
            .collect();
    }
    pieces
}

/// Siding and corner trim on the exterior (left) face of `wall`.
pub fn cladding_mesh(doc: &Document, floor: &Floor, wall: &Wall, g: &WallGeom, mesh: &mut Mesh) {
    let Some(wt) = doc.get::<WallType>(wall.wall_type) else {
        return;
    };
    if wt.cladding.is_none() && wt.corner_trim.is_none() {
        return;
    }
    let z0 = floor.elevation.mm() - floor.floor_structure.mm();
    let z1 = floor.elevation.mm() + doc.wall_height(wall).unwrap_or(floor.ceiling_height).mm();
    // The face runs between the joined corners.
    let along = |p: DVec2| (p - g.start).dot(g.dir);
    let (fu0, fu1) = (along(g.corners[0]), along(g.corners[1]));
    // Each opening's own span and heights (frames included).
    let holes: Vec<(f64, f64, f64, f64)> = doc
        .openings_in(wall.id)
        .map(|o| {
            let half = o.width.mm() / 2.0 + 30.0;
            let s = floor.elevation.mm() + o.sill.mm();
            (o.position.mm() - half, o.position.mm() + half, s - 30.0, s + o.height.mm() + 30.0)
        })
        .collect();
    let trim = wt.corner_trim.map(|c| match c {
        CornerTrim::Board { width } => width.mm(),
        CornerTrim::Quoins { width, .. } => width.mm(),
    });
    let (cu0, cu1) = match trim {
        Some(w) => (fu0 + w, fu1 - w),
        None => (fu0, fu1),
    };
    mesh.begin(wall.id, crate::mesh::Category::Wall);
    match wt.cladding {
        Some(Cladding::Lap { exposure }) | Some(Cladding::Shingle { exposure }) => {
            let e = exposure.mm().max(25.0);
            let shingle = matches!(wt.cladding, Some(Cladding::Shingle { .. }));
            let mut z = z0;
            let mut course = 0;
            while z < z1 - 1.0 {
                let top = (z + e).min(z1);
                for (a, b) in cut((cu0, cu1), &holes, (z, top)) {
                    if shingle {
                        // Shingles: joints every 8"-ish, staggered course to course.
                        let w = 8.0 * IN;
                        let mut u = a - if course % 2 == 1 { w / 2.0 } else { 0.0 };
                        while u < b {
                            let (s0, s1) = (u.max(a), (u + w - 6.0).min(b));
                            face_box(mesh, g, (s0, s1), (z, top), (0.0, 12.0), 8.0, Surface::Wall);
                            u += w;
                        }
                    } else {
                        face_box(mesh, g, (a, b), (z, top), (0.0, 14.0), 10.0, Surface::Wall);
                    }
                }
                z = top;
                course += 1;
            }
        }
        Some(Cladding::BoardAndBatten { spacing, batten }) => {
            let (sp, bw) = (spacing.mm().max(50.0), batten.mm().max(10.0));
            let mut u = cu0 + sp / 2.0;
            while u < cu1 {
                for (zz0, zz1) in cut_vertical((u - bw / 2.0, u + bw / 2.0), &holes, (z0, z1)) {
                    face_box(mesh, g, (u - bw / 2.0, u + bw / 2.0), (zz0, zz1), (0.0, 19.0), 0.0, Surface::Wall);
                }
                u += sp;
            }
        }
        Some(Cladding::StandingSeam { spacing }) => {
            let sp = spacing.mm().max(100.0);
            let mut u = cu0 + sp;
            while u < cu1 {
                for (zz0, zz1) in cut_vertical((u - 6.0, u + 6.0), &holes, (z0, z1)) {
                    face_box(mesh, g, (u - 6.0, u + 6.0), (zz0, zz1), (0.0, 25.0), 0.0, Surface::Wall);
                }
                u += sp;
            }
        }
        None => {}
    }
    // Corner boards or quoins at both ends of the face.
    if let (Some(trim), Some(w)) = (wt.corner_trim, trim) {
        for (a, b) in [(fu0, fu0 + w), (fu1 - w, fu1)] {
            match trim {
                CornerTrim::Board { .. } => {
                    face_box(mesh, g, (a, b), (z0, z1), (0.0, 25.0), 0.0, Surface::Trim)
                }
                CornerTrim::Quoins { height, .. } => {
                    let h = height.mm().max(50.0);
                    let mut z = z0;
                    let mut k = 0;
                    while z < z1 - 1.0 {
                        let top = (z + h - 10.0).min(z1);
                        // Alternate long and short blocks, measured from the corner.
                        let short = w * 0.6;
                        let (s0, s1) = if k % 2 == 0 {
                            (a, b)
                        } else if a <= fu0 + 1.0 {
                            (a, a + short)
                        } else {
                            (b - short, b)
                        };
                        face_box(mesh, g, (s0, s1), (z, top), (0.0, 30.0), 0.0, Surface::Trim);
                        z += h;
                        k += 1;
                    }
                }
            }
        }
    }
    mesh.end();
}

/// A vertical strip at `u` from `z.0` to `z.1` with openings' heights cut out.
fn cut_vertical(u: (f64, f64), holes: &[(f64, f64, f64, f64)], z: (f64, f64)) -> Vec<(f64, f64)> {
    let mut pieces = vec![z];
    for &(u0, u1, h0, h1) in holes {
        if u1 <= u.0 || u0 >= u.1 {
            continue;
        }
        pieces = pieces
            .into_iter()
            .flat_map(|(a, b)| {
                let mut v = Vec::new();
                if h0 > a {
                    v.push((a, h0.min(b)));
                }
                if h1 < b {
                    v.push((h1.max(a), b));
                }
                v.into_iter().filter(|(x, y)| y > x)
            })
            .collect();
    }
    pieces
}

/// Every room, molding polyline and clad wall on `only_floor` (or all).
pub fn build(doc: &Document, only_floor: Option<Id>, mesh: &mut Mesh) {
    for floor in doc.floors_by_elevation() {
        if only_floor.is_some_and(|f| f != floor.id) {
            continue;
        }
        room_moldings(doc, floor, mesh);
        let walls: Vec<&Wall> = doc
            .iter::<Wall>()
            .filter(|w| w.floor == floor.id && w.renovation != housecraft_model::Renovation::Demolish)
            .collect();
        let geoms = crate::walls::wall_geoms(doc, floor.id);
        for g in &geoms {
            if let Some(w) = walls.iter().find(|w| w.id == g.id) {
                cladding_mesh(doc, floor, w, g, mesh);
            }
        }
    }
    for r in doc.iter::<housecraft_model::MaterialRegion>() {
        if only_floor.is_some_and(|f| f != r.floor) {
            continue;
        }
        region_mesh(doc, r, mesh);
    }
    for m in doc.iter::<MoldingPolyline>() {
        if only_floor.is_some_and(|f| f != m.floor) {
            continue;
        }
        mesh.begin(m.id, crate::mesh::Category::Fixture);
        molding_polyline_mesh(doc, m, mesh);
        mesh.end();
    }
}

/// A material region: a thin patch above a floor or on a wall face,
/// painted with its material.
pub fn region_mesh(doc: &Document, r: &housecraft_model::MaterialRegion, mesh: &mut Mesh) {
    use housecraft_model::{Material, RegionTarget};
    let (Some(floor), Some(m)) = (doc.get::<Floor>(r.floor), doc.get::<Material>(r.material)) else {
        return;
    };
    let mut textures = std::mem::take(&mut mesh.textures);
    let look = crate::mesh::look_with_texture(m, &mut textures);
    mesh.textures = textures;
    mesh.begin(r.id, crate::mesh::Category::Floor);
    match &r.target {
        RegionTarget::Floor { points } => {
            let mut pts: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
            if signed_area(&pts) < 0.0 {
                pts.reverse();
            }
            let z = floor.elevation.mm() + 2.0;
            let top: Vec<DVec3> = pts.iter().map(|p| p.extend(z)).collect();
            mesh.polygon_look(&top, look);
        }
        RegionTarget::Wall { wall, exterior, u0, u1, z0, z1 } => {
            if let Some(g) = crate::walls::wall_geoms(doc, r.floor).into_iter().find(|g| g.id == *wall) {
                let (side, off) = if *exterior { (g.left, g.half + 3.0) } else { (-g.left, g.half + 3.0) };
                let base = floor.elevation.mm();
                let p = |u: f64, z: f64| (g.start + g.dir * u + side * off).extend(base + z);
                let quad = [p(u0.mm(), z0.mm()), p(u1.mm(), z0.mm()), p(u1.mm(), z1.mm()), p(u0.mm(), z1.mm())];
                let pts: Vec<DVec3> = if crate::roof::newell(&quad).dot(side.extend(0.0)) < 0.0 {
                    quad.iter().rev().copied().collect()
                } else {
                    quad.to_vec()
                };
                mesh.polygon_look(&pts, look);
            }
        }
    }
    mesh.end();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_skip_doors() {
        // A 4 m square, clockwise, with a door gap on the bottom edge.
        let sq = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(0.0, 4000.0),
            DVec2::new(4000.0, 4000.0),
            DVec2::new(4000.0, 0.0),
        ];
        let runs = runs_around(&sq, &[(DVec2::new(1000.0, 0.0), DVec2::new(1900.0, 0.0))]);
        assert_eq!(runs.len(), 1, "{runs:?}");
        let total: f64 = runs[0].windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!((total - (16000.0 - 900.0)).abs() < 1.0, "{total}");
        assert_eq!(runs_around(&sq, &[]).len(), 1);
    }

    #[test]
    fn cut_removes_openings() {
        let pieces = cut((0.0, 3000.0), &[(1000.0, 2000.0, 900.0, 2100.0)], (1000.0, 1200.0));
        assert_eq!(pieces, vec![(0.0, 1000.0), (2000.0, 3000.0)]);
        assert_eq!(cut((0.0, 3000.0), &[(1000.0, 2000.0, 900.0, 2100.0)], (0.0, 200.0)).len(), 1);
    }
}
