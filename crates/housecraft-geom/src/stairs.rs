//! Stair geometry: risers from the floor-to-floor height, flights and landings
//! for straight, L and U stairs, plus walk line and rail paths.

use glam::{DVec2, DVec3};
use housecraft_model::{Document, Stair, StairShape, Turn};

use crate::math::{perp, v2};

/// One tread (or landing) as a convex plan quad with its top height.
#[derive(Debug, Clone)]
pub struct Step {
    pub poly: Vec<DVec2>,
    /// Top surface height, absolute mm.
    pub z: f64,
    /// Riser number (1 = first step up).
    pub index: u32,
    pub landing: bool,
    /// The tread's nosing strip projecting past the riser (empty if none).
    pub nosing: Vec<DVec2>,
}

#[derive(Debug, Clone)]
pub struct StairGeom {
    pub risers: u32,
    pub riser_height: f64,
    pub base_z: f64,
    pub steps: Vec<Step>,
    /// Centre walk line from bottom to top, with heights at each point.
    pub walk: Vec<DVec3>,
    /// Handrail paths (at rail height) for each enabled side of each flight.
    pub rails: Vec<Vec<DVec3>>,
    /// Treads per flight, for labels.
    pub flights: Vec<u32>,
    /// Top edges of the side stringers, one per side of each flight.
    pub stringers: Vec<[DVec3; 2]>,
}

impl StairGeom {
    /// Convex pieces covering the stair in plan (for stairwell openings).
    pub fn footprint(&self) -> Vec<Vec<DVec2>> {
        self.steps.iter().map(|s| s.poly.clone()).collect()
    }
}

fn quad(origin: DVec2, dir: DVec2, from: f64, to: f64, left: f64, right: f64) -> Vec<DVec2> {
    let side = perp(dir);
    vec![
        origin + dir * from - side * right,
        origin + dir * to - side * right,
        origin + dir * to + side * left,
        origin + dir * from + side * left,
    ]
}

/// Treads in a stair with `risers` risers (the top riser lands on the floor above).
pub fn tread_count(risers: u32, shape: &StairShape) -> u32 {
    let treads = risers.saturating_sub(1);
    match shape {
        StairShape::Straight => treads,
        // The landing takes one tread's place.
        _ => treads.saturating_sub(1),
    }
}

pub fn geometry(doc: &Document, stair: &Stair) -> StairGeom {
    let rise = doc.stair_rise(stair).mm().max(1.0);
    let risers = ((rise / stair.max_riser.mm()).ceil() as u32).max(1);
    let r = rise / risers as f64;
    let base_z = doc
        .get::<housecraft_model::Floor>(stair.floor)
        .map_or(0.0, |f| f.elevation.mm());
    let d = stair.tread_depth.mm();
    let w = stair.width.mm();
    let half = w / 2.0;
    let dir = DVec2::from_angle(stair.direction.to_radians());
    let origin = v2(stair.start);
    let rail_h = stair.railing_height.mm();
    let inset = 50.0;

    let mut steps = Vec::new();
    let mut rails = Vec::new();
    let mut walk = vec![DVec3::new(origin.x, origin.y, base_z)];
    let mut flights = Vec::new();
    let mut stringers = Vec::new();
    let mut riser = 0u32;
    let overhang = stair.tread_overhang.mm().max(0.0);
    let stringer_inset = 19.0;

    // Lay out one flight of `n` treads starting at `start`, travelling `fdir`.
    let mut flight = |start: DVec2,
                      fdir: DVec2,
                      n: u32,
                      riser: &mut u32,
                      steps: &mut Vec<Step>,
                      walk: &mut Vec<DVec3>| {
        let first = *riser;
        for i in 0..n {
            *riser += 1;
            steps.push(Step {
                poly: quad(start, fdir, i as f64 * d, (i + 1) as f64 * d, half, half),
                z: base_z + *riser as f64 * r,
                index: *riser,
                landing: false,
                nosing: if overhang > 0.0 {
                    quad(
                        start,
                        fdir,
                        i as f64 * d - overhang,
                        i as f64 * d,
                        half,
                        half,
                    )
                } else {
                    Vec::new()
                },
            });
        }
        let end = start + fdir * n as f64 * d;
        walk.push(DVec3::new(end.x, end.y, base_z + *riser as f64 * r));
        if stair.stringers && n > 0 {
            for sign in [1.0, -1.0] {
                let off = perp(fdir) * (half - stringer_inset) * sign;
                let (a, b) = (start + off, end + off);
                stringers.push([
                    DVec3::new(a.x, a.y, base_z + (first + 1) as f64 * r),
                    DVec3::new(b.x, b.y, base_z + *riser as f64 * r),
                ]);
            }
        }
        // Rails rise with the nosings: from the first riser to the last tread.
        for (enabled, sign) in [(stair.railing_left, 1.0), (stair.railing_right, -1.0)] {
            if enabled && n > 0 {
                let off = perp(fdir) * (half - inset) * sign;
                let a = start + off;
                let b = end + off;
                rails.push(vec![
                    DVec3::new(a.x, a.y, base_z + first as f64 * r + r + rail_h),
                    DVec3::new(b.x, b.y, base_z + *riser as f64 * r + rail_h),
                ]);
            }
        }
        end
    };

    let total = tread_count(risers, &stair.shape);
    match stair.shape {
        StairShape::Straight => {
            flight(origin, dir, total, &mut riser, &mut steps, &mut walk);
            flights.push(total);
        }
        StairShape::L { turn, first_treads }
        | StairShape::U {
            turn, first_treads, ..
        } => {
            let k = first_treads.min(total);
            let end = flight(origin, dir, k, &mut riser, &mut steps, &mut walk);
            flights.push(k);
            let side = if turn == Turn::Left { 1.0 } else { -1.0 };
            riser += 1;
            let landing_z = base_z + riser as f64 * r;
            let rest = total - k;
            match stair.shape {
                StairShape::L { .. } => {
                    steps.push(Step {
                        poly: quad(end, dir, 0.0, w, half, half),
                        z: landing_z,
                        index: riser,
                        landing: true,
                        nosing: Vec::new(),
                    });
                    let center = end + dir * half;
                    walk.push(DVec3::new(center.x, center.y, landing_z));
                    let dir2 = perp(dir) * side;
                    let start2 = center + dir2 * half;
                    flight(start2, dir2, rest, &mut riser, &mut steps, &mut walk);
                }
                StairShape::U { gap, .. } => {
                    let g = gap.mm().max(0.0);
                    let (left, right) = if side > 0.0 {
                        (half + g + w, half)
                    } else {
                        (half, half + g + w)
                    };
                    steps.push(Step {
                        poly: quad(end, dir, 0.0, w, left, right),
                        z: landing_z,
                        index: riser,
                        landing: true,
                        nosing: Vec::new(),
                    });
                    let shift = perp(dir) * side * (w + g);
                    // Walk across the landing: up the first flight's axis, over, then back.
                    for p in [end + dir * half, end + dir * half + shift] {
                        walk.push(DVec3::new(p.x, p.y, landing_z));
                    }
                    flight(end + shift, -dir, rest, &mut riser, &mut steps, &mut walk);
                }
                StairShape::Straight => unreachable!(),
            }
            flights.push(rest);
        }
    }
    StairGeom {
        risers,
        riser_height: r,
        base_z,
        steps,
        walk,
        rails,
        flights,
        stringers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::{Document, Id};
    use housecraft_units::{Length, Point2};

    fn stair(shape: StairShape) -> (Document, Stair) {
        let doc = Document::new("t");
        let d = doc.active_defaults().clone();
        let s = Stair {
            id: Id(0),
            floor: doc.floors_by_elevation()[0].id,
            start: Point2::from_mm(0.0, 0.0),
            direction: 90.0,
            width: d.stair.width,
            shape,
            rise: Some(Length::from_inches(108.0)),
            max_riser: d.stair.max_riser,
            tread_depth: d.stair.tread_depth,
            railing_left: true,
            railing_right: true,
            railing_height: d.stair.railing_height,
            tread_overhang: d.stair.tread_overhang,
            stringers: true,
            stairwell: true,
            layer: d.layers.walls,
        };
        (doc, s)
    }

    #[test]
    fn straight_stair_counts_risers() {
        let (doc, s) = stair(StairShape::Straight);
        let g = geometry(&doc, &s);
        // 108" / 7.75" max → 14 risers of 7.714", 13 treads.
        assert_eq!(g.risers, 14);
        assert_eq!(g.steps.len(), 13);
        assert!((g.riser_height - 108.0 * 25.4 / 14.0).abs() < 1e-9);
        let top = g.steps.last().unwrap();
        assert!((top.z - 13.0 * g.riser_height).abs() < 1e-9);
        // Runs north 13 treads x 10".
        let max_y = top.poly.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        assert!((max_y - 130.0 * 25.4).abs() < 1e-6);
        assert_eq!(g.rails.len(), 2);
    }

    #[test]
    fn l_and_u_stairs_have_landings() {
        let (doc, s) = stair(StairShape::L {
            turn: Turn::Right,
            first_treads: 6,
        });
        let g = geometry(&doc, &s);
        assert_eq!(g.steps.iter().filter(|s| s.landing).count(), 1);
        assert_eq!(g.flights, vec![6, 6]);
        // The second flight heads east (a right turn from north).
        let last = g.steps.last().unwrap();
        assert!(last.poly.iter().all(|p| p.x > 0.0));
        let (doc, s) = stair(StairShape::U {
            turn: Turn::Left,
            first_treads: 6,
            gap: Length::ZERO,
        });
        let g = geometry(&doc, &s);
        let last = g.steps.last().unwrap();
        // Second flight runs back south beside the first, on the left (west).
        assert!(last.poly.iter().all(|p| p.x < 0.0));
        assert!(last.poly.iter().map(|p| p.y).fold(f64::MAX, f64::min) < 1.0);
    }
}
