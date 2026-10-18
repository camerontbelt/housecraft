//! 3D triangle meshes of the building model for the viewer and exports.

use glam::{DVec2, DVec3, Vec3};
use housecraft_model::{
    CeilingStyle, Document, Floor, FoundationKind, Id, Opening, OpeningKind, Room,
};

use crate::math::v2;
use crate::rooms;
use crate::triangulate::triangulate;
use crate::walls::{WallGeom, opening_spans, wall_geoms};

/// Flat-shaded triangles with per-vertex colour. Coordinates are metres,
/// z up, so the viewer works with comfortable magnitudes.
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    /// Which object each index range belongs to (for picking).
    pub parts: Vec<MeshPart>,
}

#[derive(Debug, Clone, Copy)]
pub struct MeshPart {
    pub object: Id,
    pub category: Category,
    pub first_index: u32,
    pub index_count: u32,
}

/// What a mesh part is, so views can show or hide groups (e.g. ceilings in an overview).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Wall,
    Opening,
    Floor,
    Ceiling,
    Roof,
    Foundation,
    Stair,
    Fixture,
}

pub const WALL_COLOR: [f32; 4] = [0.93, 0.91, 0.87, 1.0];
pub const FLOOR_COLOR: [f32; 4] = [0.72, 0.6, 0.45, 1.0];
pub const DOOR_COLOR: [f32; 4] = [0.55, 0.38, 0.24, 1.0];
pub const GLASS_COLOR: [f32; 4] = [0.55, 0.75, 0.88, 1.0];
pub const FRAME_COLOR: [f32; 4] = [0.97, 0.97, 0.97, 1.0];
pub const CEILING_COLOR: [f32; 4] = [0.96, 0.96, 0.95, 1.0];
pub const CONCRETE_COLOR: [f32; 4] = [0.68, 0.68, 0.66, 1.0];
pub const ROOF_COLOR: [f32; 4] = [0.36, 0.34, 0.33, 1.0];
pub const SOFFIT_COLOR: [f32; 4] = [0.9, 0.89, 0.86, 1.0];
pub const PLATFORM_COLOR: [f32; 4] = [0.82, 0.74, 0.6, 1.0];

impl Mesh {
    pub fn begin(&mut self, object: Id, category: Category) -> u32 {
        let first = self.indices.len() as u32;
        self.parts.push(MeshPart {
            object,
            category,
            first_index: first,
            index_count: 0,
        });
        first
    }

    pub fn end(&mut self) {
        let len = self.indices.len() as u32;
        if let Some(p) = self.parts.last_mut() {
            p.index_count = len - p.first_index;
        }
    }

    /// Add a planar polygon (counter-clockwise when seen from its normal side).
    pub fn polygon(&mut self, pts: &[DVec3], color: [f32; 4]) {
        if pts.len() < 3 {
            return;
        }
        let normal = newell_normal(pts);
        if normal.length_squared() < 1e-18 {
            return;
        }
        // Project onto the dominant plane to triangulate.
        let n = normal.normalize();
        let (u, v) = basis(n);
        let flat: Vec<DVec2> = pts.iter().map(|p| DVec2::new(p.dot(u), p.dot(v))).collect();
        let base = self.positions.len() as u32;
        for p in pts {
            self.positions.push(to_m(*p));
            self.normals.push(n.as_vec3().to_array());
            self.colors.push(color);
        }
        for t in triangulate(&flat) {
            self.indices
                .extend([base + t[0] as u32, base + t[1] as u32, base + t[2] as u32]);
        }
    }

    /// Extrude a counter-clockwise plan polygon between two heights (mm).
    pub fn prism(&mut self, poly: &[DVec2], z0: f64, z1: f64, color: [f32; 4]) {
        if z1 - z0 < 0.01 || poly.len() < 3 {
            return;
        }
        let at = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
        let top: Vec<DVec3> = poly.iter().map(|p| at(*p, z1)).collect();
        let bottom: Vec<DVec3> = poly.iter().rev().map(|p| at(*p, z0)).collect();
        self.polygon(&top, color);
        self.polygon(&bottom, color);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if a.distance(b) > 0.01 {
                self.polygon(&[at(a, z0), at(b, z0), at(b, z1), at(a, z1)], color);
            }
        }
    }
}

fn to_m(p: DVec3) -> [f32; 3] {
    (p / 1000.0).as_vec3().to_array()
}

fn newell_normal(pts: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        n += DVec3::new(
            (a.y - b.y) * (a.z + b.z),
            (a.z - b.z) * (a.x + b.x),
            (a.x - b.x) * (a.y + b.y),
        );
    }
    n
}

fn basis(n: DVec3) -> (DVec3, DVec3) {
    let helper = if n.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
    let u = helper.cross(n).normalize();
    let v = n.cross(u);
    (u, v)
}

/// Build the mesh for every floor, or only `only_floor` when given.
pub fn build(doc: &Document, only_floor: Option<Id>) -> Mesh {
    let mut mesh = Mesh::default();
    for floor in doc.floors_by_elevation() {
        if only_floor.is_some_and(|f| f != floor.id) {
            continue;
        }
        build_floor(doc, floor, &mut mesh);
    }
    for plane in doc.iter::<housecraft_model::RoofPlane>() {
        if only_floor.is_some_and(|f| f != plane.floor) {
            continue;
        }
        mesh.begin(plane.id, Category::Roof);
        roof_plane_mesh(plane, &mut mesh);
        mesh.end();
    }
    mesh
}

fn roof_plane_mesh(plane: &housecraft_model::RoofPlane, mesh: &mut Mesh) {
    let top: Vec<DVec3> = plane
        .outline
        .iter()
        .map(|p| {
            let [x, y, z] = p.mm();
            DVec3::new(x, y, z)
        })
        .collect();
    if top.len() < 3 {
        return;
    }
    if plane.gable {
        mesh.polygon(&top, WALL_COLOR);
        return;
    }
    // Underside: the same outline dropped by the vertical thickness.
    let drop = plane.thickness.mm() / plane.pitch.to_radians().cos().max(0.05);
    let bottom: Vec<DVec3> = top.iter().rev().map(|p| *p - DVec3::Z * drop).collect();
    let mut up = top.clone();
    if crate::roof::newell(&up).z < 0.0 {
        up.reverse();
    }
    mesh.polygon(&up, ROOF_COLOR);
    mesh.polygon(&bottom, SOFFIT_COLOR);
    let n = top.len();
    for i in 0..n {
        let (a, b) = (top[i], top[(i + 1) % n]);
        mesh.polygon(
            &[a, b, b - DVec3::Z * drop, a - DVec3::Z * drop],
            FRAME_COLOR,
        );
    }
}

/// Clip a polygon against a counter-clockwise convex polygon (Sutherland–Hodgman).
fn clip_convex(subject: &[DVec2], clip: &[DVec2]) -> Vec<DVec2> {
    let mut out = subject.to_vec();
    let m = clip.len();
    for i in 0..m {
        if out.is_empty() {
            break;
        }
        let (a, b) = (clip[i], clip[(i + 1) % m]);
        let inside = |p: DVec2| crate::math::cross(b - a, p - a) >= -1e-9;
        let input = std::mem::take(&mut out);
        for j in 0..input.len() {
            let (p, q) = (input[j], input[(j + 1) % input.len()]);
            let (pi, qi) = (inside(p), inside(q));
            if pi {
                out.push(p);
            }
            if pi != qi
                && let Some(x) = crate::math::line_intersection(p, q - p, a, b - a)
            {
                out.push(x);
            }
        }
    }
    out
}

/// Ceiling pieces under the sloped roof planes of `floor` covering `room_poly`.
fn vaulted_ceiling(doc: &Document, floor: Id, room_poly: &[DVec2], mesh: &mut Mesh) -> bool {
    let mut any = false;
    for plane in doc
        .iter::<housecraft_model::RoofPlane>()
        .filter(|r| r.floor == floor && !r.gable)
    {
        let outline: Vec<DVec3> = plane
            .outline
            .iter()
            .map(|p| {
                let [x, y, z] = p.mm();
                DVec3::new(x, y, z)
            })
            .collect();
        let mut flat: Vec<DVec2> = outline.iter().map(|p| p.truncate()).collect();
        if crate::math::signed_area(&flat) < 0.0 {
            flat.reverse();
        }
        for t in triangulate(&flat) {
            let tri = [flat[t[0]], flat[t[1]], flat[t[2]]];
            let piece = clip_convex(room_poly, &tri);
            if piece.len() < 3 || crate::math::signed_area(&piece).abs() < 1.0 {
                continue;
            }
            let pts: Vec<DVec3> = piece
                .iter()
                .filter_map(|p| {
                    crate::roof::underside_at(&outline, plane.thickness.mm(), plane.pitch, *p)
                        .or_else(|| {
                            crate::roof::underside_at(
                                &outline,
                                plane.thickness.mm(),
                                plane.pitch,
                                *p + (crate::math::centroid(&piece) - *p) * 1e-6,
                            )
                        })
                        .map(|z| DVec3::new(p.x, p.y, z))
                })
                .collect();
            if pts.len() == piece.len() {
                mesh.polygon(&pts, CEILING_COLOR);
                any = true;
            }
        }
    }
    any
}

fn build_floor(doc: &Document, floor: &Floor, mesh: &mut Mesh) {
    let z0 = floor.elevation.mm();
    let foundation = floor.foundation.as_ref();
    let (wall_category, wall_color) = if foundation.is_some() {
        (Category::Foundation, CONCRETE_COLOR)
    } else {
        (Category::Wall, WALL_COLOR)
    };
    let geoms = wall_geoms(doc, floor.id);
    for g in &geoms {
        let Some(wall) = doc.get::<housecraft_model::Wall>(g.id) else {
            continue;
        };
        let height = doc.wall_height(wall).map_or(2438.4, |h| h.mm());
        let openings: Vec<&Opening> = doc.openings_in(g.id).collect();
        mesh.begin(g.id, wall_category);
        for piece in g.pieces(&openings) {
            mesh.prism(&piece, z0, z0 + height, wall_color);
        }
        if let Some(f) = wall.footing {
            // Footing centred under the wall, running past each end by half its width.
            let hw = f.width.mm() / 2.0;
            let (a, b) = (g.start - g.dir * hw, g.end + g.dir * hw);
            let n = g.left * hw;
            let quad = vec![a - n, b - n, b + n, a + n];
            mesh.prism(&quad, z0 - f.depth.mm(), z0, CONCRETE_COLOR);
        }
        // Wall above and below each opening span.
        for o in &openings {
            let (a, b) = span_of(g, o);
            let quad = vec![g.right_at(a), g.right_at(b), g.left_at(b), g.left_at(a)];
            let head = z0 + (o.sill + o.height).mm();
            mesh.prism(&quad, head, z0 + height, wall_color);
            if o.sill.mm() > 0.0 {
                mesh.prism(&quad, z0, z0 + o.sill.mm(), wall_color);
            }
        }
        mesh.end();
        for o in &openings {
            mesh.begin(o.id, Category::Opening);
            opening_mesh(g, o, z0, mesh);
            mesh.end();
        }
    }

    let regions = rooms::detect(doc, floor.id);
    let rooms_here: Vec<&Room> = doc.rooms_on(floor.id).collect();
    let lowest = doc.floors_by_elevation().first().map(|f| f.id) == Some(floor.id);
    for region in &regions.rooms {
        let room = rooms_here
            .iter()
            .find(|r| crate::math::point_in_polygon(v2(r.anchor), &region.centerline));
        let owner = room.map_or(Id(0), |r| r.id);
        let at = |z: f64| -> Vec<DVec3> {
            region
                .polygon
                .iter()
                .map(|p| DVec3::new(p.x, p.y, z))
                .collect()
        };

        // Floor surface, and the platform under it on upper floors; foundations
        // get a concrete slab (none in a crawlspace).
        match foundation {
            Some(fd) => {
                if fd.kind != FoundationKind::Crawlspace {
                    mesh.begin(owner, Category::Foundation);
                    mesh.prism(
                        &region.polygon,
                        z0 - fd.slab_thickness.mm(),
                        z0,
                        CONCRETE_COLOR,
                    );
                    mesh.end();
                }
                if fd.kind != FoundationKind::Basement {
                    continue;
                }
            }
            None => {
                mesh.begin(owner, Category::Floor);
                mesh.polygon(&at(z0), FLOOR_COLOR);
                if !lowest && floor.floor_structure.mm() > 1.0 {
                    mesh.prism(
                        &region.polygon,
                        z0 - floor.floor_structure.mm(),
                        z0 - 1.0,
                        PLATFORM_COLOR,
                    );
                }
                mesh.end();
            }
        }

        // Ceiling.
        let Some(room) = room else {
            continue;
        };
        let h = doc.room_ceiling_height(room).map_or(2438.4, |h| h.mm());
        let zc = z0 + h;
        mesh.begin(owner, Category::Ceiling);
        match room.ceiling {
            CeilingStyle::None => {}
            CeilingStyle::Flat => mesh.polygon(&at(zc), CEILING_COLOR),
            CeilingStyle::Vaulted => {
                if !vaulted_ceiling(doc, floor.id, &region.polygon, mesh) {
                    mesh.polygon(&at(zc), CEILING_COLOR);
                }
            }
            CeilingStyle::Tray { inset, rise } => {
                let inner = crate::math::inset_polygon(&region.polygon, inset.mm());
                let outer = &region.polygon;
                let n = outer.len();
                if inner.len() == n && crate::math::signed_area(&inner) > 0.0 {
                    let zr = zc + rise.mm();
                    for i in 0..n {
                        let j = (i + 1) % n;
                        let (o0, o1, i0, i1) = (outer[i], outer[j], inner[i], inner[j]);
                        // Lower ring, the vertical step, and the raised centre.
                        mesh.polygon(
                            &[
                                DVec3::new(o0.x, o0.y, zc),
                                DVec3::new(o1.x, o1.y, zc),
                                DVec3::new(i1.x, i1.y, zc),
                                DVec3::new(i0.x, i0.y, zc),
                            ],
                            CEILING_COLOR,
                        );
                        mesh.polygon(
                            &[
                                DVec3::new(i0.x, i0.y, zc),
                                DVec3::new(i1.x, i1.y, zc),
                                DVec3::new(i1.x, i1.y, zr),
                                DVec3::new(i0.x, i0.y, zr),
                            ],
                            CEILING_COLOR,
                        );
                    }
                    let top: Vec<DVec3> = inner.iter().map(|p| DVec3::new(p.x, p.y, zr)).collect();
                    mesh.polygon(&top, CEILING_COLOR);
                } else {
                    mesh.polygon(&at(zc), CEILING_COLOR);
                }
            }
        }
        mesh.end();
    }
}

fn span_of(g: &WallGeom, o: &Opening) -> (f64, f64) {
    let spans = opening_spans(g, &[o]);
    spans.first().copied().unwrap_or((0.0, 0.0))
}

fn opening_mesh(g: &WallGeom, o: &Opening, z0: f64, mesh: &mut Mesh) {
    let (a, b) = span_of(g, o);
    let bottom = z0 + o.sill.mm();
    let top = bottom + o.height.mm();
    let slab = |mesh: &mut Mesh, thickness: f64, u0: f64, u1: f64, z_lo: f64, z_hi: f64, color| {
        let c0 = g.center_at(u0);
        let c1 = g.center_at(u1);
        let t = g.left * (thickness / 2.0);
        let quad = vec![c0 - t, c1 - t, c1 + t, c0 + t];
        mesh.prism(&quad, z_lo, z_hi, color);
    };
    match o.opening_kind {
        OpeningKind::Door(housecraft_model::DoorStyle::Doorway) => {}
        OpeningKind::Door(_) => slab(mesh, 44.0, a, b, bottom, top, DOOR_COLOR),
        OpeningKind::Window(_) => {
            let frame = 50.0;
            slab(
                mesh,
                20.0,
                a + frame,
                b - frame,
                bottom + frame,
                top - frame,
                GLASS_COLOR,
            );
            slab(
                mesh,
                2.0 * g.half,
                a,
                b,
                bottom,
                bottom + frame,
                FRAME_COLOR,
            );
            slab(mesh, 2.0 * g.half, a, b, top - frame, top, FRAME_COLOR);
            slab(mesh, 2.0 * g.half, a, a + frame, bottom, top, FRAME_COLOR);
            slab(mesh, 2.0 * g.half, b - frame, b, bottom, top, FRAME_COLOR);
        }
    }
}

/// Axis-aligned bounds of a mesh in metres.
pub fn mesh_bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
    let mut it = mesh.positions.iter().map(|p| Vec3::from_array(*p));
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}
