//! Room detection: enclosed regions of the wall-centreline graph, offset to
//! the wall faces.

use glam::DVec2;
use housecraft_model::{Document, Id, WallType};

use crate::math::{
    EPS, closest_on_segment, interior_point, line_intersection, perp, segment_intersection,
    signed_area, v2,
};

/// An enclosed region bounded by walls.
#[derive(Debug, Clone)]
pub struct Region {
    /// Interior (finished-face) polygon, counter-clockwise.
    pub polygon: Vec<DVec2>,
    /// Wall-centreline polygon, counter-clockwise.
    pub centerline: Vec<DVec2>,
    /// Interior area in mm².
    pub area: f64,
    /// A point well inside the region, for labels.
    pub label_point: DVec2,
    /// Walls bounding this region.
    pub walls: Vec<Id>,
}

/// Result of analysing a floor's wall graph.
#[derive(Debug, Clone, Default)]
pub struct FloorRegions {
    pub rooms: Vec<Region>,
    /// Exterior outlines of each connected building footprint, counter-clockwise.
    pub footprints: Vec<Vec<DVec2>>,
}

impl FloorRegions {
    pub fn room_at(&self, p: DVec2) -> Option<usize> {
        self.rooms
            .iter()
            .position(|r| crate::math::point_in_polygon(p, &r.centerline))
    }
}

struct Edge {
    a: usize,
    b: usize,
    wall: Id,
    half: f64,
}

pub fn detect(doc: &Document, floor: Id) -> FloorRegions {
    let segs: Vec<(DVec2, DVec2, Id, f64)> = doc
        .walls_on(floor)
        .map(|w| {
            let half = doc
                .get::<WallType>(w.wall_type)
                .map_or(50.0, |t| t.thickness().mm() / 2.0);
            (v2(w.start), v2(w.end), w.id, half)
        })
        .collect();

    // Split points along each segment: its ends, crossings, and T-junction touches.
    let mut cuts: Vec<Vec<f64>> = vec![vec![0.0, 1.0]; segs.len()];
    for i in 0..segs.len() {
        for j in 0..segs.len() {
            if i == j {
                continue;
            }
            let (a0, a1, ..) = segs[i];
            let (b0, b1, ..) = segs[j];
            if let Some((_, t, _)) = segment_intersection(a0, a1, b0, b1) {
                cuts[i].push(t.clamp(0.0, 1.0));
            }
            for p in [b0, b1] {
                let (c, t) = closest_on_segment(p, a0, a1);
                if c.distance(p) < EPS {
                    cuts[i].push(t);
                }
            }
        }
    }

    let mut nodes: Vec<DVec2> = Vec::new();
    let mut node_of = |p: DVec2| -> usize {
        if let Some(i) = nodes.iter().position(|n| n.distance(p) < EPS) {
            i
        } else {
            nodes.push(p);
            nodes.len() - 1
        }
    };
    let mut edges: Vec<Edge> = Vec::new();
    for (i, (a0, a1, wall, half)) in segs.iter().enumerate() {
        let ts = &mut cuts[i];
        ts.sort_by(f64::total_cmp);
        let ids: Vec<usize> = ts.iter().map(|t| node_of(*a0 + (*a1 - *a0) * *t)).collect();
        for w in ids.windows(2) {
            if w[0] != w[1]
                && !edges
                    .iter()
                    .any(|e| (e.a, e.b) == (w[0], w[1]) || (e.a, e.b) == (w[1], w[0]))
            {
                edges.push(Edge {
                    a: w[0],
                    b: w[1],
                    wall: *wall,
                    half: *half,
                });
            }
        }
    }

    // Prune dangling edges: they never bound a region.
    loop {
        let mut degree = vec![0usize; nodes.len()];
        for e in &edges {
            degree[e.a] += 1;
            degree[e.b] += 1;
        }
        let before = edges.len();
        edges.retain(|e| degree[e.a] > 1 && degree[e.b] > 1);
        if edges.len() == before {
            break;
        }
    }

    // Half-edges 2k (a→b) and 2k+1 (b→a); outgoing lists sorted by angle.
    let he_from = |h: usize| {
        if h.is_multiple_of(2) {
            edges[h / 2].a
        } else {
            edges[h / 2].b
        }
    };
    let he_to = |h: usize| {
        if h.is_multiple_of(2) {
            edges[h / 2].b
        } else {
            edges[h / 2].a
        }
    };
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for h in 0..edges.len() * 2 {
        outgoing[he_from(h)].push(h);
    }
    for (n, list) in outgoing.iter_mut().enumerate() {
        list.sort_by(|&x, &y| {
            let ax = (nodes[he_to(x)] - nodes[n]).to_angle();
            let ay = (nodes[he_to(y)] - nodes[n]).to_angle();
            ax.total_cmp(&ay)
        });
    }

    let mut used = vec![false; edges.len() * 2];
    let mut result = FloorRegions::default();
    for start in 0..edges.len() * 2 {
        if used[start] {
            continue;
        }
        let mut face = Vec::new();
        let mut h = start;
        loop {
            used[h] = true;
            face.push(h);
            // Next half-edge: at the head, turn to the next edge clockwise from the twin.
            let v = he_to(h);
            let twin = h ^ 1;
            let list = &outgoing[v];
            let k = list
                .iter()
                .position(|&x| x == twin)
                .expect("twin is outgoing");
            h = list[(k + list.len() - 1) % list.len()];
            if h == start || face.len() > edges.len() * 2 {
                break;
            }
        }
        let centerline: Vec<DVec2> = face.iter().map(|&h| nodes[he_from(h)]).collect();
        if centerline.len() < 3 {
            continue;
        }
        let offset = offset_left(
            &centerline,
            &face.iter().map(|&h| edges[h / 2].half).collect::<Vec<_>>(),
        );
        let area = signed_area(&centerline);
        if area > 1.0 {
            let mut walls: Vec<Id> = face.iter().map(|&h| edges[h / 2].wall).collect();
            walls.sort();
            walls.dedup();
            let interior_area = signed_area(&offset).max(0.0);
            result.rooms.push(Region {
                label_point: interior_point(&offset),
                polygon: offset,
                centerline,
                area: interior_area,
                walls,
            });
        } else if area < -1.0 {
            // The outer face of a component; its left side is the exterior.
            let mut outline = offset;
            outline.reverse();
            result.footprints.push(outline);
        }
    }
    result
}

/// Offset each edge of a closed polygon to its left by `half[i]` and rejoin
/// consecutive edges at their intersections.
fn offset_left(poly: &[DVec2], half: &[f64]) -> Vec<DVec2> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = (i + n - 1) % n;
        let (p0, p1) = (poly[prev], poly[i]);
        let p2 = poly[(i + 1) % n];
        let d0 = (p1 - p0).normalize_or_zero();
        let d1 = (p2 - p1).normalize_or_zero();
        let a = p1 + perp(d0) * half[prev];
        let b = p1 + perp(d1) * half[i];
        let corner = line_intersection(a, d0, b, d1)
            .filter(|c| c.distance(p1) < 6.0 * half[prev].max(half[i]))
            .unwrap_or(b);
        out.push(corner);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::Wall;
    use housecraft_units::Point2;

    fn wall(doc: &mut Document, a: (f64, f64), b: (f64, f64)) {
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
        });
    }

    #[test]
    fn box_with_partition_makes_two_rooms() {
        let mut doc = Document::new("t");
        wall(&mut doc, (0.0, 0.0), (6000.0, 0.0));
        wall(&mut doc, (6000.0, 0.0), (6000.0, 4000.0));
        wall(&mut doc, (6000.0, 4000.0), (0.0, 4000.0));
        wall(&mut doc, (0.0, 4000.0), (0.0, 0.0));
        // T-junction partition, plus a dangling stub that must be ignored.
        wall(&mut doc, (2000.0, 0.0), (2000.0, 4000.0));
        wall(&mut doc, (4000.0, 0.0), (4000.0, 1000.0));
        let floor = doc.floors_by_elevation()[0].id;
        let r = detect(&doc, floor);
        assert_eq!(r.rooms.len(), 2, "{:?}", r.rooms);
        assert_eq!(r.footprints.len(), 1);
        let h = 4.5 * 25.4 / 2.0;
        let mut areas: Vec<f64> = r.rooms.iter().map(|x| x.area).collect();
        areas.sort_by(f64::total_cmp);
        let small = (2000.0 - 2.0 * h) * (4000.0 - 2.0 * h);
        let big = (4000.0 - 2.0 * h) * (4000.0 - 2.0 * h);
        assert!((areas[0] - small).abs() < 1.0, "{areas:?} vs {small}");
        assert!((areas[1] - big).abs() < 1.0, "{areas:?} vs {big}");
        let fp = &r.footprints[0];
        assert!((signed_area(fp) - (6000.0 + 2.0 * h) * (4000.0 + 2.0 * h)).abs() < 1.0);
        for room in &r.rooms {
            assert!(crate::math::point_in_polygon(
                room.label_point,
                &room.polygon
            ));
        }
    }

    #[test]
    fn open_walls_make_no_room() {
        let mut doc = Document::new("t");
        wall(&mut doc, (0.0, 0.0), (6000.0, 0.0));
        wall(&mut doc, (6000.0, 0.0), (6000.0, 4000.0));
        wall(&mut doc, (6000.0, 4000.0), (0.0, 4000.0));
        let floor = doc.floors_by_elevation()[0].id;
        let r = detect(&doc, floor);
        assert!(r.rooms.is_empty());
    }
}
