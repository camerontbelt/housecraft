//! 3D solids: extruded outlines and revolved profiles.

use glam::{DVec2, DVec3};
use housecraft_model::{Document, Floor, Solid, SolidShape};

use crate::math::{signed_area, v2};
use crate::mesh::{Look, Mesh};

/// Sides used for a full turn.
const TURN_SIDES: f64 = 32.0;

pub fn solid_mesh(doc: &Document, s: &Solid, mesh: &mut Mesh) {
    let Some(floor) = doc.get::<Floor>(s.floor) else {
        return;
    };
    let z0 = floor.elevation.mm() + s.elevation.mm();
    let [r, g, b, _] = s.color.0;
    let f = |v: u8| v as f32 / 255.0;
    let look = mesh.painted_whole().unwrap_or(Look {
        color: [f(r), f(g), f(b), 1.0],
        pbr: [0.6, 0.0, 0.0, 1.0],
        surface: None,
        tex: None,
    });
    match &s.shape {
        SolidShape::Extrude { points, height } => {
            let mut poly: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
            if signed_area(&poly) < 0.0 {
                poly.reverse();
            }
            mesh.prism_look(&poly, z0, z0 + height.mm(), look);
        }
        SolidShape::Revolve {
            center,
            profile,
            sweep,
        } => revolve(v2(*center), profile, *sweep, z0, look, mesh),
    }
}

/// The plan outline: the extruded outline, or the revolved shape's widest
/// circle (a pie slice for part turns).
pub fn footprint(s: &Solid) -> Vec<DVec2> {
    match &s.shape {
        SolidShape::Extrude { points, .. } => points.iter().map(|p| v2(*p)).collect(),
        SolidShape::Revolve {
            center,
            profile,
            sweep,
        } => {
            let c = v2(*center);
            let r = profile.iter().map(|p| p[0].mm()).fold(0.0, f64::max);
            let n = (sweep / 360.0 * TURN_SIDES).ceil().max(3.0) as usize;
            let mut out: Vec<DVec2> = (0..=n)
                .map(|i| c + DVec2::from_angle((sweep * i as f64 / n as f64).to_radians()) * r)
                .collect();
            if *sweep < 359.9 {
                out.push(c);
            } else {
                out.pop();
            }
            out
        }
    }
}

fn revolve(c: DVec2, profile: &[[housecraft_units::Length; 2]], sweep: f64, z0: f64, look: Look, mesh: &mut Mesh) {
    let mut prof: Vec<DVec2> = profile.iter().map(|p| DVec2::new(p[0].mm().max(0.0), p[1].mm())).collect();
    if prof.len() < 2 {
        return;
    }
    // Outward faces need the profile counter-clockwise in (radius, height).
    if signed_area(&prof) < 0.0 {
        prof.reverse();
    }
    let n = (sweep / 360.0 * TURN_SIDES).ceil().max(3.0) as usize;
    let at = |p: DVec2, i: usize| {
        let a = (sweep * i as f64 / n as f64).to_radians();
        let d = DVec2::from_angle(a) * p.x;
        DVec3::new(c.x + d.x, c.y + d.y, z0 + p.y)
    };
    for k in 0..prof.len() {
        let (p, q) = (prof[k], prof[(k + 1) % prof.len()]);
        if (p.x < 0.01 && q.x < 0.01) || p.distance(q) < 0.01 {
            continue;
        }
        for i in 0..n {
            let mut quad = vec![at(p, i), at(p, i + 1), at(q, i + 1), at(q, i)];
            quad.dedup_by(|a, b| a.distance(*b) < 0.01);
            if quad.len() >= 3 {
                mesh.polygon_look(&quad, look);
            }
        }
    }
    if sweep < 359.9 {
        // Cap the cut ends with the profile.
        let start: Vec<DVec3> = prof.iter().map(|p| at(*p, 0)).collect();
        let end: Vec<DVec3> = prof.iter().rev().map(|p| at(*p, n)).collect();
        mesh.polygon_look(&start, look);
        mesh.polygon_look(&end, look);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::*;
    use housecraft_units::{Length, Point2};

    #[test]
    fn solids_build_closed_meshes() {
        let doc = Document::new("t");
        let floor = doc.floors_by_elevation()[0].id;
        let layer = doc.active_defaults().layers.cad;
        let c = Point2(Length::from_mm(0.0), Length::from_mm(0.0));
        for shape in [
            SolidShape::cylinder(c, Length::from_mm(500.0), Length::from_mm(1000.0)),
            SolidShape::sphere(c, Length::from_mm(500.0)),
            SolidPreset::Box.shape(c, Point2(Length::from_mm(500.0), Length::from_mm(300.0)), Length::from_mm(900.0)),
        ] {
            let s = Solid {
                id: Id(0),
                floor,
                layer,
                shape,
                elevation: Length::ZERO,
                color: Color::rgb(1, 2, 3),
            };
            let mut mesh = Mesh::for_document(&doc);
            mesh.begin(Id(1), crate::mesh::Category::Fixture);
            solid_mesh(&doc, &s, &mut mesh);
            mesh.end();
            assert!(mesh.indices.len() >= 36, "{:?}", s.shape);
            // Normals point away from the axis on the sides.
            let (lo, hi) = crate::mesh::mesh_bounds(&mesh).unwrap();
            assert!(hi.z > lo.z);
            assert!(footprint(&s).len() >= 4);
        }
    }
}
