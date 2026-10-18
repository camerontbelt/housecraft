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
    upper: Option<UpperSpec>,
}

#[derive(Debug, Clone, Copy)]
struct UpperSpec {
    break_height: f64,
    pitch: f64,
    gable: bool,
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
        upper: edge.upper.map(|u| UpperSpec {
            break_height: u.break_height.mm(),
            pitch: u.pitch.clamp(1.0, 85.0),
            gable: u.gable,
        }),
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
///
/// Edges with an `upper` setting change pitch (or turn to a gable) at a break
/// height above the plate: the lower stage is the skeleton clipped at the
/// break, and the upper stage is a second skeleton over the outline offset to
/// that height. That gives mansards, gambrels and Dutch gables.
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
        let break_height = specs
            .iter()
            .filter_map(|s| s.upper.map(|u| u.break_height))
            .filter(|h| *h > 1.0)
            .fold(None, |m: Option<f64>, h| Some(m.map_or(h, |m| m.min(h))));
        let lower = speeds_of(&specs);
        if let Some(h) = break_height
            && let Some(upper_poly) = offset_outline(&poly, &lower, h)
        {
            let stage1 = skeleton(&poly, &lower)
                .into_iter()
                .map(|f| (f.edge, clip_below(&f.polygon, h)))
                .filter(|(_, p)| p.len() >= 3);
            emit(&poly, &specs, &lower, plate, true, stage1, &mut planes);
            let upper_specs: Vec<EdgeSpec> = specs
                .iter()
                .map(|s| match s.upper {
                    Some(u) => EdgeSpec {
                        gable: u.gable,
                        pitch: u.pitch,
                        // The upper stage sits inside the lower roof: no overhangs.
                        overhang: 0.0,
                        upper: None,
                    },
                    None => EdgeSpec { upper: None, ..*s },
                })
                .collect();
            let upper = speeds_of(&upper_specs);
            let stage2 = skeleton(&upper_poly, &upper)
                .into_iter()
                .map(|f| (f.edge, f.polygon));
            emit(
                &upper_poly,
                &upper_specs,
                &upper,
                plate + h,
                false,
                stage2,
                &mut planes,
            );
        } else {
            let faces = skeleton(&poly, &lower)
                .into_iter()
                .map(|f| (f.edge, f.polygon));
            emit(&poly, &specs, &lower, plate, true, faces, &mut planes);
        }
    }
    planes
}

/// Horizontal speed of each edge's roof plane per unit of rise (0 for gables).
fn speeds_of(specs: &[EdgeSpec]) -> Vec<f64> {
    specs
        .iter()
        .map(|s| {
            if s.gable {
                0.0
            } else {
                1.0 / s.pitch.to_radians().tan()
            }
        })
        .collect()
}

/// The outline offset inward to height `h` (each edge moves `speed * h`), or
/// `None` if an edge would have vanished by then (the break is too high).
fn offset_outline(poly: &[DVec2], speeds: &[f64], h: f64) -> Option<Vec<DVec2>> {
    let n = poly.len();
    let dirs: Vec<DVec2> = (0..n)
        .map(|i| (poly[(i + 1) % n] - poly[i]).normalize_or_zero())
        .collect();
    let at = |e: usize| poly[e] + perp(dirs[e]) * speeds[e] * h;
    let corners: Vec<DVec2> = (0..n)
        .map(|i| {
            let p = (i + n - 1) % n;
            line_intersection(at(p), dirs[p], at(i), dirs[i]).unwrap_or(at(i))
        })
        .collect();
    let ok = (0..n).all(|i| (corners[(i + 1) % n] - corners[i]).dot(dirs[i]) > 1.0);
    (ok && crate::math::signed_area(&corners) > 1.0).then_some(corners)
}

/// The part of a skeleton face (heights relative to the plate) at or below `h`.
fn clip_below(face: &[DVec3], h: f64) -> Vec<DVec3> {
    let mut out = Vec::new();
    for i in 0..face.len() {
        let (p, q) = (face[i], face[(i + 1) % face.len()]);
        let (pin, qin) = (p.z <= h + 1e-9, q.z <= h + 1e-9);
        if pin {
            out.push(p);
        }
        if pin != qin {
            let t = (h - p.z) / (q.z - p.z);
            out.push(p.lerp(q, t));
        }
    }
    out
}

/// Turn skeleton faces over `poly` into roof planes based at height `base`,
/// adding eave overhangs (lower stage only) and rake overhangs at gables.
fn emit(
    poly: &[DVec2],
    specs: &[EdgeSpec],
    speeds: &[f64],
    base: f64,
    eaves: bool,
    faces: impl Iterator<Item = (usize, Vec<DVec3>)>,
    planes: &mut Vec<GenPlane>,
) {
    let n = poly.len();
    let dirs: Vec<DVec2> = (0..n)
        .map(|i| (poly[(i + 1) % n] - poly[i]).normalize_or_zero())
        .collect();
    // Corner position at "time" τ (height above the base) between edges p and q.
    let corner = |p: usize, q: usize, tau: f64| -> DVec2 {
        let at = |e: usize| poly[e] + perp(dirs[e]) * speeds[e] * tau;
        line_intersection(at(p), dirs[p], at(q), dirs[q]).unwrap_or(poly[q])
    };
    for (e, face) in faces {
        let spec = specs[e];
        let mut outline: Vec<DVec3> = face
            .iter()
            .map(|p| DVec3::new(p.x, p.y, base + p.z))
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
        if eaves && spec.overhang > 0.0 {
            let tau = -spec.overhang / speeds[e];
            let a2 = corner(prev, e, tau);
            let b2 = corner(e, next, tau);
            for p in outline.iter_mut() {
                let q = p.truncate();
                if q.distance(a) < 1e-3 && (p.z - base).abs() < 1e-3 {
                    *p = DVec3::new(a2.x, a2.y, base + tau);
                } else if q.distance(b) < 1e-3 && (p.z - base).abs() < 1e-3 {
                    *p = DVec3::new(b2.x, b2.y, base + tau);
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
                    upper: None,
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

    /// Give every wall of `house()` an upper pitch above `h`; ends optionally gable above it.
    fn with_upper(
        doc: &mut Document,
        floor: &Floor,
        lower: f64,
        upper: f64,
        h: f64,
        end_gable_above: bool,
        end_gable: bool,
    ) {
        let ids: Vec<Id> = doc.walls_on(floor.id).map(|w| w.id).collect();
        for id in ids {
            let w = doc.get_mut::<Wall>(id).unwrap();
            let end = w.start.x() == w.end.x();
            w.roof.gable = end && end_gable;
            w.roof.pitch = Some(lower);
            w.roof.upper = (!(end && end_gable)).then_some(housecraft_model::UpperRoof {
                break_height: housecraft_units::Length::from_mm(h),
                pitch: if end && end_gable_above { lower } else { upper },
                gable: end && end_gable_above,
            });
        }
    }

    #[test]
    fn mansard_gambrel_and_dutch_gable() {
        let peak = |planes: &[GenPlane]| {
            planes
                .iter()
                .flat_map(|p| p.outline.iter().map(|q| q.z))
                .fold(0.0, f64::max)
        };
        // Mansard: steep 70° up 1.5 m, then 20° to the top.
        let (mut doc, floor) = house(false);
        with_upper(&mut doc, &floor, 70.0, 20.0, 1500.0, false, false);
        let planes = generate(&doc, &floor);
        let plate = plate_height(&doc, &floor);
        assert_eq!(planes.iter().filter(|p| p.pitch == 70.0).count(), 4);
        assert_eq!(planes.iter().filter(|p| p.pitch == 20.0).count(), 4);
        for p in planes.iter().filter(|p| p.pitch == 70.0) {
            let top = p.outline.iter().map(|q| q.z).fold(0.0, f64::max);
            assert!(
                (top - (plate + 1500.0)).abs() < 1e-6,
                "lower planes stop at the break"
            );
        }
        // Remaining half-depth after the steep part, climbed at 20°.
        let half = (6000.0 + 7.1875 * 25.4) / 2.0;
        let rest = half - 1500.0 / 70f64.to_radians().tan();
        assert!((peak(&planes) - (plate + 1500.0 + rest * 20f64.to_radians().tan())).abs() < 1e-6);

        // Gambrel: gable ends; steep then shallow sides.
        let (mut doc, floor) = house(false);
        with_upper(&mut doc, &floor, 60.0, 25.0, 1500.0, false, true);
        let planes = generate(&doc, &floor);
        assert_eq!(planes.iter().filter(|p| !p.gable).count(), 4);
        assert!(planes.iter().filter(|p| p.gable).count() >= 2);

        // Dutch gable: hips below, a small gable on each end above the break.
        let (mut doc, floor) = house(false);
        with_upper(&mut doc, &floor, 45.0, 45.0, 1000.0, true, false);
        let planes = generate(&doc, &floor);
        let gables: Vec<_> = planes.iter().filter(|p| p.gable).collect();
        assert_eq!(gables.len(), 2);
        for g in gables {
            let low = g.outline.iter().map(|q| q.z).fold(f64::MAX, f64::min);
            assert!(
                (low - (plate + 1000.0)).abs() < 1e-6,
                "gable starts at the break"
            );
        }
    }
}
