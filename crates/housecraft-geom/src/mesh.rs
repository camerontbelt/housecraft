//! 3D triangle meshes of the building model for the viewer and exports.

use glam::{DVec2, DVec3, Vec3};
use housecraft_model::{Document, Floor, Id, Opening, OpeningKind, Room};

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
    pub first_index: u32,
    pub index_count: u32,
}

pub const WALL_COLOR: [f32; 4] = [0.93, 0.91, 0.87, 1.0];
pub const FLOOR_COLOR: [f32; 4] = [0.72, 0.6, 0.45, 1.0];
pub const DOOR_COLOR: [f32; 4] = [0.55, 0.38, 0.24, 1.0];
pub const GLASS_COLOR: [f32; 4] = [0.55, 0.75, 0.88, 1.0];
pub const FRAME_COLOR: [f32; 4] = [0.97, 0.97, 0.97, 1.0];

impl Mesh {
    fn begin(&mut self, object: Id) -> u32 {
        let first = self.indices.len() as u32;
        self.parts.push(MeshPart {
            object,
            first_index: first,
            index_count: 0,
        });
        first
    }

    fn end(&mut self) {
        let len = self.indices.len() as u32;
        if let Some(p) = self.parts.last_mut() {
            p.index_count = len - p.first_index;
        }
    }

    /// Add a planar polygon (counter-clockwise when seen from its normal side).
    fn polygon(&mut self, pts: &[DVec3], color: [f32; 4]) {
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
    fn prism(&mut self, poly: &[DVec2], z0: f64, z1: f64, color: [f32; 4]) {
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
    mesh
}

fn build_floor(doc: &Document, floor: &Floor, mesh: &mut Mesh) {
    let z0 = floor.elevation.mm();
    let geoms = wall_geoms(doc, floor.id);
    for g in &geoms {
        let Some(wall) = doc.get::<housecraft_model::Wall>(g.id) else {
            continue;
        };
        let height = doc.wall_height(wall).map_or(2438.4, |h| h.mm());
        let openings: Vec<&Opening> = doc.openings_in(g.id).collect();
        mesh.begin(g.id);
        for piece in g.pieces(&openings) {
            mesh.prism(&piece, z0, z0 + height, WALL_COLOR);
        }
        // Wall above and below each opening span.
        for o in &openings {
            let (a, b) = span_of(g, o);
            let quad = vec![g.right_at(a), g.right_at(b), g.left_at(b), g.left_at(a)];
            let head = z0 + (o.sill + o.height).mm();
            mesh.prism(&quad, head, z0 + height, WALL_COLOR);
            if o.sill.mm() > 0.0 {
                mesh.prism(&quad, z0, z0 + o.sill.mm(), WALL_COLOR);
            }
        }
        mesh.end();
        for o in &openings {
            mesh.begin(o.id);
            opening_mesh(g, o, z0, mesh);
            mesh.end();
        }
    }

    let regions = rooms::detect(doc, floor.id);
    let rooms_here: Vec<&Room> = doc.rooms_on(floor.id).collect();
    for region in &regions.rooms {
        let owner = rooms_here
            .iter()
            .find(|r| crate::math::point_in_polygon(v2(r.anchor), &region.centerline))
            .map_or(Id(0), |r| r.id);
        mesh.begin(owner);
        let pts: Vec<DVec3> = region
            .polygon
            .iter()
            .map(|p| DVec3::new(p.x, p.y, z0))
            .collect();
        mesh.polygon(&pts, FLOOR_COLOR);
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
