//! 3D triangle meshes of the building model for the viewer and exports.

use glam::{DVec2, DVec3, Vec3};
use std::collections::HashMap;

use housecraft_model::{
    CeilingStyle, Document, Floor, FoundationKind, Id, Material, MaterialAssignment, Opening,
    OpeningKind, Renovation, Room, Surface,
    ShutterStyle, TreatmentKind,
};

use crate::math::v2;
use crate::rooms;
use crate::triangulate::triangulate;
use crate::walls::{DoorSwing, WallGeom, opening_spans, wall_geoms};

/// Flat-shaded triangles with per-vertex colour and material. Coordinates
/// are metres, z up, so the viewer works with comfortable magnitudes.
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    /// Per vertex: roughness, metallic, emissive, opacity.
    pub pbr: Vec<[f32; 4]>,
    /// Per vertex: the surface role (for the material painter).
    pub surfaces: Vec<Option<Surface>>,
    /// Per vertex: texture coordinates (in tiles) and texture slot (−1 for none).
    pub uvs: Vec<[f32; 2]>,
    pub tex: Vec<f32>,
    /// The textures the slots refer to.
    pub textures: Vec<TextureKey>,
    /// Lights given off by emissive fixtures: position (m), colour, strength.
    pub lights: Vec<MeshLight>,
    palette: Palette,
    pub indices: Vec<u32>,
    /// Which object each index range belongs to (for picking).
    pub parts: Vec<MeshPart>,
    /// Every planar polygon as added (millimetres), for 2D views such as
    /// elevations and sections that need faces rather than triangles.
    pub faces: Vec<Face>,
    /// Glowing surfaces (emissive materials): part, centre (mm), area
    /// (mm²), colour × strength. They become lights.
    pub emitters: Vec<(usize, DVec3, f64, [f32; 3])>,
}

#[derive(Debug, Clone)]
pub struct Face {
    /// Index into [`Mesh::parts`] of the part the face belongs to.
    pub part: usize,
    pub points: Vec<DVec3>,
    pub normal: DVec3,
    pub color: [f32; 4],
}

/// How a surface looks: colour (alpha unused) and roughness, metallic,
/// emissive, opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub color: [f32; 4],
    pub pbr: [f32; 4],
    /// The surface role it was resolved for (none for one-off colours).
    pub surface: Option<Surface>,
    /// The material's texture, if it has one.
    pub tex: Option<TexRef>,
}

/// Where a texture comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum TextureKey {
    /// An image file.
    File(String),
    /// A generated tile pattern in a colour.
    Pattern(housecraft_model::TilePattern, housecraft_model::Color),
}

/// A texture on a surface: its slot in [`Mesh::textures`] and how it lies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TexRef {
    pub slot: u32,
    /// Size of one tile, mm.
    pub size: [f64; 2],
    /// Degrees.
    pub angle: f64,
    /// mm along the texture's own axes.
    pub offset: [f64; 2],
}

/// A point light in the model (a light fixture).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshLight {
    pub object: Id,
    /// Metres.
    pub position: [f32; 3],
    pub color: [f32; 3],
    /// Relative brightness (1 = a typical fixture).
    pub intensity: f32,
    pub shadows: bool,
    /// How the light spreads: 0 all round (wall sconce), 1 the lower half
    /// (ceiling fixture), 2 a downward cone (recessed can).
    pub spread: u8,
}

/// Resolved materials for the surfaces being built.
#[derive(Debug, Clone, Default)]
struct Palette {
    painted: HashMap<(Id, Option<Surface>), Look>,
    defaults: HashMap<Surface, Look>,
    /// The floor of every object that has one.
    floors: HashMap<Id, Id>,
    /// Object and floor of the part being built.
    object: Option<Id>,
    floor: Option<Id>,
}

impl Palette {
    fn new(doc: &Document, textures: &mut Vec<TextureKey>) -> Self {
        let mut p = Palette::default();
        let mut look = |m: &Material| {
            let mut l = look_of(m);
            l.tex = texture_of(m, textures);
            l
        };
        for a in doc.iter::<MaterialAssignment>() {
            if let Some(m) = doc.get::<Material>(a.material) {
                p.painted.insert((a.object, a.surface), look(m));
            }
        }
        p.floors = doc
            .objects
            .keys()
            .filter_map(|id| Some((*id, doc.floor_of(*id)?)))
            .collect();
        for (s, m) in &doc.active_defaults().materials {
            if let Some(m) = doc.get::<Material>(*m) {
                p.defaults.insert(*s, look(m));
            }
        }
        p
    }

    fn look(&self, surface: Surface) -> Look {
        [self.object, self.floor]
            .into_iter()
            .flatten()
            .flat_map(|o| [(o, Some(surface)), (o, None)])
            .find_map(|k| self.painted.get(&k))
            .or_else(|| self.defaults.get(&surface))
            .copied()
            .unwrap_or_else(|| builtin_look(surface))
            .with_surface(surface)
    }
}

impl Look {
    fn with_surface(self, surface: Surface) -> Look {
        Look {
            surface: Some(surface),
            ..self
        }
    }
}

/// A material's texture, registering it in `textures`: its colour image,
/// or else its tile pattern.
fn texture_of(m: &Material, textures: &mut Vec<TextureKey>) -> Option<TexRef> {
    let (key, size) = match (&m.maps.color, &m.pattern) {
        (Some(map), _) => (
            TextureKey::File(map.path.clone()),
            [
                m.texture_width.map_or(1219.2, |l| l.mm()),
                m.texture_height.map_or(1219.2, |l| l.mm()),
            ],
        ),
        (None, Some(p)) => {
            let (w, h) = crate::pattern::repeat(p);
            (TextureKey::Pattern(*p, m.color), [w, h])
        }
        (None, None) => return None,
    };
    let slot = match textures.iter().position(|k| *k == key) {
        Some(i) => i,
        None => {
            textures.push(key);
            textures.len() - 1
        }
    };
    Some(TexRef {
        slot: slot as u32,
        size,
        angle: m.texture_angle,
        offset: [m.texture_offset[0].mm(), m.texture_offset[1].mm()],
    })
}

/// A material's look with its texture registered in `textures`.
pub fn look_with_texture(m: &Material, textures: &mut Vec<TextureKey>) -> Look {
    let mut l = look_of(m);
    l.tex = texture_of(m, textures);
    l
}

/// The 3D look of a material.
pub fn look_of(m: &Material) -> Look {
    let [r, g, b, _] = m.color.0;
    let c = |v: u8| v as f32 / 255.0;
    Look {
        color: [c(r), c(g), c(b), 1.0],
        pbr: [
            m.roughness as f32,
            m.metallic as f32,
            // Water is marked by emissive -1 (the 3D view ripples it).
            if m.class == housecraft_model::MaterialClass::Water { -1.0 } else { m.emissive as f32 },
            m.opacity as f32,
        ],
        surface: None,
        tex: None,
    }
}

/// Colours used when a plan has no material for a surface.
pub fn builtin_look(surface: Surface) -> Look {
    let (color, pbr) = match surface {
        Surface::Wall => ([0.93, 0.91, 0.87, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Floor => ([0.72, 0.6, 0.45, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Door => ([0.55, 0.38, 0.24, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Glass => ([0.55, 0.75, 0.88, 1.0], [0.05, 0.0, 0.0, 0.25]),
        Surface::Frame => ([0.97, 0.97, 0.97, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Ceiling => ([0.96, 0.96, 0.95, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Concrete => ([0.68, 0.68, 0.66, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Roof => ([0.36, 0.34, 0.33, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Soffit => ([0.9, 0.89, 0.86, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Stair => ([0.68, 0.52, 0.36, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Stringer => ([0.58, 0.44, 0.30, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Railing => ([0.95, 0.95, 0.93, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Device => ([0.96, 0.95, 0.92, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::Lamp => ([1.0, 0.93, 0.62, 1.0], [0.5, 0.0, 1.0, 1.0]),
        Surface::Platform => ([0.82, 0.74, 0.6, 1.0], [0.8, 0.0, 0.0, 1.0]),
        Surface::CabinetBox | Surface::CabinetDoor => ([0.84, 0.71, 0.54, 1.0], [0.4, 0.0, 0.0, 1.0]),
        Surface::Countertop => ([0.47, 0.44, 0.41, 1.0], [0.3, 0.0, 0.0, 1.0]),
        Surface::Backsplash => ([0.94, 0.94, 0.92, 1.0], [0.3, 0.0, 0.0, 1.0]),
        Surface::Hardware => ([0.77, 0.76, 0.74, 1.0], [0.3, 1.0, 0.0, 1.0]),
        Surface::Trim => ([0.97, 0.97, 0.97, 1.0], [0.4, 0.0, 0.0, 1.0]),
        Surface::Furniture => ([0.48, 0.28, 0.18, 1.0], [0.4, 0.0, 0.0, 1.0]),
        Surface::Upholstery => ([0.7, 0.67, 0.62, 1.0], [0.9, 0.0, 0.0, 1.0]),
    };
    Look {
        color,
        pbr,
        surface: None,
        tex: None,
    }
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
    /// Cabinets, countertops and appliances.
    Cabinet,
    Terrain,
    Framing,
}


impl Mesh {
    /// Add `other`'s triangles as parts of `object`, untextured (another
    /// plan shown for reference).
    pub fn append_untextured(&mut self, other: &Mesh, object: Id) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.colors.extend_from_slice(&other.colors);
        self.pbr.extend_from_slice(&other.pbr);
        self.uvs.extend_from_slice(&other.uvs);
        self.surfaces.extend(other.surfaces.iter().map(|_| None));
        self.tex.extend(other.tex.iter().map(|_| -1.0));
        let first = self.indices.len() as u32;
        self.indices.extend(other.indices.iter().map(|i| i + base));
        for p in &other.parts {
            self.parts.push(MeshPart {
                object,
                category: p.category,
                first_index: p.first_index + first,
                index_count: p.index_count,
            });
        }
    }

    /// A mesh whose surfaces take their materials from `doc`.
    pub fn for_document(doc: &Document) -> Self {
        let mut textures = Vec::new();
        let palette = Palette::new(doc, &mut textures);
        Mesh {
            palette,
            textures,
            ..Mesh::default()
        }
    }

    /// The look `surface` gets on the part being built.
    pub fn look(&self, surface: Surface) -> Look {
        self.palette.look(surface)
    }

    /// A material painted on the whole of the part being built (or its
    /// floor), if any.
    pub fn painted_whole(&self) -> Option<Look> {
        [self.palette.object, self.palette.floor]
            .into_iter()
            .flatten()
            .find_map(|o| self.palette.painted.get(&(o, None)))
            .copied()
    }

    /// Start a part for `object` (its surfaces take materials painted on it
    /// or on its floor).
    pub fn begin(&mut self, object: Id, category: Category) -> u32 {
        self.palette.object = Some(object);
        self.palette.floor = self.palette.floors.get(&object).copied();
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
    pub fn polygon(&mut self, pts: &[DVec3], surface: Surface) {
        let look = self.palette.look(surface);
        self.polygon_look(pts, look);
    }

    /// [`Mesh::polygon`] with an explicit look.
    pub fn polygon_look(&mut self, pts: &[DVec3], look: Look) {
        let color = look.color;
        if pts.len() < 3 {
            return;
        }
        let normal = newell_normal(pts);
        if normal.length_squared() < 1e-18 {
            return;
        }
        if look.pbr[2] > 0.3 && look.surface != Some(Surface::Lamp) && !self.parts.is_empty() {
            let c = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
            let k = look.pbr[2];
            self.emitters.push((
                self.parts.len() - 1,
                c,
                normal.length() / 2.0,
                [look.color[0] * k, look.color[1] * k, look.color[2] * k],
            ));
        }
        // Project onto the dominant plane to triangulate.
        let n = normal.normalize();
        let (u, v) = basis(n);
        let flat: Vec<DVec2> = pts.iter().map(|p| DVec2::new(p.dot(u), p.dot(v))).collect();
        if !self.parts.is_empty() {
            self.faces.push(Face {
                part: self.parts.len() - 1,
                points: pts.to_vec(),
                normal: n,
                color,
            });
        }
        let base = self.positions.len() as u32;
        // Planar texture coordinates: plan x/y on floors and roofs, along
        // the face and up on walls.
        let (tu, tv) = if n.z.abs() > 0.7 {
            (DVec3::X, DVec3::Y)
        } else {
            (DVec3::Z.cross(n).normalize_or(DVec3::X), DVec3::Z)
        };
        for p in pts {
            self.positions.push(to_m(*p));
            self.normals.push(n.as_vec3().to_array());
            self.colors.push(color);
            self.pbr.push(look.pbr);
            self.surfaces.push(look.surface);
            match look.tex {
                Some(t) => {
                    let q = DVec2::new(p.dot(tu), p.dot(tv));
                    let r = DVec2::from_angle(-t.angle.to_radians()).rotate(q)
                        - DVec2::new(t.offset[0], t.offset[1]);
                    self.uvs.push([
                        (r.x / t.size[0].max(1.0)) as f32,
                        (r.y / t.size[1].max(1.0)) as f32,
                    ]);
                    self.tex.push(t.slot as f32);
                }
                None => {
                    self.uvs.push([0.0, 0.0]);
                    self.tex.push(-1.0);
                }
            }
        }
        for t in triangulate(&flat) {
            self.indices
                .extend([base + t[0] as u32, base + t[1] as u32, base + t[2] as u32]);
        }
    }

    /// Extrude a counter-clockwise plan polygon between two heights (mm).
    pub fn prism(&mut self, poly: &[DVec2], z0: f64, z1: f64, surface: Surface) {
        let look = self.palette.look(surface);
        self.prism_look(poly, z0, z1, look);
    }

    /// [`Mesh::prism`] with an explicit look.
    pub fn prism_look(&mut self, poly: &[DVec2], z0: f64, z1: f64, color: Look) {
        if z1 - z0 < 0.01 || poly.len() < 3 {
            return;
        }
        let at = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
        let top: Vec<DVec3> = poly.iter().map(|p| at(*p, z1)).collect();
        let bottom: Vec<DVec3> = poly.iter().rev().map(|p| at(*p, z0)).collect();
        self.polygon_look(&top, color);
        self.polygon_look(&bottom, color);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if a.distance(b) > 0.01 {
                self.polygon_look(&[at(a, z0), at(b, z0), at(b, z1), at(a, z1)], color);
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
    let mut mesh = Mesh::for_document(doc);
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
        let roof = &doc.active_defaults().roof;
        // Layer bands (top down) as fractions of the thickness, foam pale.
        let total: f64 = roof.layers.iter().map(|l| l.thickness.mm()).sum();
        let bands: Vec<(f64, Surface)> = if total > 0.0 {
            roof.layers
                .iter()
                .map(|l| {
                    let name = format!("{} {}", l.name, l.material).to_lowercase();
                    let foam = ["foam", "eps", "xps", "insul", "core", "polyiso"].iter().any(|k| name.contains(k));
                    (l.thickness.mm() / total, if foam { Surface::Trim } else { Surface::Frame })
                })
                .collect()
        } else {
            Vec::new()
        };
        roof_plane_mesh(plane, roof.fascia.mm(), roof.shadow_board.mm(), &bands, &mut mesh);
        mesh.end();
    }
    for stair in doc.iter::<housecraft_model::Stair>() {
        if only_floor.is_some_and(|f| f != stair.floor) {
            continue;
        }
        mesh.begin(stair.id, Category::Stair);
        stair_mesh(doc, stair, &mut mesh);
        mesh.end();
    }
    for e in doc.iter::<housecraft_model::Electrical>() {
        if only_floor.is_some_and(|f| f != e.floor) || e.renovation == Renovation::Demolish {
            continue;
        }
        mesh.begin(e.id, Category::Fixture);
        let start = mesh.pbr.len();
        electrical_mesh(doc, e, &mut mesh);
        mesh.end();
        if e.electrical_kind.emits_light() {
            let settings = e
                .light
                .unwrap_or_else(|| housecraft_model::LightSettings::typical(e.electrical_kind));
            let level = if settings.on { settings.dimmer } else { 0.0 };
            // Lenses glow only as much as the fixture is turned up.
            for (pbr, surface) in mesh.pbr[start..].iter_mut().zip(&mesh.surfaces[start..]) {
                if *surface == Some(Surface::Lamp) {
                    pbr[2] *= level as f32;
                }
            }
            if level > 0.0
                && settings.lumens > 0.0
                && let Some(at) = light_position(doc, e)
            {
                let [r, g, b] = settings.rgb();
                mesh.lights.push(MeshLight {
                    object: e.id,
                    position: to_m(at),
                    color: [r as f32, g as f32, b as f32],
                    intensity: (settings.lumens / 800.0 * level) as f32,
                    shadows: settings.shadows,
                    spread: match e.electrical_kind {
                        housecraft_model::ElectricalKind::RecessedLight => 2,
                        housecraft_model::ElectricalKind::WallLight => 0,
                        _ => 1,
                    },
                });
            }
        }
    }
    for c in doc.iter::<housecraft_model::Cabinet>() {
        if only_floor.is_some_and(|f| f != c.floor) || c.renovation == Renovation::Demolish {
            continue;
        }
        mesh.begin(c.id, Category::Cabinet);
        crate::cabinets::cabinet_mesh(doc, c, &mut mesh);
        mesh.end();
    }
    for t in doc.iter::<housecraft_model::Countertop>() {
        if only_floor.is_some_and(|f| f != t.floor) {
            continue;
        }
        mesh.begin(t.id, Category::Cabinet);
        crate::cabinets::countertop_mesh(doc, t, &mut mesh);
        mesh.end();
    }
    crate::trim::build(doc, only_floor, &mut mesh);
    for bs in doc.iter::<housecraft_model::Backsplash>() {
        if only_floor.is_some_and(|f| f != bs.floor) {
            continue;
        }
        let Some(floor) = doc.get::<Floor>(bs.floor) else {
            continue;
        };
        mesh.begin(bs.id, Category::Cabinet);
        let path: Vec<DVec2> = bs.points.iter().map(|p| v2(*p)).collect();
        let (t, h) = (bs.thickness.mm(), bs.height.mm());
        let profile = |h: f64| [DVec2::ZERO, DVec2::new(t, 0.0), DVec2::new(t, h), DVec2::new(0.0, h)];
        let z0 = floor.elevation.mm() + bs.bottom.mm();
        let cuts = backsplash_cutouts(doc, bs.floor, &path, z0, z0 + h);
        if cuts.iter().all(|c| c.is_empty()) {
            crate::sweep::sweep(&mut mesh, &path, false, &profile(h), z0, Surface::Backsplash);
        } else {
            // Stop short of windows: run by run, lower under each window's sill.
            for (k, seg_cuts) in cuts.iter().enumerate() {
                let (a, b) = (path[k], path[k + 1]);
                let len = a.distance(b);
                let d = (b - a) / len.max(1e-9);
                let mut from = 0.0;
                let mut runs: Vec<(f64, f64, f64)> = Vec::new();
                for (u0, u1, sill) in seg_cuts {
                    if *u0 > from {
                        runs.push((from, *u0, h));
                    }
                    runs.push((u0.max(from), *u1, sill - z0));
                    from = from.max(*u1);
                }
                if from < len {
                    runs.push((from, len, h));
                }
                for (u0, u1, hh) in runs {
                    if u1 - u0 > 1.0 && hh > 5.0 {
                        crate::sweep::sweep(&mut mesh, &[a + d * u0, a + d * u1], false, &profile(hh.min(h)), z0, Surface::Backsplash);
                    }
                }
            }
        }
        mesh.end();
    }
    for s in doc.iter::<housecraft_model::Symbol>() {
        if only_floor.is_some_and(|f| f != s.floor) || s.renovation == Renovation::Demolish {
            continue;
        }
        mesh.begin(s.id, Category::Cabinet);
        crate::cabinets::symbol_mesh(doc, s, &mut mesh);
        mesh.end();
        // Light fixture symbols light the room from near their top.
        if let Some(def) = doc.get::<housecraft_model::SymbolDef>(s.def)
            && let housecraft_model::SymbolBehavior::Light { light } = &def.behavior
            && light.on
            && light.lumens > 0.0
            && let Some(floor) = doc.get::<Floor>(s.floor)
        {
            let hull = crate::cabinets::symbol_outline(doc, s);
            if hull.len() >= 3 {
                let c = crate::math::centroid(&hull);
                let z = floor.elevation.mm() + s.elevation.mm() + s.height.mm() * 0.85;
                let [r, g, b] = light.rgb();
                mesh.lights.push(MeshLight {
                    object: s.id,
                    position: to_m(c.extend(z)),
                    color: [r as f32, g as f32, b as f32],
                    intensity: (light.lumens / 800.0 * light.dimmer) as f32,
                    shadows: light.shadows,
                    spread: 0,
                });
            }
        }
    }
    for t in doc.iter::<housecraft_model::Terrain>() {
        if only_floor.is_none_or(|f| f == t.floor) {
            crate::terrain::terrain_mesh(doc, t, &mut mesh);
        }
    }
    crate::site::build(doc, only_floor, &mut mesh);
    crate::framing::build(doc, only_floor, &mut mesh);
    crate::deck::build(doc, only_floor, &mut mesh);
    // Roof trim and accessories.
    let roof = &doc.active_defaults().roof;
    if roof.ridge_caps {
        mesh.begin(Id(0), Category::Roof);
        crate::roof_extras::ridge_caps(doc, only_floor, &mut mesh);
        mesh.end();
    }
    if roof.gutters {
        mesh.begin(Id(0), Category::Roof);
        crate::roof_extras::gutters(doc, only_floor, roof.fascia.mm(), &mut mesh);
        mesh.end();
    }
    if roof.frieze.mm() > 1.0 {
        for floor in doc.floors_by_elevation().into_iter().filter(|f| f.foundation.is_none()) {
            if only_floor.is_none_or(|o| o == floor.id) {
                crate::roof_extras::frieze(doc, floor, roof.frieze.mm(), &mut mesh);
            }
        }
    }
    for plane in doc.iter::<housecraft_model::RoofPlane>().filter(|p| !p.skylights.is_empty()) {
        if only_floor.is_none_or(|f| f == plane.floor) {
            mesh.begin(plane.id, Category::Roof);
            crate::roof_extras::skylights(plane, &mut mesh);
            mesh.end();
        }
    }
    for d in doc.iter::<housecraft_model::Dormer>() {
        if only_floor.is_none_or(|f| f == d.floor) {
            mesh.begin(d.id, Category::Roof);
            crate::dormer::mesh(doc, d, &mut mesh);
            mesh.end();
        }
    }
    // Emissive surfaces light their surroundings: one light per glowing
    // object, at the middle of its glow, brighter the more glows.
    let mut glow: std::collections::BTreeMap<usize, (DVec3, f64, [f32; 3])> = std::collections::BTreeMap::new();
    for (part, c, area, color) in &mesh.emitters {
        let e = glow.entry(*part).or_insert((DVec3::ZERO, 0.0, [0.0; 3]));
        e.0 += *c * *area;
        e.1 += area;
        for (m, c) in e.2.iter_mut().zip(color) {
            *m = m.max(*c);
        }
    }
    for (part, (sum, area, color)) in glow {
        if area < 1.0 {
            continue;
        }
        let object = mesh.parts[part].object;
        let m2 = area / 1e6;
        mesh.lights.push(MeshLight {
            object,
            position: to_m(sum / area),
            color,
            intensity: (m2 * 4.0).clamp(0.2, 6.0) as f32,
            shadows: false,
            spread: 0,
        });
    }
    for x in doc.iter::<housecraft_model::Solid>() {
        if only_floor.is_some_and(|f| f != x.floor) {
            continue;
        }
        mesh.begin(x.id, Category::Fixture);
        crate::solid::solid_mesh(doc, x, &mut mesh);
        mesh.end();
    }
    for a in doc.iter::<housecraft_model::Appliance>() {
        if only_floor.is_some_and(|f| f != a.floor) || a.renovation == Renovation::Demolish {
            continue;
        }
        mesh.begin(a.id, Category::Cabinet);
        crate::cabinets::appliance_mesh(doc, a, &mut mesh);
        mesh.end();
    }
    mesh
}

/// A regular polygon approximating a circle.
fn disc(c: DVec2, r: f64, sides: usize) -> Vec<DVec2> {
    (0..sides)
        .map(|i| c + DVec2::from_angle(i as f64 / sides as f64 * std::f64::consts::TAU) * r)
        .collect()
}

/// A rectangle centred on `c`, `w` along `dir` and `d` across it.
fn rect(c: DVec2, dir: DVec2, w: f64, d: f64) -> Vec<DVec2> {
    let (u, v) = (dir * (w / 2.0), crate::math::perp(dir) * (d / 2.0));
    ccw(vec![c - u - v, c + u - v, c + u + v, c - u + v])
}

/// Where a light fixture's light comes from (mm): just below or in front of
/// its lens.
pub fn light_position(doc: &Document, e: &housecraft_model::Electrical) -> Option<DVec3> {
    use housecraft_model::ElectricalKind as K;
    let floor = doc.get::<Floor>(e.floor)?;
    let z0 = floor.elevation.mm();
    let ceiling = z0 + floor.ceiling_height.mm();
    let p = v2(e.position);
    let out = DVec2::from_angle(e.rotation.to_radians());
    let z = e
        .height
        .or(e.electrical_kind.default_height())
        .map(|h| z0 + h.mm());
    let (q, zz) = match e.electrical_kind {
        K::CeilingLight => (p, ceiling - 150.0),
        K::RecessedLight => (p, ceiling - 40.0),
        K::WallLight => (p + out * 160.0, z.unwrap_or(z0 + 1800.0) + 40.0),
        K::CeilingFan => (p, ceiling - 560.0),
        _ => return None,
    };
    Some(q.extend(zz))
}

fn electrical_mesh(doc: &Document, e: &housecraft_model::Electrical, mesh: &mut Mesh) {
    use housecraft_model::ElectricalKind as K;
    let Some(floor) = doc.get::<Floor>(e.floor) else {
        return;
    };
    let z0 = floor.elevation.mm();
    let ceiling = z0 + floor.ceiling_height.mm();
    let p = v2(e.position);
    let out = DVec2::from_angle(e.rotation.to_radians());
    let along = crate::math::perp(out);
    let z = e
        .height
        .or(e.electrical_kind.default_height())
        .map(|h| z0 + h.mm());
    match e.electrical_kind {
        K::Outlet | K::GfciOutlet | K::Outlet240 | K::Switch | K::Dimmer => {
            // Cover plate standing proud of the wall face.
            let zc = z.unwrap_or(z0 + 300.0);
            let plate = rect(p + out * 4.0, along, 70.0, 8.0);
            mesh.prism(&plate, zc - 57.0, zc + 57.0, Surface::Device);
        }
        K::WallLight => {
            let zc = z.unwrap_or(z0 + 1800.0);
            mesh.prism(
                &rect(p + out * 10.0, along, 120.0, 20.0),
                zc - 100.0,
                zc + 100.0,
                Surface::Device,
            );
            mesh.prism(
                &rect(p + out * 70.0, along, 150.0, 100.0),
                zc - 60.0,
                zc + 140.0,
                Surface::Lamp,
            );
        }
        K::CeilingLight => {
            mesh.prism(&disc(p, 170.0, 16), ceiling - 90.0, ceiling, Surface::Lamp);
        }
        K::RecessedLight => {
            mesh.prism(&disc(p, 90.0, 16), ceiling - 6.0, ceiling, Surface::Lamp);
        }
        K::SmokeDetector => {
            mesh.prism(&disc(p, 65.0, 12), ceiling - 40.0, ceiling, Surface::Device);
        }
        K::HoseBibb | K::GasOutlet => {
            let zc = z.unwrap_or(z0 + 450.0);
            let color = Look {
                color: if e.electrical_kind == K::GasOutlet {
                    [0.85, 0.75, 0.2, 1.0]
                } else {
                    [0.72, 0.55, 0.25, 1.0]
                },
                pbr: [0.4, 1.0, 0.0, 1.0],
                surface: None,
                tex: None,
            };
            mesh.prism_look(
                &rect(p + out * 40.0, along, 40.0, 80.0),
                zc - 20.0,
                zc + 20.0,
                color,
            );
        }
        K::FloorDrain => {
            let grate = Look {
                color: [0.35, 0.35, 0.37, 1.0],
                pbr: [0.4, 1.0, 0.0, 1.0],
                surface: None,
                tex: None,
            };
            mesh.prism_look(&disc(p, 75.0, 16), z0, z0 + 3.0, grate);
        }
        K::SupplyRegister => {
            mesh.prism(
                &rect(p, out, 250.0, 100.0),
                ceiling - 8.0,
                ceiling,
                Surface::Device,
            );
        }
        K::ReturnGrille => {
            let zc = z.unwrap_or(z0 + 300.0);
            mesh.prism(
                &rect(p + out * 4.0, along, 500.0, 8.0),
                zc - 150.0,
                zc + 150.0,
                Surface::Device,
            );
        }
        K::Thermostat => {
            let zc = z.unwrap_or(z0 + 1500.0);
            mesh.prism(
                &rect(p + out * 12.0, along, 100.0, 24.0),
                zc - 50.0,
                zc + 50.0,
                Surface::Device,
            );
        }
        K::ExhaustFan => {
            mesh.prism(
                &rect(p, DVec2::X, 250.0, 250.0),
                ceiling - 12.0,
                ceiling,
                Surface::Device,
            );
        }
        K::JunctionBox => {
            mesh.prism(
                &rect(p, DVec2::X, 100.0, 100.0),
                ceiling - 50.0,
                ceiling,
                Surface::Device,
            );
        }
        K::CeilingFan => {
            let hub = ceiling - 300.0;
            mesh.prism(&disc(p, 20.0, 8), hub, ceiling, Surface::Device);
            mesh.prism(&disc(p, 110.0, 16), hub - 120.0, hub, Surface::Device);
            for i in 0..4 {
                let d = DVec2::from_angle(
                    e.rotation.to_radians() + i as f64 * std::f64::consts::FRAC_PI_2,
                );
                mesh.prism(
                    &rect(p + d * 420.0, d, 640.0, 130.0),
                    hub - 60.0,
                    hub - 48.0,
                    Surface::Stair,
                );
            }
            mesh.prism(&disc(p, 80.0, 12), hub - 200.0, hub - 120.0, Surface::Lamp);
        }
    }
}

fn stair_mesh(doc: &Document, stair: &housecraft_model::Stair, mesh: &mut Mesh) {
    let g = crate::stairs::geometry(doc, stair);
    for step in &g.steps {
        if let Some(low) = step.low_z {
            // A ramp run: the top slopes from the bottom edge to the far edge.
            let p = &step.poly;
            let tops = [low, step.z, step.z, low];
            let at = |i: usize, z: f64| p[i].extend(z);
            mesh.polygon(&[at(0, tops[0]), at(1, tops[1]), at(2, tops[2]), at(3, tops[3])], Surface::Stair);
            mesh.polygon(&[at(3, g.base_z), at(2, g.base_z), at(1, g.base_z), at(0, g.base_z)], Surface::Stair);
            for i in 0..4 {
                let j = (i + 1) % 4;
                mesh.polygon(&[at(i, g.base_z), at(j, g.base_z), at(j, tops[j]), at(i, tops[i])], Surface::Stair);
                mesh.polygon(&[at(i, g.base_z), at(i, tops[i]), at(j, tops[j]), at(j, g.base_z)], Surface::Stair);
            }
            continue;
        }
        // Open risers: just the tread board (landings stay solid).
        let bottom = if stair.extras.open_risers && !step.landing { step.z - 38.0 } else { g.base_z };
        mesh.prism(&ccw(step.poly.clone()), bottom, step.z, Surface::Stair);
        if !step.nosing.is_empty() {
            mesh.prism(
                &ccw(step.nosing.clone()),
                step.z - 25.0,
                step.z,
                Surface::Stair,
            );
        }
    }
    // Stringers: boards under each side, their top edge just above the nosings.
    for [a, b] in &g.stringers {
        let lift = DVec3::Z * 20.0;
        sloped_board(*a + lift, *b + lift, 19.0, 300.0, Surface::Stringer, mesh);
    }
    let spacing = 4.0 * 25.4;
    let def = |id: Option<Id>| id.and_then(|i| doc.get::<housecraft_model::SymbolDef>(i));
    let (baluster, newel) = (def(stair.baluster), def(stair.newel));
    for rail in &g.rails {
        for w in rail.windows(2) {
            rail_segment(w[0], w[1], spacing, true, baluster, mesh);
        }
        // Newel posts at each end, a little above the rail.
        if let Some(newel) = newel {
            for p in [rail.first(), rail.last()].into_iter().flatten() {
                let base = p.z - stair.railing_height.mm();
                symbol_post(newel, p.truncate(), base, p.z + 75.0, mesh);
            }
        }
    }
}

/// A 3D symbol standing at `c` from `z0` to `z1`, scaled evenly to that
/// height, in the railing's look.
fn symbol_post(def: &housecraft_model::SymbolDef, c: DVec2, z0: f64, z1: f64, mesh: &mut Mesh) {
    let (lo, hi) = def.bounds();
    let h = (hi[2] - lo[2]) as f64;
    if h <= 0.0 || z1 - z0 < 1.0 {
        return;
    }
    let k = (z1 - z0) / h;
    let mid = DVec2::new((lo[0] + hi[0]) as f64 / 2.0, (lo[1] + hi[1]) as f64 / 2.0);
    let look = mesh.look(Surface::Railing);
    for tri in &def.triangles {
        let pts: Vec<DVec3> = tri
            .iter()
            .filter_map(|i| def.positions.get(*i as usize))
            .map(|p| {
                let q = (DVec2::new(p[0] as f64, p[1] as f64) - mid) * k;
                DVec3::new(c.x + q.x, c.y + q.y, z0 + (p[2] - lo[2]) as f64 * k)
            })
            .collect();
        if pts.len() == 3 {
            mesh.polygon_look(&pts, look);
        }
    }
}

/// A handrail between two 3D points with balusters dropping `drop_to_stair`
/// (stairs: to the tread surface below the rail) or to `z0` for level railings.
fn rail_segment(
    a: DVec3,
    b: DVec3,
    spacing: f64,
    along_stair: bool,
    baluster: Option<&housecraft_model::SymbolDef>,
    mesh: &mut Mesh,
) {
    let flat = (b - a).truncate();
    let len = flat.length();
    if len < 1.0 {
        return;
    }
    let dir = flat / len;
    let side = crate::math::perp(dir) * 25.0;
    let rail_depth = 50.0;
    // Handrail: a sloped bar (two triangles per face via polygons).
    let corners = |p: DVec3, dz: f64| -> [DVec3; 2] {
        [
            DVec3::new(p.x + side.x, p.y + side.y, p.z + dz),
            DVec3::new(p.x - side.x, p.y - side.y, p.z + dz),
        ]
    };
    let [a_l, a_r] = corners(a, 0.0);
    let [b_l, b_r] = corners(b, 0.0);
    let [a_lb, a_rb] = corners(a, -rail_depth);
    let [b_lb, b_rb] = corners(b, -rail_depth);
    mesh.polygon(&[a_r, b_r, b_l, a_l], Surface::Railing);
    mesh.polygon(&[a_lb, b_lb, b_rb, a_rb], Surface::Railing);
    mesh.polygon(&[a_l, b_l, b_lb, a_lb], Surface::Railing);
    mesh.polygon(&[a_rb, b_rb, b_r, a_r], Surface::Railing);
    // Balusters.
    let n = (len / spacing).floor() as usize;
    let rail_height_above_base = if along_stair { 36.0 * 25.4 } else { 0.0 };
    for i in 0..=n {
        let t = (i as f64 * spacing + (len - n as f64 * spacing) / 2.0) / len;
        let p = a.lerp(b, t);
        let top = p.z - rail_depth;
        let bottom = if along_stair {
            p.z - rail_height_above_base
        } else {
            a.z.min(b.z) - (a.z.min(b.z) - top).abs()
        };
        if top - bottom < 10.0 {
            continue;
        }
        let c = p.truncate();
        if let Some(def) = baluster {
            symbol_post(def, c, bottom, top, mesh);
            continue;
        }
        let q = 15.0;
        let square = vec![
            c + DVec2::new(-q, -q),
            c + DVec2::new(q, -q),
            c + DVec2::new(q, q),
            c + DVec2::new(-q, q),
        ];
        mesh.prism(&square, bottom, top, Surface::Railing);
    }
}

/// A board along a->b (top edge), `half` thick either side, `depth` deep below.
fn sloped_board(a: DVec3, b: DVec3, half: f64, depth: f64, color: Surface, mesh: &mut Mesh) {
    let flat = (b - a).truncate();
    if flat.length() < 1.0 {
        return;
    }
    let side = crate::math::perp(flat.normalize()) * half;
    let at = |p: DVec3, s: f64, dz: f64| DVec3::new(p.x + side.x * s, p.y + side.y * s, p.z + dz);
    let (al, ar, bl, br) = (
        at(a, 1.0, 0.0),
        at(a, -1.0, 0.0),
        at(b, 1.0, 0.0),
        at(b, -1.0, 0.0),
    );
    let (alb, arb, blb, brb) = (
        at(a, 1.0, -depth),
        at(a, -1.0, -depth),
        at(b, 1.0, -depth),
        at(b, -1.0, -depth),
    );
    mesh.polygon(&[ar, br, bl, al], color);
    mesh.polygon(&[alb, blb, brb, arb], color);
    mesh.polygon(&[al, bl, blb, alb], color);
    mesh.polygon(&[arb, brb, br, ar], color);
    mesh.polygon(&[al, alb, arb, ar], color);
    mesh.polygon(&[br, brb, blb, bl], color);
}

fn ccw(mut poly: Vec<DVec2>) -> Vec<DVec2> {
    if crate::math::signed_area(&poly) < 0.0 {
        poly.reverse();
    }
    poly
}

/// Stairwell openings cut into `floor`'s platform (from stairs below) and into
/// its ceiling (from its own stairs).
fn stairwell_holes(doc: &Document, floor: Id) -> (Vec<Vec<DVec2>>, Vec<Vec<DVec2>>) {
    let mut platform = Vec::new();
    let mut ceiling = Vec::new();
    for s in doc
        .iter::<housecraft_model::Stair>()
        .filter(|s| s.stairwell)
    {
        let above = doc.floor_above(s.floor).map(|f| f.id);
        if above.is_none() {
            continue;
        }
        let pieces = crate::stairs::geometry(doc, s).footprint();
        if above == Some(floor) {
            platform.extend(pieces.iter().cloned());
        }
        if s.floor == floor {
            ceiling.extend(pieces);
        }
    }
    (platform, ceiling)
}

fn roof_plane_mesh(plane: &housecraft_model::RoofPlane, fascia: f64, shadow: f64, bands: &[(f64, Surface)], mesh: &mut Mesh) {
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
        mesh.polygon(&top, Surface::Wall);
        return;
    }
    // Underside: the same outline dropped by the vertical thickness.
    let drop = plane.thickness.mm() / plane.pitch.to_radians().cos().max(0.05);
    let bottom: Vec<DVec3> = top.iter().rev().map(|p| *p - DVec3::Z * drop).collect();
    let mut up = top.clone();
    if crate::roof::newell(&up).z < 0.0 {
        up.reverse();
    }
    mesh.polygon(&up, Surface::Roof);
    mesh.polygon(&bottom, Surface::Soffit);
    let n = top.len();
    for i in 0..n {
        let (a, b) = (top[i], top[(i + 1) % n]);
        if bands.is_empty() {
            mesh.polygon(&[a, b, b - DVec3::Z * drop, a - DVec3::Z * drop], Surface::Frame);
            continue;
        }
        // A layered roof shows its layers on the cut edges.
        let mut from = 0.0;
        for (f, surface) in bands {
            let (z0, z1) = (drop * from, drop * (from + f));
            mesh.polygon(&[a - DVec3::Z * z0, b - DVec3::Z * z0, b - DVec3::Z * z1, a - DVec3::Z * z1], *surface);
            from += f;
        }
    }
    // Fascia: a trim board on the level edge(s) at the bottom of the plane.
    let low = top.iter().map(|p| p.z).fold(f64::MAX, f64::min);
    if fascia > 0.0 {
        let centre = top.iter().fold(DVec2::ZERO, |s, p| s + p.truncate()) / n as f64;
        for i in 0..n {
            let (a, b) = (top[i], top[(i + 1) % n]);
            if (a.z - low).abs() > 1.0 || (b.z - low).abs() > 1.0 || a.distance(b) < 10.0 {
                continue;
            }
            let (a2, b2) = (a.truncate(), b.truncate());
            let mut out = crate::math::perp((b2 - a2).normalize());
            if out.dot((a2 + b2) / 2.0 - centre) < 0.0 {
                out = -out;
            }
            let board = vec![a2, b2, b2 + out * 25.0, a2 + out * 25.0];
            let depth = fascia.max(drop);
            mesh.prism(&ccw(board), low - depth, low + 10.0, Surface::Frame);
            // A shadow board along the top of the fascia's face.
            if shadow > 0.0 {
                let sb = vec![a2 + out * 25.0, b2 + out * 25.0, b2 + out * 44.0, a2 + out * 44.0];
                mesh.prism(&ccw(sb), low + 10.0 - shadow.min(depth), low + 10.0, Surface::Trim);
            }
        }
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
                mesh.polygon(&pts, Surface::Ceiling);
                any = true;
            }
        }
    }
    any
}

fn build_floor(doc: &Document, floor: &Floor, mesh: &mut Mesh) {
    let z0 = floor.elevation.mm();
    if let Some(l) = crate::piers::layout(doc, floor) {
        mesh.begin(floor.id, Category::Foundation);
        let beam_bottom = l.top - crate::piers::BEAM_DEPTH;
        for p in &l.piers {
            let column = if l.square { rect(*p, DVec2::X, l.diameter, l.diameter) } else { disc(*p, l.diameter / 2.0, 16) };
            mesh.prism(
                &column,
                l.bottom,
                beam_bottom,
                Surface::Concrete,
            );
            mesh.prism(
                &rect(*p, DVec2::X, l.footing, l.footing),
                l.bottom - l.footing_depth,
                l.bottom,
                Surface::Concrete,
            );
        }
        for [a, b] in &l.beams {
            let d = (*b - *a).normalize_or_zero();
            let n = crate::math::perp(d) * (crate::piers::BEAM_WIDTH / 2.0);
            mesh.prism(
                &ccw(vec![*a - n, *b - n, *b + n, *a + n]),
                beam_bottom,
                l.top,
                Surface::Stair,
            );
        }
        mesh.end();
    }
    let foundation = floor.foundation.as_ref();
    let (wall_category, wall_color) = if foundation.is_some() {
        (Category::Foundation, Surface::Concrete)
    } else {
        (Category::Wall, Surface::Wall)
    };
    let geoms = wall_geoms(doc, floor.id);
    // Roofs built on a lower floor (a story and a half): walls under them
    // stop at the roof's underside.
    let roofs_below: Vec<(Vec<DVec3>, f64)> = doc
        .iter::<housecraft_model::RoofPlane>()
        .filter(|r| !r.gable && doc.get::<housecraft_model::Floor>(r.floor).is_some_and(|f| f.elevation.mm() < floor.elevation.mm() - 1.0))
        .map(|r| {
            let outline = r.outline.iter().map(|q| { let [x, y, z] = q.mm(); DVec3::new(x, y, z) }).collect();
            (outline, r.thickness.mm() / r.pitch.to_radians().cos().max(0.1))
        })
        .collect();
    let under_roof = |p: DVec2| -> Option<f64> {
        roofs_below
            .iter()
            .filter_map(|(o, drop)| crate::roof_extras::plane_z(o, p).map(|z| z - drop))
            .reduce(f64::min)
    };
    for g in &geoms {
        let Some(wall) = doc.get::<housecraft_model::Wall>(g.id) else {
            continue;
        };
        // 3D shows the finished design: demolished work is gone and
        // demolished openings are filled in.
        if wall.renovation == Renovation::Demolish {
            continue;
        }
        let height = doc.wall_height(wall).map_or(2438.4, |h| h.mm());
        let openings: Vec<&Opening> = doc
            .openings_in(g.id)
            .filter(|o| o.renovation != Renovation::Demolish)
            .collect();
        mesh.begin(g.id, wall_category);
        let railing = doc
            .get::<housecraft_model::WallType>(wall.wall_type)
            .and_then(|t| t.railing);
        if let Some(r) = railing {
            let h = z0 + r.height.mm();
            rail_segment(
                DVec3::new(g.start.x, g.start.y, h),
                DVec3::new(g.end.x, g.end.y, h),
                r.baluster_spacing.mm(),
                false,
                None,
                mesh,
            );
            // Newel posts: at the ends, every `newel_spacing`, and where asked.
            for u in newel_positions(g.length, r.newel_spacing.map(|s| s.mm()), &wall.shape.newels) {
                let c = g.start + g.dir * u;
                let s = 45.0;
                mesh.prism(
                    &[c + DVec2::new(-s, -s), c + DVec2::new(s, -s), c + DVec2::new(s, s), c + DVec2::new(-s, s)],
                    z0,
                    h + 60.0,
                    Surface::Railing,
                );
            }
            // Bottom rail so the balusters have something to stand on.
            let half = g.half.min(30.0);
            let n = g.left * half;
            mesh.prism(
                &[g.start - n, g.end - n, g.end + n, g.start + n],
                z0,
                z0 + 40.0,
                Surface::Railing,
            );
            mesh.end();
            continue;
        }
        let shape = &wall.shape;
        let plain_top = |u: f64| z0 + shape.top_at(u, height);
        let mut breaks = shape.breaks();
        // Trimmed to a roof above where it dips below the wall's own top.
        let clipped = !roofs_below.is_empty()
            && (0..=20).any(|k| {
                let u = g.length * k as f64 / 20.0;
                under_roof(g.start + g.dir * u).is_some_and(|z| z < plain_top(u) - 1.0)
            });
        if clipped {
            let n = (g.length / 150.0).ceil().max(1.0) as usize;
            breaks.extend((1..n).map(|k| g.length * k as f64 / n as f64));
            breaks.sort_by(f64::total_cmp);
        }
        let top = |u: f64| {
            let t = plain_top(u);
            if clipped {
                under_roof(g.start + g.dir * u.clamp(0.0, g.length)).map_or(t, |z| z.min(t).max(z0))
            } else {
                t
            }
        };
        let shared = crate::walls::shared_openings(doc, &geoms, g);
        let niche_spans: Vec<(f64, f64)> = shape
            .niches
            .iter()
            .map(|n| (n.position.mm() - n.width.mm() / 2.0, n.position.mm() + n.width.mm() / 2.0))
            .collect();
        let extra: Vec<(f64, f64)> = niche_spans.iter().copied().chain(shared.iter().map(|s| (s.0, s.1))).collect();
        // A pony wall's lower type, flush with the exterior face.
        let pony = shape.pony.as_ref().and_then(|p| {
            doc.get::<housecraft_model::WallType>(p.lower_type).map(|t| (p.split.mm(), t.thickness().mm()))
        });
        let wall_bottom = pony.map_or(z0, |(split, _)| z0 + split);
        for piece in g.pieces_with(&openings, &extra) {
            wall_piece(mesh, g, &piece, wall_bottom, &top, &breaks, wall_color);
            if let Some((split, lower)) = pony {
                let us: Vec<f64> = piece.iter().map(|p| (*p - g.start).dot(g.dir)).collect();
                let (ua, ub) = (us.iter().copied().fold(f64::MAX, f64::min), us.iter().copied().fold(f64::MIN, f64::max));
                let q = vec![g.left_at(ua) - g.left * lower, g.left_at(ub) - g.left * lower, g.left_at(ub), g.left_at(ua)];
                mesh.prism(&crate::walls::ccw_pub(q), z0, z0 + split, wall_color);
            }
        }
        for n in &shape.niches {
            let (a, b) = ((n.position.mm() - n.width.mm() / 2.0).max(0.0), (n.position.mm() + n.width.mm() / 2.0).min(g.length));
            if b <= a {
                continue;
            }
            let quad = vec![g.right_at(a), g.right_at(b), g.left_at(b), g.left_at(a)];
            let (sill, head) = (z0 + n.sill.mm(), z0 + (n.sill + n.height).mm());
            mesh.prism(&quad, z0, sill, wall_color);
            wall_piece(mesh, g, &quad, head, &top, &breaks, wall_color);
            // What is left behind the recess.
            let d = n.depth.mm().min(2.0 * g.half - 10.0);
            let back = if n.interior {
                vec![g.right_at(a) + g.left * d, g.right_at(b) + g.left * d, g.left_at(b), g.left_at(a)]
            } else {
                vec![g.right_at(a), g.right_at(b), g.left_at(b) - g.left * d, g.left_at(a) - g.left * d]
            };
            mesh.prism(&back, sill, head, wall_color);
        }
        // Wall above and below the openings of a wall built back to back.
        for (a, b, sill, h) in &shared {
            let quad = vec![g.right_at(*a), g.right_at(*b), g.left_at(*b), g.left_at(*a)];
            wall_piece(mesh, g, &quad, z0 + sill + h, &top, &breaks, wall_color);
            if *sill > 0.0 {
                mesh.prism(&quad, z0, z0 + sill, wall_color);
            }
        }
        if let Some(f) = wall.effective_footing() {
            // Footing centred under the wall, running past each end by half its width.
            let hw = f.width.mm() / 2.0;
            let (a, b) = (g.start - g.dir * hw, g.end + g.dir * hw);
            let n = g.left * hw;
            let quad = vec![a - n, b - n, b + n, a + n];
            mesh.prism(&quad, z0 - f.depth.mm(), z0, Surface::Concrete);
        }
        // Wall above and below each opening span.
        for o in &openings {
            let (a, b) = span_of(g, o);
            let quad = vec![g.right_at(a), g.right_at(b), g.left_at(b), g.left_at(a)];
            let head = z0 + (o.sill + o.height).mm();
            wall_piece(mesh, g, &quad, head, &top, &breaks, wall_color);
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

    let (platform_holes, ceiling_holes) = stairwell_holes(doc, floor.id);
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
                if !matches!(fd.kind, FoundationKind::Crawlspace | FoundationKind::Piers) {
                    mesh.begin(owner, Category::Foundation);
                    mesh.prism(
                        &region.polygon,
                        z0 - fd.slab_thickness.mm(),
                        z0,
                        Surface::Concrete,
                    );
                    mesh.end();
                }
                if fd.kind != FoundationKind::Basement {
                    continue;
                }
            }
            None => {
                mesh.begin(owner, Category::Floor);
                for piece in crate::math::minus_convex_holes(&region.polygon, &platform_holes) {
                    let pts: Vec<DVec3> = piece.iter().map(|p| DVec3::new(p.x, p.y, z0)).collect();
                    mesh.polygon(&pts, Surface::Floor);
                    if !lowest && floor.floor_structure.mm() > 1.0 {
                        platform_mesh(floor, &ccw(piece.clone()), z0, mesh);
                    }
                }
                mesh.end();
            }
        }

        // A curb round a garage on a slab with one.
        if let (Some(room), true) = (room, foundation.is_none()) {
            let curb = doc
                .floors_by_elevation()
                .iter()
                .filter_map(|f| f.foundation.as_ref())
                .find(|fd| fd.kind == FoundationKind::Slab)
                .map_or(0.0, |fd| fd.garage_curb.mm());
            let garage = doc.get::<housecraft_model::RoomType>(room.room_type).is_some_and(|t| t.name.to_lowercase().contains("garage"));
            if lowest && garage && curb > 1.0 {
                mesh.begin(owner, Category::Foundation);
                garage_curb(doc, floor.id, &region.polygon, z0, curb, mesh);
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
            CeilingStyle::Flat => {
                for piece in crate::math::minus_convex_holes(&region.polygon, &ceiling_holes) {
                    let pts: Vec<DVec3> = piece.iter().map(|p| DVec3::new(p.x, p.y, zc)).collect();
                    mesh.polygon(&pts, Surface::Ceiling);
                }
            }
            CeilingStyle::Vaulted => {
                if !vaulted_ceiling(doc, floor.id, &region.polygon, mesh) {
                    mesh.polygon(&at(zc), Surface::Ceiling);
                }
            }
            CeilingStyle::Coffered {
                spacing,
                beam_width,
                depth,
            } => {
                for piece in crate::math::minus_convex_holes(&region.polygon, &ceiling_holes) {
                    let pts: Vec<DVec3> = piece.iter().map(|p| DVec3::new(p.x, p.y, zc)).collect();
                    mesh.polygon(&pts, Surface::Ceiling);
                }
                coffer_beams(
                    &region.polygon,
                    zc,
                    spacing.mm(),
                    beam_width.mm(),
                    depth.mm(),
                    mesh,
                );
            }
            CeilingStyle::Barrel { rise } => barrel_ceiling(&region.polygon, zc, rise.mm(), mesh),
            CeilingStyle::Tray { inset, rise, sloped, rope_light } => {
                let inner = crate::math::inset_polygon(&region.polygon, inset.mm());
                let outer = &region.polygon;
                let n = outer.len();
                if inner.len() == n && crate::math::signed_area(&inner) > 0.0 {
                    let zr = zc + rise.mm();
                    for i in 0..n {
                        let j = (i + 1) % n;
                        let (o0, o1, i0, i1) = (outer[i], outer[j], inner[i], inner[j]);
                        if sloped {
                            // One sloped face from the walls up to the raised centre.
                            mesh.polygon(
                                &[
                                    DVec3::new(o0.x, o0.y, zc),
                                    DVec3::new(o1.x, o1.y, zc),
                                    DVec3::new(i1.x, i1.y, zr),
                                    DVec3::new(i0.x, i0.y, zr),
                                ],
                                Surface::Ceiling,
                            );
                            continue;
                        }
                        // Lower ring, the vertical step, and the raised centre.
                        mesh.polygon(
                            &[
                                DVec3::new(o0.x, o0.y, zc),
                                DVec3::new(o1.x, o1.y, zc),
                                DVec3::new(i1.x, i1.y, zc),
                                DVec3::new(i0.x, i0.y, zc),
                            ],
                            Surface::Ceiling,
                        );
                        mesh.polygon(
                            &[
                                DVec3::new(i0.x, i0.y, zc),
                                DVec3::new(i1.x, i1.y, zc),
                                DVec3::new(i1.x, i1.y, zr),
                                DVec3::new(i0.x, i0.y, zr),
                            ],
                            Surface::Ceiling,
                        );
                    }
                    let top: Vec<DVec3> = inner.iter().map(|p| DVec3::new(p.x, p.y, zr)).collect();
                    mesh.polygon(&top, Surface::Ceiling);
                    if rope_light {
                        // Cove lighting: a warm rope light hidden along the step.
                        let ring: Vec<DVec2> = inner.iter().chain(inner.first()).copied().collect();
                        for (p, _) in crate::site::along(&ring, false, 600.0) {
                            mesh.lights.push(MeshLight {
                                object: owner,
                                position: to_m(p.extend(zc + 40.0)),
                                color: [1.0, 0.82, 0.6],
                                intensity: 0.12,
                                shadows: false,
                                spread: 0,
                            });
                        }
                    }
                } else {
                    mesh.polygon(&at(zc), Surface::Ceiling);
                }
            }
        }
        mesh.end();
    }
}

/// The floor platform under an upper floor, drawn layer by layer.
fn platform_mesh(floor: &Floor, poly: &[DVec2], z0: f64, mesh: &mut Mesh) {
    if floor.platform.is_empty() {
        mesh.prism(
            poly,
            z0 - floor.floor_structure.mm(),
            z0 - 1.0,
            Surface::Platform,
        );
        return;
    }
    let mut top = z0 - 1.0;
    for (i, layer) in floor.platform.iter().enumerate() {
        let bottom = top - layer.thickness.mm();
        // Alternate shades so the layers read in a cutaway.
        let k = if i % 2 == 0 { 1.0 } else { 0.85 };
        let mut look = mesh.look(Surface::Platform);
        for v in &mut look.color[..3] {
            *v *= k;
        }
        mesh.prism_look(poly, bottom, top, look);
        top = bottom;
    }
}

/// A grid of beams under a flat ceiling at `zc`, aligned with the room's long axis.
fn coffer_beams(poly: &[DVec2], zc: f64, spacing: f64, width: f64, depth: f64, mesh: &mut Mesh) {
    if spacing <= width || spacing < 50.0 {
        return;
    }
    let u = crate::math::long_axis(poly);
    let v = crate::math::perp(u);
    for (along, across) in [(u, v), (v, u)] {
        // Beams run along `along`, spaced across `across`.
        let (lo, hi) = poly
            .iter()
            .map(|p| p.dot(across))
            .fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
        let (alo, ahi) = poly
            .iter()
            .map(|p| p.dot(along))
            .fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
        let n = ((hi - lo) / spacing).floor() as i64;
        let first = lo + ((hi - lo) - n as f64 * spacing) / 2.0;
        for i in 0..=n {
            let c = first + i as f64 * spacing;
            if c - width / 2.0 <= lo + 1.0 || c + width / 2.0 >= hi - 1.0 {
                continue; // beams at the walls would just thicken them
            }
            let rect = [
                along * alo + across * (c - width / 2.0),
                along * ahi + across * (c - width / 2.0),
                along * ahi + across * (c + width / 2.0),
                along * alo + across * (c + width / 2.0),
            ];
            for piece in crate::math::intersect_convex(poly, &rect) {
                mesh.prism(&ccw(piece), zc - depth, zc, Surface::Ceiling);
            }
        }
    }
}

/// A vault along the room's long axis: height above `zc` follows a half
/// ellipse across the room, `rise` at the middle.
fn barrel_ceiling(poly: &[DVec2], zc: f64, rise: f64, mesh: &mut Mesh) {
    let u = crate::math::long_axis(poly);
    let v = crate::math::perp(u);
    let (lo, hi) = poly
        .iter()
        .map(|p| p.dot(v))
        .fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
    let width = (hi - lo).max(1.0);
    let height = |p: DVec2| {
        let t = ((p.dot(v) - lo) / width).clamp(0.0, 1.0);
        zc + rise * (1.0 - (2.0 * t - 1.0).powi(2)).max(0.0).sqrt()
    };
    // Bands parallel to the axis; within each, height is interpolated
    // linearly across the band so every face stays planar.
    let bands = 16;
    for k in 0..bands {
        let (t0, t1) = (k as f64 / bands as f64, (k + 1) as f64 / bands as f64);
        let (c0, c1) = (lo + t0 * width, lo + t1 * width);
        let (z0, z1) = (height(v * c0), height(v * c1));
        let a = u * 1.0;
        // Points with a larger v lie to the left of a line running along u.
        let mut band = crate::math::clip_half_plane(poly, v * c0, v * c0 + a, true);
        band = crate::math::clip_half_plane(&band, v * c1, v * c1 + a, false);
        if band.len() < 3 {
            continue;
        }
        let lift = |p: &DVec2| {
            let s = ((p.dot(v) - c0) / (c1 - c0)).clamp(0.0, 1.0);
            DVec3::new(p.x, p.y, z0 + (z1 - z0) * s)
        };
        let pts: Vec<DVec3> = band.iter().map(lift).collect();
        mesh.polygon(&pts, Surface::Ceiling);
    }
    // Close the ends: infill from the wall tops up to the curve.
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let steps = 24;
        let pts: Vec<DVec2> = (0..=steps)
            .map(|s| a.lerp(b, s as f64 / steps as f64))
            .collect();
        if pts.iter().map(|p| height(*p) - zc).fold(0.0, f64::max) < 1.0 {
            continue;
        }
        let mut face: Vec<DVec3> = pts.iter().map(|p| DVec3::new(p.x, p.y, zc)).collect();
        face.extend(pts.iter().rev().map(|p| DVec3::new(p.x, p.y, height(*p))));
        mesh.polygon(&face, Surface::Ceiling);
    }
}

/// Newel posts along a railing `len` long: both ends, every `spacing` at
/// most, and any extra positions.
pub fn newel_positions(len: f64, spacing: Option<f64>, extra: &[housecraft_units::Length]) -> Vec<f64> {
    let mut us = vec![0.0, len];
    if let Some(s) = spacing.filter(|s| *s > 100.0) {
        let n = (len / s).ceil() as usize;
        for i in 1..n {
            us.push(len * i as f64 / n as f64);
        }
    }
    if spacing.is_none() && extra.is_empty() {
        return Vec::new();
    }
    us.extend(extra.iter().map(|u| u.mm().clamp(0.0, len)));
    us.sort_by(f64::total_cmp);
    us.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    us
}

/// One piece of wall from `zb` up to its top (`top(u)` at each distance
/// along the wall), sliced where the top bends or steps.
fn wall_piece(mesh: &mut Mesh, g: &WallGeom, piece: &[DVec2], zb: f64, top: &dyn Fn(f64) -> f64, breaks: &[f64], surface: Surface) {
    let u_of = |p: DVec2| (p - g.start).dot(g.dir);
    if breaks.is_empty() {
        let zt = top(u_of(piece[0]));
        mesh.prism(piece, zb, zt, surface);
        return;
    }
    let us: Vec<f64> = piece.iter().map(|p| u_of(*p)).collect();
    let (lo, hi) = (us.iter().copied().fold(f64::MAX, f64::min), us.iter().copied().fold(f64::MIN, f64::max));
    let mut cuts = vec![lo];
    cuts.extend(breaks.iter().copied().filter(|b| *b > lo + 0.5 && *b < hi - 0.5));
    cuts.push(hi);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    for w in cuts.windows(2) {
        let (u0, u1) = (w[0], w[1]);
        let slab = clip_u(&clip_u(piece, g, u0, true), g, u1, false);
        if slab.len() < 3 {
            continue;
        }
        // Inside the slab (so a step belongs to the side it is on).
        let z_at = |p: DVec2| top(u_of(p).clamp(u0 + 1e-3, u1 - 1e-3));
        let at = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
        let tops: Vec<f64> = slab.iter().map(|p| z_at(*p).max(zb)).collect();
        if tops.iter().all(|z| *z - zb < 0.01) {
            continue;
        }
        let top_face: Vec<DVec3> = slab.iter().zip(&tops).map(|(p, z)| at(*p, *z)).collect();
        let bottom: Vec<DVec3> = slab.iter().rev().map(|p| at(*p, zb)).collect();
        mesh.polygon(&top_face, surface);
        mesh.polygon(&bottom, surface);
        for i in 0..slab.len() {
            let j = (i + 1) % slab.len();
            if slab[i].distance(slab[j]) > 0.01 {
                mesh.polygon(&[at(slab[i], zb), at(slab[j], zb), at(slab[j], tops[j]), at(slab[i], tops[i])], surface);
            }
        }
    }
}

/// The part of `poly` with distance along `g` at least (`keep_above`) or at
/// most `u`.
fn clip_u(poly: &[DVec2], g: &WallGeom, u: f64, keep_above: bool) -> Vec<DVec2> {
    let side = |p: DVec2| {
        let d = (p - g.start).dot(g.dir) - u;
        if keep_above { d } else { -d }
    };
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sa, sb) = (side(a), side(b));
        if sa >= 0.0 {
            out.push(a);
        }
        if (sa >= 0.0) != (sb >= 0.0) {
            out.push(a + (b - a) * (sa / (sa - sb)));
        }
    }
    out
}

/// A concrete curb along the inside of a garage, broken at doors.
fn garage_curb(doc: &Document, floor: Id, poly: &[DVec2], z0: f64, height: f64, mesh: &mut Mesh) {
    // Door spans in plan.
    let mut doors: Vec<(DVec2, f64)> = Vec::new();
    for g in wall_geoms(doc, floor) {
        for o in doc.openings_in(g.id).filter(|o| o.opening_kind.is_door()) {
            doors.push((g.center_at(o.position.mm()), o.width.mm() / 2.0 + 20.0));
        }
    }
    let width = 150.0;
    let n = poly.len();
    let inward = if crate::math::signed_area(poly) > 0.0 { 1.0 } else { -1.0 };
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let len = a.distance(b);
        if len < 50.0 {
            continue;
        }
        let d = (b - a) / len;
        let inn = crate::math::perp(d) * inward;
        // Pieces of the edge clear of doors.
        let mut cuts: Vec<(f64, f64)> = doors
            .iter()
            .filter(|(c, _)| (*c - a).dot(inn).abs() < 400.0)
            .map(|(c, hw)| {
                let u = (*c - a).dot(d);
                (u - hw, u + hw)
            })
            .collect();
        cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut from = 0.0;
        let mut pieces = Vec::new();
        for (c0, c1) in cuts {
            if c0 > from {
                pieces.push((from, c0.min(len)));
            }
            from = from.max(c1);
        }
        if from < len {
            pieces.push((from, len));
        }
        for (u0, u1) in pieces {
            if u1 - u0 < 30.0 {
                continue;
            }
            let (p0, p1) = (a + d * u0, a + d * u1);
            let quad = vec![p0, p1, p1 + inn * width, p0 + inn * width];
            mesh.prism(&ccw(quad), z0, z0 + height, Surface::Concrete);
        }
    }
}

/// For each segment of a backsplash path, the windows it runs past whose
/// sill is below its top: (from, to) along the segment and the sill height.
fn backsplash_cutouts(doc: &Document, floor: Id, path: &[DVec2], _bottom: f64, top: f64) -> Vec<Vec<(f64, f64, f64)>> {
    let fz = doc.get::<Floor>(floor).map_or(0.0, |f| f.elevation.mm());
    let geoms = wall_geoms(doc, floor);
    path.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let len = a.distance(b);
            let d = (b - a) / len.max(1e-9);
            let mut v: Vec<(f64, f64, f64)> = Vec::new();
            for g in &geoms {
                if g.dir.dot(d).abs() < 0.99 {
                    continue;
                }
                for o in doc.openings_in(g.id).filter(|o| !o.opening_kind.is_door()) {
                    let c = g.center_at(o.position.mm());
                    let off = (c - a).perp_dot(d).abs();
                    let sill = fz + o.sill.mm();
                    if off > g.half + 150.0 || sill >= top {
                        continue;
                    }
                    let u = (c - a).dot(d);
                    let (u0, u1) = ((u - o.width.mm() / 2.0).max(0.0), (u + o.width.mm() / 2.0).min(len));
                    if u1 > u0 {
                        v.push((u0, u1, sill));
                    }
                }
            }
            v.sort_by(|x, y| x.0.total_cmp(&y.0));
            v
        })
        .collect()
}

fn span_of(g: &WallGeom, o: &Opening) -> (f64, f64) {
    let spans = opening_spans(g, &[o]);
    spans.first().copied().unwrap_or((0.0, 0.0))
}

const DOOR_THICKNESS: f64 = 44.0;

/// A flat leaf hinged at `pivot`, closed along `closed` (unit), swung `angle`
/// radians toward `side`.
#[allow(clippy::too_many_arguments)]
fn swung_leaf(
    pivot: DVec2,
    closed: DVec2,
    side: DVec2,
    width: f64,
    angle: f64,
    z: (f64, f64),
    glass: Option<housecraft_model::DoorGlass>,
    mesh: &mut Mesh,
) {
    let d = closed * angle.cos() + side * angle.sin();
    // Thickness goes back into the opening (away from the swing side) when closed.
    let n = -side * angle.cos() + closed * angle.sin();
    let quad = vec![
        pivot,
        pivot + d * width,
        pivot + d * width + n * DOOR_THICKNESS,
        pivot + n * DOOR_THICKNESS,
    ];
    mesh.prism(&ccw(quad), z.0, z.1, Surface::Door);
    if let Some(gl) = glass {
        // A glass panel through the leaf, inside a rail and stile border.
        let m = (width * 0.18).min(120.0);
        let (g0, g1) = match gl {
            housecraft_model::DoorGlass::Full => (z.0 + 250.0, z.1 - 120.0),
            housecraft_model::DoorGlass::Upper => ((z.0 + z.1) / 2.0, z.1 - 120.0),
        };
        let quad = vec![
            pivot + d * m - n * 2.0,
            pivot + d * (width - m) - n * 2.0,
            pivot + d * (width - m) + n * (DOOR_THICKNESS + 2.0),
            pivot + d * m + n * (DOOR_THICKNESS + 2.0),
        ];
        mesh.prism(&ccw(quad), g0, g1, Surface::Glass);
    }
}

#[allow(clippy::too_many_arguments)]
fn door_mesh(
    g: &WallGeom,
    o: &Opening,
    style: housecraft_model::DoorStyle,
    a: f64,
    b: f64,
    bottom: f64,
    top: f64,
    mesh: &mut Mesh,
) {
    use housecraft_model::DoorStyle as D;
    let open = (o.open_percent / 100.0).clamp(0.0, 1.0);
    let swing = DoorSwing::new(g, o, a, b);
    let along = |from: f64, to: f64| if to > from { g.dir } else { -g.dir };
    // A panel lying in the wall between u0..u1, offset `off` toward the left face.
    let panel = |mesh: &mut Mesh, u0: f64, u1: f64, off: f64, z0: f64, z1: f64| {
        let t = g.left * (DOOR_THICKNESS / 2.0);
        let o = g.left * off;
        let (c0, c1) = (g.center_at(u0) + o, g.center_at(u1) + o);
        mesh.prism(
            &ccw(vec![c0 - t, c1 - t, c1 + t, c0 + t]),
            z0,
            z1,
            Surface::Door,
        );
    };
    let angle = open * std::f64::consts::FRAC_PI_2;
    let inset = |u: f64| swing.face(g, u) - swing.side * DOOR_THICKNESS;
    match style {
        D::Doorway => {}
        D::Hinged | D::Shower => {
            let w = (swing.free_u - swing.hinge_u).abs();
            let closed = along(swing.hinge_u, swing.free_u);
            let start = mesh.colors.len();
            swung_leaf(
                inset(swing.hinge_u),
                closed,
                swing.side,
                w,
                angle,
                (bottom, top),
                o.details.glass,
                mesh,
            );
            if style == D::Shower {
                let glass = mesh.look(Surface::Glass);
                mesh.colors[start..].fill(glass.color);
                mesh.pbr[start..].fill(glass.pbr);
                mesh.surfaces[start..].fill(glass.surface);
                mesh.tex[start..].fill(-1.0);
            }
        }
        D::Atrium => {
            let mid = (a + b) / 2.0;
            let fixed = if swing.hinge_u == a { b } else { a };
            let active = (mid - swing.hinge_u).abs();
            swung_leaf(
                inset(swing.hinge_u),
                along(swing.hinge_u, mid),
                swing.side,
                active,
                angle,
                (bottom, top),
                o.details.glass,
                mesh,
            );
            swung_leaf(
                inset(fixed),
                along(fixed, mid),
                swing.side,
                (mid - fixed).abs(),
                0.0,
                (bottom, top),
                o.details.glass,
                mesh,
            );
        }
        D::Fixed => {
            let frame = 50.0;
            let (lo, hi) = (a.min(b), a.max(b));
            panel(mesh, lo, hi, 0.0, bottom, bottom + frame);
            panel(mesh, lo, hi, 0.0, top - frame, top);
            panel(mesh, lo, lo + frame, 0.0, bottom, top);
            panel(mesh, hi - frame, hi, 0.0, bottom, top);
            let t = g.left * 10.0;
            let (c0, c1) = (g.center_at(lo + frame), g.center_at(hi - frame));
            mesh.prism(
                &ccw(vec![c0 - t, c1 - t, c1 + t, c0 + t]),
                bottom + frame,
                top - frame,
                Surface::Glass,
            );
        }
        D::Double => {
            let mid = (a + b) / 2.0;
            for (h, f) in [(a, mid), (b, mid)] {
                swung_leaf(
                    inset(h),
                    along(h, f),
                    swing.side,
                    (f - h).abs(),
                    angle,
                    (bottom, top),
                    o.details.glass,
                    mesh,
                );
            }
        }
        D::Sliding => {
            // Panels in staggered tracks; opening stacks them toward the start.
            let (lo, hi) = (a.min(b), a.max(b));
            let count = (o.details.panels.max(2)) as usize;
            let w = (hi - lo) / count as f64;
            let lap = w * 0.08;
            let tracks = (count.min(3)) as f64;
            for k in 0..count {
                let track = (k % 3) as f64;
                let off = g.half * 0.6 * (track / (tracks - 1.0).max(1.0) - 0.5);
                // Every panel but the first slides over the ones before it.
                let shift = if k == 0 { 0.0 } else { open * w * k as f64 };
                let u0 = (lo + w * k as f64 - lap - shift).max(lo - lap);
                panel(mesh, u0, u0 + w + 2.0 * lap, off, bottom, top);
            }
        }
        D::Pocket => {
            let (lo, hi) = (a.min(b), a.max(b));
            let shift = open * (hi - lo);
            panel(mesh, lo - shift, hi - shift, 0.0, bottom, top);
        }
        D::Bifold => {
            // Leaves folding back against each jamb (many: an accordion door).
            let (lo, hi) = (a.min(b), a.max(b));
            let count = (o.details.panels.max(4) as usize).div_ceil(2) * 2;
            let w = (hi - lo) / count as f64;
            for (hinge, dir) in [(lo, g.dir), (hi, -g.dir)] {
                let d1 = dir * angle.cos() + swing.side * angle.sin();
                let d2 = dir * angle.cos() - swing.side * angle.sin();
                let mut p = inset(hinge);
                let mut leaves = Vec::new();
                for k in 0..count / 2 {
                    let d = if k % 2 == 0 { d1 } else { d2 };
                    leaves.push((p, d));
                    p += d * w;
                }
                for (p, d) in leaves {
                    let n = perp_of(d) * DOOR_THICKNESS;
                    mesh.prism(
                        &ccw(vec![p, p + d * w, p + d * w + n, p + n]),
                        bottom,
                        top,
                        Surface::Door,
                    );
                }
            }
        }
        D::Garage => {
            // Four sections that roll up the jambs and onto overhead tracks.
            let height = top - bottom;
            let sections = 4;
            let h = height / sections as f64;
            let lift = open * height;
            let off = swing.side.dot(g.left) * (g.half - DOOR_THICKNESS / 2.0);
            let gap = 5.0;
            for i in 0..sections {
                let s0 = i as f64 * h + lift;
                // Glass sections: all of them, or the top one.
                let glazed = match o.details.glass {
                    Some(housecraft_model::DoorGlass::Full) => true,
                    Some(housecraft_model::DoorGlass::Upper) => i == sections - 1,
                    None => false,
                };
                if s0 + h <= height + 1e-6 && glazed {
                    panel(mesh, a, b, off, bottom + s0 + gap, bottom + s0 + 2.0 * gap + 60.0);
                    panel(mesh, a, b, off, bottom + s0 + h - 60.0 - 2.0 * gap, bottom + s0 + h - gap);
                    let t = g.left * 8.0;
                    let o2 = g.left * off;
                    let (c0, c1) = (g.center_at(a) + o2, g.center_at(b) + o2);
                    mesh.prism(&ccw(vec![c0 - t, c1 - t, c1 + t, c0 + t]), bottom + s0 + 60.0 + 2.0 * gap, bottom + s0 + h - 60.0 - 2.0 * gap, Surface::Glass);
                } else if s0 + h <= height + 1e-6 {
                    panel(mesh, a, b, off, bottom + s0 + gap, bottom + s0 + h - gap);
                } else {
                    // Horizontal, running into the room under the ceiling.
                    let d = s0 - height;
                    let (f0, f1) = (swing.face(g, a), swing.face(g, b));
                    let side = swing.side;
                    let quad = vec![
                        f0 + side * (d + gap),
                        f1 + side * (d + gap),
                        f1 + side * (d + h - gap),
                        f0 + side * (d + h - gap),
                    ];
                    mesh.prism(&ccw(quad), top, top + DOOR_THICKNESS, Surface::Door);
                }
            }
        }
        D::Barn => {
            let lap = 50.0;
            let dir = if b > a { 1.0 } else { -1.0 };
            let shift = open * (b - a);
            let s = swing.side;
            let (u0, u1) = (a - lap * dir + shift, b + lap * dir + shift);
            let quad = vec![
                swing.face(g, u0) + s * 15.0,
                swing.face(g, u1) + s * 15.0,
                swing.face(g, u1) + s * (15.0 + DOOR_THICKNESS),
                swing.face(g, u0) + s * (15.0 + DOOR_THICKNESS),
            ];
            mesh.prism(&ccw(quad), bottom, top + 30.0, Surface::Door);
            // Track above the opening.
            let t_end = b + (b - a) + lap * dir;
            let track = vec![
                swing.face(g, a - lap * dir) + s * 5.0,
                swing.face(g, t_end) + s * 5.0,
                swing.face(g, t_end) + s * 40.0,
                swing.face(g, a - lap * dir) + s * 40.0,
            ];
            mesh.prism(&ccw(track), top + 30.0, top + 70.0, Surface::Frame);
        }
    }
}

fn perp_of(d: DVec2) -> DVec2 {
    DVec2::new(-d.y, d.x)
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
        OpeningKind::Door(style) => door_mesh(g, o, style, a, b, bottom, top, mesh),
        OpeningKind::Window(_) => {
            let frame = 50.0;
            slab(
                mesh,
                20.0,
                a + frame,
                b - frame,
                bottom + frame,
                top - frame,
                Surface::Glass,
            );
            slab(
                mesh,
                2.0 * g.half,
                a,
                b,
                bottom,
                bottom + frame,
                Surface::Frame,
            );
            slab(mesh, 2.0 * g.half, a, b, top - frame, top, Surface::Frame);
            slab(mesh, 2.0 * g.half, a, a + frame, bottom, top, Surface::Frame);
            slab(mesh, 2.0 * g.half, b - frame, b, bottom, top, Surface::Frame);
            // Mulled units: a double mullion between each pair.
            let units = o.details.mulled.max(1) as usize;
            for k in 1..units {
                let u = a + (b - a) * k as f64 / units as f64;
                slab(mesh, 2.0 * g.half, u - frame, u + frame, bottom, top, Surface::Frame);
            }
            window_details(g, o, z0, (a + frame, b - frame), (bottom + frame, top - frame), mesh);
        }
    }
    // Trim: lintel above and sill below outside, apron under the inside stool.
    let dt = &o.details;
    let trim = mesh.look(Surface::Trim);
    let (lo, hi) = (a.min(b), a.max(b));
    if dt.lintel {
        wall_box(g, (lo - 100.0, hi + 100.0), (top, top + 180.0), (g.half - 1.0, g.half + 25.0), trim, mesh);
    }
    if dt.sill_trim && bottom > z0 + 1.0 {
        wall_box(g, (lo - 50.0, hi + 50.0), (bottom - 45.0, bottom), (g.half - 1.0, g.half + 60.0), trim, mesh);
    }
    if dt.apron && bottom > z0 + 1.0 {
        wall_box(g, (lo - 30.0, hi + 30.0), (bottom - 20.0, bottom), (-g.half - 40.0, -g.half + 1.0), trim, mesh);
        wall_box(g, (lo, hi), (bottom - 110.0, bottom - 20.0), (-g.half - 18.0, -g.half + 1.0), trim, mesh);
    }
}

/// A look of plain colour.
fn color_look(c: housecraft_model::Color, shade: f32) -> Look {
    let [r, g, b, _] = c.0;
    let f = |v: u8| v as f32 / 255.0 * shade;
    Look {
        color: [f(r), f(g), f(b), 1.0],
        pbr: [0.7, 0.0, 0.0, 1.0],
        surface: None,
        tex: None,
    }
}

/// A box on a wall: `u` along it, `z` up, `off` out from its centre line
/// toward the exterior (left) side.
#[allow(clippy::too_many_arguments)]
fn wall_box(g: &WallGeom, u: (f64, f64), z: (f64, f64), off: (f64, f64), look: Look, mesh: &mut Mesh) {
    if (u.1 - u.0).abs() < 0.5 || (off.1 - off.0).abs() < 0.1 {
        return;
    }
    let quad = vec![
        g.center_at(u.0) + g.left * off.0,
        g.center_at(u.1) + g.left * off.0,
        g.center_at(u.1) + g.left * off.1,
        g.center_at(u.0) + g.left * off.1,
    ];
    mesh.prism_look(&ccw(quad), z.0.min(z.1), z.0.max(z.1), look);
}

/// A bar of `width` on the wall plane between two (u, z) points.
fn wall_bar(g: &WallGeom, p: DVec2, q: DVec2, width: f64, off: (f64, f64), look: Look, mesh: &mut Mesh) {
    let d = (q - p).normalize_or_zero();
    if d == DVec2::ZERO {
        return;
    }
    let n = perp_of(d) * width / 2.0;
    let corners = [p + n, q + n, q - n, p - n];
    let at = |c: DVec2, o: f64| (g.center_at(c.x) + g.left * o).extend(c.y);
    let face = |o: f64| corners.map(|c| at(c, o)).to_vec();
    for o in [off.0, off.1] {
        let f = face(o);
        let mut r = f.clone();
        r.reverse();
        mesh.polygon_look(&f, look);
        mesh.polygon_look(&r, look);
    }
    for i in 0..4 {
        let (c0, c1) = (corners[i], corners[(i + 1) % 4]);
        let side = vec![at(c0, off.0), at(c1, off.0), at(c1, off.1), at(c0, off.1)];
        let mut r = side.clone();
        r.reverse();
        mesh.polygon_look(&side, look);
        mesh.polygon_look(&r, look);
    }
}

/// Muntins, shutters and curtains or blinds for a window whose glass spans
/// `gu` along the wall and `gz` up.
fn window_details(g: &WallGeom, o: &Opening, z0: f64, gu: (f64, f64), gz: (f64, f64), mesh: &mut Mesh) {
    let dt = &o.details;
    let frame_look = mesh.look(Surface::Frame);
    if let Some(m) = &dt.muntins {
        let w = m.width.mm();
        let (gw, gh) = (gu.1 - gu.0, gz.1 - gz.0);
        let off = (-15.0, 15.0);
        for i in 1..m.across.max(1) {
            let u = gu.0 + gw * i as f64 / m.across as f64;
            wall_box(g, (u - w / 2.0, u + w / 2.0), gz, off, frame_look, mesh);
        }
        for j in 1..m.up.max(1) {
            let z = gz.0 + gh * j as f64 / m.up as f64;
            wall_box(g, gu, (z - w / 2.0, z + w / 2.0), off, frame_look, mesh);
        }
        for [p, q] in &m.custom {
            let at = |v: &[f64; 2]| DVec2::new(gu.0 + v[0] * gw, gz.0 + v[1] * gh);
            wall_bar(g, at(p), at(q), w, off, frame_look, mesh);
        }
    }
    let (a, b) = (gu.0 - 50.0, gu.1 + 50.0);
    let (bottom, top) = (gz.0 - 50.0, gz.1 + 50.0);
    if let Some(s) = &dt.shutters {
        let sw = s.width.map_or((b - a) / 2.0, |w| w.mm());
        let look = mesh.painted_whole().unwrap_or(color_look(s.color, 1.0));
        let dark = color_look(s.color, 0.55);
        let o0 = g.half + 25.0;
        for (u0, u1) in [(a - sw, a), (b, b + sw)] {
            match s.style {
                ShutterStyle::Louvered => {
                    wall_box(g, (u0, u1), (bottom, top), (o0, o0 + 8.0), dark, mesh);
                    wall_box(g, (u0, u0 + 50.0), (bottom, top), (o0, o0 + 30.0), look, mesh);
                    wall_box(g, (u1 - 50.0, u1), (bottom, top), (o0, o0 + 30.0), look, mesh);
                    wall_box(g, (u0, u1), (bottom, bottom + 80.0), (o0, o0 + 30.0), look, mesh);
                    wall_box(g, (u0, u1), (top - 80.0, top), (o0, o0 + 30.0), look, mesh);
                    let mut z = bottom + 90.0;
                    while z + 25.0 < top - 80.0 {
                        wall_box(g, (u0 + 50.0, u1 - 50.0), (z, z + 25.0), (o0 + 8.0, o0 + 24.0), look, mesh);
                        z += 40.0;
                    }
                }
                ShutterStyle::Panel => {
                    wall_box(g, (u0, u1), (bottom, top), (o0, o0 + 18.0), look, mesh);
                    let mid = (bottom + top) / 2.0;
                    for (p0, p1) in [(bottom + 60.0, mid - 30.0), (mid + 30.0, top - 60.0)] {
                        wall_box(g, (u0 + 60.0, u1 - 60.0), (p0, p1), (o0 + 18.0, o0 + 26.0), look, mesh);
                    }
                }
                ShutterStyle::Board => {
                    let bw = ((u1 - u0) - 16.0) / 3.0;
                    for k in 0..3 {
                        let s0 = u0 + k as f64 * (bw + 8.0);
                        wall_box(g, (s0, s0 + bw), (bottom, top), (o0, o0 + 20.0), look, mesh);
                    }
                    let h = top - bottom;
                    for zc in [bottom + 0.2 * h, top - 0.2 * h] {
                        wall_box(g, (u0 + 20.0, u1 - 20.0), (zc - 45.0, zc + 45.0), (o0 + 20.0, o0 + 40.0), look, mesh);
                    }
                }
            }
        }
    }
    if let Some(t) = &dt.treatment {
        let look = color_look(t.color, 1.0);
        let open = t.open.clamp(0.0, 1.0);
        let room = -g.half;
        match t.kind {
            TreatmentKind::Curtains => {
                let (r0, r1) = (a - 200.0, b + 200.0);
                let rod_off = room - 80.0;
                wall_box(g, (r0 - 30.0, r1 + 30.0), (top + 150.0, top + 180.0), (rod_off - 15.0, rod_off + 15.0), frame_look, mesh);
                let half = (r1 - r0) / 2.0;
                let pw = ((1.0 - open) * half).max(150.0);
                for (u0, u1) in [(r0, r0 + pw), (r1 - pw, r1)] {
                    // Folds: a zigzag strip hanging from the rod to the floor.
                    let folds = ((u1 - u0) / 100.0).ceil().max(1.0) as usize;
                    let mut front = Vec::new();
                    let mut back = Vec::new();
                    for i in 0..=folds {
                        let u = u0 + (u1 - u0) * i as f64 / folds as f64;
                        let o = room - if i % 2 == 0 { 50.0 } else { 110.0 };
                        front.push(g.center_at(u) + g.left * o);
                        back.push(g.center_at(u) + g.left * (o + 8.0));
                    }
                    back.reverse();
                    front.extend(back);
                    mesh.prism_look(&ccw(front), z0 + 20.0, top + 150.0, look);
                }
            }
            TreatmentKind::Blinds => {
                let off = (room + 35.0, room + 60.0);
                let stop = gz.1 - (1.0 - open) * (gz.1 - gz.0);
                if open >= 0.999 {
                    wall_box(g, gu, (gz.1 - 60.0, gz.1), off, look, mesh);
                } else {
                    let mut z = gz.1 - 3.0;
                    while z > stop {
                        wall_box(g, gu, (z - 3.0, z), off, look, mesh);
                        z -= 25.0;
                    }
                    wall_box(g, gu, (stop, stop + 20.0), off, look, mesh);
                }
            }
            TreatmentKind::Shade => {
                let stop = gz.1 - (1.0 - open) * (gz.1 - gz.0);
                wall_box(g, gu, (gz.1 - 50.0, gz.1), (room + 30.0, room + 80.0), look, mesh);
                wall_box(g, gu, (stop, gz.1), (room + 40.0, room + 45.0), look, mesh);
            }
        }
    }
}

/// Axis-aligned bounds of a mesh in metres.
pub fn mesh_bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
    let mut it = mesh.positions.iter().map(|p| Vec3::from_array(*p));
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}
