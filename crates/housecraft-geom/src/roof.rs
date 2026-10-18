//! Automatic roofs: a weighted straight skeleton over each building outline,
//! lifted to the wall plate, with eave and rake overhangs.

use glam::{DVec2, DVec3};
use housecraft_model::{DefaultSet, Document, Floor, Id, RoofEdge, Wall};

use crate::math::{line_intersection, perp, simplify_collinear};
use crate::rooms;
use crate::skeleton::skeleton;
use crate::walls::wall_geoms;

/// A generated roof plane (or gable infill), absolute millimetres.
#[derive(Debug, Clone)]
pub struct GenPlane {
    pub outline: Vec<DVec3>,
    pub pitch: f64,
    pub gable: bool,
}

/// Roof settings for one footprint edge, resolved against the defaults.
#[derive(Debug, Clone, Copy)]
struct EdgeSpec {
    gable: bool,
    pitch: f64,
    overhang: f64,
}

/// The wall whose outer face lies along the footprint edge `a`→`b`.
fn edge_wall(doc: &Document, floor: Id, a: DVec2, b: DVec2) -> Option<&Wall> {
    let mid = (a + b) / 2.0;
    let dir = (b - a).normalize_or_zero();
    wall_geoms(doc, floor)
        .into_iter()
        .filter(|g| g.dir.dot(dir).abs() > 0.99)
        .map(|g| {
            let (c, _) = crate::math::closest_on_segment(mid, g.start, g.end);
            ((c.distance(mid) - g.half).abs(), g.id)
        })
        .filter(|(d, _)| *d < 5.0)
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .and_then(|(_, id)| doc.get::<Wall>(id))
}

fn resolve(edge: RoofEdge, d: &DefaultSet) -> EdgeSpec {
    EdgeSpec {
        gable: edge.gable,
        pitch: edge.pitch.unwrap_or(d.roof.pitch).clamp(1.0, 85.0),
        overhang: edge.overhang.map_or(
            if edge.gable {
                d.roof.rake_overhang.mm()
            } else {
                d.roof.overhang.mm()
            },
            |o| o.mm(),
        ),
    }
}

/// Height of the wall plate the roof sits on.
pub fn plate_height(doc: &Document, floor: &Floor) -> f64 {
    let wall_top = doc
        .walls_on(floor.id)
        .filter_map(|w| doc.wall_height(w))
        .map(|h| h.mm())
        .fold(floor.ceiling_height.mm(), f64::max);
    floor.elevation.mm() + wall_top + doc.active_defaults().roof.heel.mm()
}

/// Generate the roof for `floor` from its exterior walls.
pub fn generate(doc: &Document, floor: &Floor) -> Vec<GenPlane> {
    let defaults = doc.active_defaults();
    let plate = plate_height(doc, floor);
    let mut planes = Vec::new();
    for fp in rooms::detect(doc, floor.id).footprints {
        let poly = simplify_collinear(&fp);
        let n = poly.len();
        if n < 3 {
            continue;
        }
        let specs: Vec<EdgeSpec> = (0..n)
            .map(|i| {
                let edge = edge_wall(doc, floor.id, poly[i], poly[(i + 1) % n])
                    .map_or_else(RoofEdge::default, |w| w.roof);
                resolve(edge, defaults)
            })
            .collect();
        let speeds: Vec<f64> = specs
            .iter()
            .map(|s| {
                if s.gable {
                    0.0
                } else {
                    1.0 / s.pitch.to_radians().tan()
                }
            })
            .collect();
        let dirs: Vec<DVec2> = (0..n)
            .map(|i| (poly[(i + 1) % n] - poly[i]).normalize_or_zero())
            .collect();
        // Corner position at "time" τ (height above plate) between edges p and q.
        let corner = |p: usize, q: usize, tau: f64| -> DVec2 {
            let at = |e: usize| poly[e] + perp(dirs[e]) * speeds[e] * tau;
            line_intersection(at(p), dirs[p], at(q), dirs[q]).unwrap_or(poly[q])
        };
        for face in skeleton(&poly, &speeds) {
            let e = face.edge;
            let spec = specs[e];
            let mut outline: Vec<DVec3> = face
                .polygon
                .iter()
                .map(|p| DVec3::new(p.x, p.y, plate + p.z))
                .collect();
            if spec.gable {
                planes.push(GenPlane {
                    outline,
                    pitch: 90.0,
                    gable: true,
                });
                continue;
            }
            let (prev, next) = ((e + n - 1) % n, (e + 1) % n);
            let a = poly[e];
            let b = poly[next];
            // Eave overhang: extend the eave corners down their hip/valley lines.
            if spec.overhang > 0.0 {
                let tau = -spec.overhang / speeds[e];
                let a2 = corner(prev, e, tau);
                let b2 = corner(e, next, tau);
                for p in outline.iter_mut() {
                    let q = p.truncate();
                    if q.distance(a) < 1e-3 && (p.z - plate).abs() < 1e-3 {
                        *p = DVec3::new(a2.x, a2.y, plate + tau);
                    } else if q.distance(b) < 1e-3 && (p.z - plate).abs() < 1e-3 {
                        *p = DVec3::new(b2.x, b2.y, plate + tau);
                    }
                }
            }
            // Rake overhang past neighbouring gables: slide this plane's points that sit
            // on the gable line outward along the eave direction (level, so they stay
            // on the plane).
            for (neighbor, sign) in [(prev, -1.0), (next, 1.0)] {
                if !specs[neighbor].gable || specs[neighbor].overhang <= 0.0 {
                    continue;
                }
                let gn = perp(dirs[neighbor]);
                let gk = gn.dot(poly[neighbor]);
                let along = dirs[e] * sign;
                let rate = along.dot(-gn);
                if rate <= 1e-6 {
                    continue;
                }
                let shift = along * (specs[neighbor].overhang / rate);
                for p in outline.iter_mut() {
                    if (gn.dot(p.truncate()) - gk).abs() < 1e-3 {
                        p.x += shift.x;
                        p.y += shift.y;
                    }
                }
            }
            planes.push(GenPlane {
                outline,
                pitch: spec.pitch,
                gable: false,
            });
        }
    }
    planes
}

/// Height of a sloped roof plane's underside at plan point `p`, if `p` is under it.
pub fn underside_at(outline: &[DVec3], thickness: f64, pitch: f64, p: DVec2) -> Option<f64> {
    let flat: Vec<DVec2> = outline.iter().map(|q| q.truncate()).collect();
    if !crate::math::point_in_polygon(p, &flat) {
        return None;
    }
    let normal = newell(outline).normalize_or_zero();
    if normal.z.abs() < 1e-6 {
        return None;
    }
    let o = outline[0];
    let z = o.z - (normal.x * (p.x - o.x) + normal.y * (p.y - o.y)) / normal.z;
    Some(z - thickness / pitch.to_radians().cos())
}

pub(crate) fn newell(pts: &[DVec3]) -> DVec3 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::{Document, Id, Wall};
    use housecraft_units::Point2;

    fn house(gable_ends: bool) -> (Document, Floor) {
        let mut doc = Document::new("t");
        let d = doc.active_defaults().clone();
        let floor = doc.floors_by_elevation()[0].clone();
        // 10 m x 6 m, drawn clockwise (exterior on the left).
        let pts = [(0.0, 0.0), (0.0, 6000.0), (10000.0, 6000.0), (10000.0, 0.0)];
        for i in 0..4 {
            let (a, b) = (pts[i], pts[(i + 1) % 4]);
            let short = (a.0 - b.0) == 0.0;
            doc.insert(Wall {
                id: Id(0),
                floor: floor.id,
                start: Point2::from_mm(a.0, a.1),
                end: Point2::from_mm(b.0, b.1),
                wall_type: d.exterior_wall_type,
                height: None,
                layer: d.layers.walls,
                footing: None,
                roof: RoofEdge {
                    gable: gable_ends && short,
                    pitch: Some(45.0),
                    overhang: Some(housecraft_units::Length::ZERO),
                },
                renovation: Default::default(),
            });
        }
        (doc, floor)
    }

    #[test]
    fn hip_roof_over_a_box() {
        let (doc, floor) = house(false);
        let planes = generate(&doc, &floor);
        assert_eq!(planes.len(), 4);
        let plate = plate_height(&doc, &floor);
        let half_depth = (6000.0 + 7.1875 * 25.4) / 2.0;
        let peak = planes
            .iter()
            .flat_map(|p| p.outline.iter().map(|q| q.z))
            .fold(0.0, f64::max);
        assert!(
            (peak - (plate + half_depth)).abs() < 1e-6,
            "45° roof peaks at half the outside depth"
        );
    }

    #[test]
    fn gable_ends_make_infill_and_take_rake_overhang() {
        let (mut doc, floor) = house(true);
        let planes = generate(&doc, &floor);
        assert_eq!(planes.iter().filter(|p| p.gable).count(), 2);
        assert_eq!(planes.iter().filter(|p| !p.gable).count(), 2);
        // Add a 12" rake: sloped planes now reach past the gable walls.
        let ids: Vec<Id> = doc.walls_on(floor.id).map(|w| w.id).collect();
        for id in ids {
            let w = doc.get_mut::<Wall>(id).unwrap();
            w.roof.overhang = Some(housecraft_units::Length::from_inches(12.0));
        }
        let planes = generate(&doc, &floor);
        let min_x = planes
            .iter()
            .filter(|p| !p.gable)
            .flat_map(|p| p.outline.iter().map(|q| q.x))
            .fold(f64::INFINITY, f64::min);
        let outer = -(7.1875 * 25.4) / 2.0;
        assert!((min_x - (outer - 304.8)).abs() < 1e-6, "{min_x}");
    }
}
