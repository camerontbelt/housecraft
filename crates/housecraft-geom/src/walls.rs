//! Wall outlines: mitred joins at shared endpoints, clean T-junctions, and
//! pieces split around openings.

use glam::DVec2;
use housecraft_model::{Document, Id, Opening, Wall, WallType};

use crate::math::{EPS, closest_on_segment, line_intersection, perp, signed_area, v2};

/// A wall's footprint in plan.
#[derive(Debug, Clone)]
pub struct WallGeom {
    pub id: Id,
    pub start: DVec2,
    pub end: DVec2,
    /// Unit vector from start to end.
    pub dir: DVec2,
    /// Unit left normal (the exterior side for exterior→interior layer order).
    pub left: DVec2,
    pub half: f64,
    pub length: f64,
    /// Outline corners after joins: start-left, end-left, end-right, start-right.
    pub corners: [DVec2; 4],
    /// Whether the start / end is joined to another wall (no end cap drawn).
    pub joined: [bool; 2],
}

impl WallGeom {
    /// The full outline polygon, counter-clockwise.
    pub fn outline(&self) -> Vec<DVec2> {
        ccw(self.corners.to_vec())
    }

    /// Point on the left face at distance `u` along the centreline.
    pub fn left_at(&self, u: f64) -> DVec2 {
        self.start + self.dir * u + self.left * self.half
    }

    pub fn right_at(&self, u: f64) -> DVec2 {
        self.start + self.dir * u - self.left * self.half
    }

    pub fn center_at(&self, u: f64) -> DVec2 {
        self.start + self.dir * u
    }

    fn u_of(&self, p: DVec2) -> f64 {
        (p - self.start).dot(self.dir)
    }

    /// Solid pieces of the wall, split around openings (counter-clockwise polygons).
    pub fn pieces(&self, openings: &[&Opening]) -> Vec<Vec<DVec2>> {
        self.pieces_with(openings, &[])
    }

    /// [`WallGeom::pieces`], also leaving gaps at `extra` spans (niches,
    /// openings of a wall built back to back with this one).
    pub fn pieces_with(&self, openings: &[&Opening], extra: &[(f64, f64)]) -> Vec<Vec<DVec2>> {
        let spans = merge_spans(opening_spans(self, openings), extra);
        let [sl, el, er, sr] = self.corners;
        let mut pieces = Vec::new();
        let mut left_start = sl;
        let mut right_start = sr;
        for (a, b) in spans {
            let poly = vec![left_start, self.left_at(a), self.right_at(a), right_start];
            if self.u_of(self.left_at(a)) - self.u_of(left_start) > 0.01
                || self.u_of(self.right_at(a)) - self.u_of(right_start) > 0.01
            {
                pieces.push(ccw(poly));
            }
            left_start = self.left_at(b);
            right_start = self.right_at(b);
        }
        let last = vec![left_start, el, er, right_start];
        if self.u_of(el) - self.u_of(left_start) > 0.01
            || self.u_of(er) - self.u_of(right_start) > 0.01
        {
            pieces.push(ccw(last));
        }
        pieces
    }
}

impl WallGeom {
    /// Line segments that outline the wall in plan: both faces broken at
    /// openings, jambs at each opening, and caps on ends not joined to a wall.
    pub fn outline_segments(&self, openings: &[&Opening]) -> Vec<[DVec2; 2]> {
        self.outline_segments_with(openings, &[])
    }

    /// [`WallGeom::outline_segments`] with gaps at `extra` spans too.
    pub fn outline_segments_with(&self, openings: &[&Opening], extra: &[(f64, f64)]) -> Vec<[DVec2; 2]> {
        let [sl, el, er, sr] = self.corners;
        let spans = merge_spans(opening_spans(self, openings), extra);
        let mut segs = Vec::new();
        let mut l0 = sl;
        let mut r0 = sr;
        for (a, b) in &spans {
            segs.push([l0, self.left_at(*a)]);
            segs.push([r0, self.right_at(*a)]);
            segs.push([self.left_at(*a), self.right_at(*a)]);
            segs.push([self.left_at(*b), self.right_at(*b)]);
            l0 = self.left_at(*b);
            r0 = self.right_at(*b);
        }
        segs.push([l0, el]);
        segs.push([r0, er]);
        if !self.joined[0] {
            segs.push([sl, sr]);
        }
        if !self.joined[1] {
            segs.push([el, er]);
        }
        segs.retain(|[a, b]| a.distance(*b) > 0.01);
        segs
    }
}

/// Sorted, merged `(start, end)` distances along the wall covered by openings.
pub fn opening_spans(wall: &WallGeom, openings: &[&Opening]) -> Vec<(f64, f64)> {
    let mut spans: Vec<(f64, f64)> = openings
        .iter()
        .map(|o| {
            let half = o.width.mm() / 2.0;
            (
                (o.position.mm() - half).max(0.0),
                (o.position.mm() + half).min(wall.length),
            )
        })
        .filter(|(a, b)| b > a)
        .collect();
    spans.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for s in spans {
        match merged.last_mut() {
            Some(last) if s.0 <= last.1 => last.1 = last.1.max(s.1),
            _ => merged.push(s),
        }
    }
    merged
}

/// `spans` plus `extra`, sorted and merged.
fn merge_spans(mut spans: Vec<(f64, f64)>, extra: &[(f64, f64)]) -> Vec<(f64, f64)> {
    spans.extend(extra.iter().copied().filter(|(a, b)| b > a));
    spans.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for s in spans {
        match merged.last_mut() {
            Some(last) if s.0 <= last.1 => last.1 = last.1.max(s.1),
            _ => merged.push(s),
        }
    }
    merged
}

/// A door or window of a wall built back to back with `g` (parallel, at
/// most a foot apart), as a hole through `g` too: (start, end, sill, height).
pub fn shared_openings(doc: &Document, geoms: &[WallGeom], g: &WallGeom) -> Vec<(f64, f64, f64, f64)> {
    let mut out = Vec::new();
    for h in geoms {
        if h.id == g.id || h.dir.dot(g.dir).abs() < 0.999 {
            continue;
        }
        let gap = (h.start - g.start).dot(g.left).abs() - g.half - h.half;
        if !(-1.0..=305.0).contains(&gap) {
            continue;
        }
        for o in doc.openings_in(h.id) {
            let c = h.start + h.dir * o.position.mm();
            let u = (c - g.start).dot(g.dir);
            let half = o.width.mm() / 2.0;
            let (a, b) = ((u - half).max(0.0), (u + half).min(g.length));
            if b - a > 1.0 {
                out.push((a, b, o.sill.mm(), o.height.mm()));
            }
        }
    }
    out
}

/// A polygon wound counter-clockwise.
pub fn ccw_pub(poly: Vec<DVec2>) -> Vec<DVec2> {
    ccw(poly)
}

fn ccw(mut poly: Vec<DVec2>) -> Vec<DVec2> {
    if signed_area(&poly) < 0.0 {
        poly.reverse();
    }
    poly
}

struct Incident {
    wall: usize,
    /// Outgoing direction from the node.
    dir: DVec2,
    half: f64,
    /// `Some(true)` = this is the wall's start, `Some(false)` = its end,
    /// `None` = the node is in the wall's middle (a T or X junction).
    at_start: Option<bool>,
}

/// Outline geometry for every wall on a floor.
pub fn wall_geoms(doc: &Document, floor: Id) -> Vec<WallGeom> {
    let walls: Vec<&Wall> = doc.walls_on(floor).collect();
    let thickness = |w: &Wall| {
        doc.get::<WallType>(w.wall_type)
            .map_or(100.0, |t| t.thickness().mm())
    };
    let mut geoms: Vec<WallGeom> = walls
        .iter()
        .map(|w| {
            let (s, e) = (v2(w.start), v2(w.end));
            let length = s.distance(e);
            let dir = if length > 0.0 {
                (e - s) / length
            } else {
                DVec2::X
            };
            let left = perp(dir);
            let half = thickness(w) / 2.0;
            WallGeom {
                id: w.id,
                start: s,
                end: e,
                dir,
                left,
                half,
                length,
                corners: [
                    s + left * half,
                    e + left * half,
                    e - left * half,
                    s - left * half,
                ],
                joined: [false, false],
            }
        })
        .collect();

    // Nodes are wall endpoints, merged when within EPS.
    let mut nodes: Vec<DVec2> = Vec::new();
    let mut node_of = |p: DVec2| -> usize {
        if let Some(i) = nodes.iter().position(|n| n.distance(p) < EPS) {
            i
        } else {
            nodes.push(p);
            nodes.len() - 1
        }
    };
    let ends: Vec<(usize, usize)> = geoms
        .iter()
        .map(|g| (node_of(g.start), node_of(g.end)))
        .collect();

    for (n, &node) in nodes.iter().enumerate() {
        let mut incidents: Vec<Incident> = Vec::new();
        for (i, g) in geoms.iter().enumerate() {
            if ends[i].0 == n {
                incidents.push(Incident {
                    wall: i,
                    dir: g.dir,
                    half: g.half,
                    at_start: Some(true),
                });
            } else if ends[i].1 == n {
                incidents.push(Incident {
                    wall: i,
                    dir: -g.dir,
                    half: g.half,
                    at_start: Some(false),
                });
            } else {
                let (c, t) = closest_on_segment(node, g.start, g.end);
                if c.distance(node) < EPS && t > 0.0 && t < 1.0 {
                    for dir in [g.dir, -g.dir] {
                        incidents.push(Incident {
                            wall: i,
                            dir,
                            half: g.half,
                            at_start: None,
                        });
                    }
                }
            }
        }
        if incidents.iter().all(|i| i.at_start.is_none()) || incidents.len() < 2 {
            continue;
        }
        incidents.sort_by(|a, b| a.dir.to_angle().total_cmp(&b.dir.to_angle()));
        let count = incidents.len();
        // corner[i] is between incident i (its left side) and incident i+1 (its right side).
        let corners: Vec<Option<DVec2>> = (0..count)
            .map(|i| {
                let a = &incidents[i];
                let b = &incidents[(i + 1) % count];
                let pa = node + perp(a.dir) * a.half;
                let pb = node - perp(b.dir) * b.half;
                let p = line_intersection(pa, a.dir, pb, b.dir)?;
                // Reject near-parallel or wildly acute mitres.
                let limit = 6.0 * a.half.max(b.half);
                (p.distance(node) <= limit).then_some(p)
            })
            .collect();
        for i in 0..count {
            let inc = &incidents[i];
            let Some(at_start) = inc.at_start else {
                continue;
            };
            let square_left = node + perp(inc.dir) * inc.half;
            let square_right = node - perp(inc.dir) * inc.half;
            let left = corners[i].unwrap_or(square_left);
            let right = corners[(i + count - 1) % count].unwrap_or(square_right);
            let g = &mut geoms[inc.wall];
            if at_start {
                g.joined[0] = true;
                g.corners[0] = left;
                g.corners[3] = right;
            } else {
                // Seen from the end, the incident's left is the wall's right.
                g.joined[1] = true;
                g.corners[2] = left;
                g.corners[1] = right;
            }
        }
    }
    geoms
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::{Document, Wall};
    use housecraft_units::Point2;

    fn add_wall(doc: &mut Document, a: (f64, f64), b: (f64, f64)) -> Id {
        let d = doc.active_defaults().clone();
        let floor = doc.floors_by_elevation()[0].id;
        doc.insert(Wall {
            id: Id(0),
            floor,
            start: Point2::from_mm(a.0, a.1),
            end: Point2::from_mm(b.0, b.1),
            wall_type: d.interior_wall_type,
            height: None,
            layer: d.layers.walls,
            footing: None,
            roof: Default::default(),
            renovation: Default::default(),
            shape: Default::default(),
        })
    }

    fn near(a: DVec2, x: f64, y: f64) -> bool {
        a.distance(DVec2::new(x, y)) < 1e-6
    }

    #[test]
    fn corner_is_mitred() {
        let mut doc = Document::new("t");
        let a = add_wall(&mut doc, (0.0, 0.0), (1000.0, 0.0));
        add_wall(&mut doc, (1000.0, 0.0), (1000.0, 1000.0));
        let floor = doc.floors_by_elevation()[0].id;
        let geoms = wall_geoms(&doc, floor);
        let g = geoms.iter().find(|g| g.id == a).unwrap();
        let h = g.half;
        // Inside corner (left of the first wall) pulls in; outside corner extends.
        assert!(near(g.corners[1], 1000.0 - h, h), "{:?}", g.corners);
        assert!(near(g.corners[2], 1000.0 + h, -h), "{:?}", g.corners);
    }

    #[test]
    fn t_junction_stops_at_face() {
        let mut doc = Document::new("t");
        add_wall(&mut doc, (0.0, 0.0), (2000.0, 0.0));
        let stem = add_wall(&mut doc, (1000.0, 0.0), (1000.0, 1000.0));
        let floor = doc.floors_by_elevation()[0].id;
        let geoms = wall_geoms(&doc, floor);
        let g = geoms.iter().find(|g| g.id == stem).unwrap();
        let h = g.half;
        assert!(near(g.corners[0], 1000.0 - h, h), "{:?}", g.corners);
        assert!(near(g.corners[3], 1000.0 + h, h), "{:?}", g.corners);
    }

    #[test]
    fn free_ends_are_square() {
        let mut doc = Document::new("t");
        let a = add_wall(&mut doc, (0.0, 0.0), (1000.0, 0.0));
        let floor = doc.floors_by_elevation()[0].id;
        let g = &wall_geoms(&doc, floor)[0];
        assert_eq!(g.id, a);
        assert!(near(g.corners[0], 0.0, g.half));
        assert!((signed_area(&g.outline()) - 1000.0 * 2.0 * g.half).abs() < 1e-6);
    }
}

/// How a door leaf hangs in its opening `a..b` (distances along the wall).
#[derive(Debug, Clone, Copy)]
pub struct DoorSwing {
    /// Unit normal pointing to the side the door swings toward.
    pub side: DVec2,
    pub hinge_u: f64,
    pub free_u: f64,
}

impl DoorSwing {
    pub fn new(g: &WallGeom, o: &Opening, a: f64, b: f64) -> Self {
        let side = if o.swing_in { -g.left } else { g.left };
        // Seen from the swing side, "left" is perp(-side).
        let viewer_left_is_plus_u = perp(-side).dot(g.dir) > 0.0;
        let hinge_at_b = matches!(o.hinge, housecraft_model::Hinge::Left) == viewer_left_is_plus_u;
        let (hinge_u, free_u) = if hinge_at_b { (b, a) } else { (a, b) };
        DoorSwing {
            side,
            hinge_u,
            free_u,
        }
    }

    /// The point on the swing-side wall face at distance `u`.
    pub fn face(&self, g: &WallGeom, u: f64) -> DVec2 {
        g.center_at(u) + self.side * g.half
    }
}
