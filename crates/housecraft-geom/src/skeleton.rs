//! Weighted straight skeleton of a simple polygon, used to generate roofs.
//!
//! Every edge of a counter-clockwise polygon moves inward at its own speed
//! (horizontal distance per unit of height, i.e. `1 / tan(pitch)`); a speed of
//! zero keeps the edge still (a gable). The surface traced by each moving edge
//! is that edge's roof plane, so the roof is read straight off the simulation.
//!
//! The simulation advances event by event: an **edge event** when an edge
//! shrinks to nothing, and a **split event** when a reflex vertex runs into
//! an opposite edge (L- and T-shaped plans). House footprints have tens of
//! edges, so events are found by brute force at each step, which keeps the
//! code simple and robust.

use glam::{DVec2, DVec3};

use crate::math::cross;

/// One input edge's moving line: points `x` with `n·x = k0 + speed·t`.
#[derive(Debug, Clone, Copy)]
struct Line {
    /// Inward unit normal.
    n: DVec2,
    /// Unit direction (counter-clockwise).
    d: DVec2,
    k0: f64,
    speed: f64,
}

impl Line {
    fn k(&self, t: f64) -> f64 {
        self.k0 + self.speed * t
    }
}

#[derive(Debug, Clone, Copy)]
struct Vert {
    /// Index into `nodes` of this vertex's position at the start of the step.
    node: usize,
    /// Edge leaving this vertex (to the next vertex).
    out: usize,
}

/// A face of the skeleton: the roof plane swept by one input edge.
#[derive(Debug, Clone)]
pub struct Face {
    /// Index of the input edge (edge `i` runs from vertex `i` to `i + 1`).
    pub edge: usize,
    /// Boundary in 3D (z = height above the start), counter-clockwise seen from above
    /// for sloped faces.
    pub polygon: Vec<DVec3>,
}

const EPS: f64 = 1e-7;

/// A node position quantised for exact matching when merging slabs.
type Key = (i64, i64, i64);

struct Sim {
    lines: Vec<Line>,
    /// (position, time) of every recorded vertex position.
    nodes: Vec<(DVec2, f64)>,
    /// Slab polygons per face as node indices.
    slabs: Vec<Vec<Vec<usize>>>,
}

impl Sim {
    /// Position at time `t` of the vertex between edges `a` (incoming) and `b` (outgoing).
    fn pos(&self, a: usize, b: usize, t: f64, fallback: (DVec2, f64)) -> DVec2 {
        let (la, lb) = (&self.lines[a], &self.lines[b]);
        let det = cross(la.n, lb.n);
        if det.abs() > 1e-9 {
            let (ka, kb) = (la.k(t), lb.k(t));
            // Solve n_a·x = ka, n_b·x = kb.
            DVec2::new(
                (ka * lb.n.y - kb * la.n.y) / det,
                (la.n.x * kb - lb.n.x * ka) / det,
            )
        } else {
            // Collinear edges: move with the faster normal offset.
            let (p0, t0) = fallback;
            let s = la.speed.max(lb.speed);
            p0 + la.n * s * (t - t0)
        }
    }

    fn vert_pos(&self, ring: &[Vert], i: usize, t: f64) -> DVec2 {
        let m = ring.len();
        let prev = ring[(i + m - 1) % m].out;
        self.pos(prev, ring[i].out, t, self.nodes[ring[i].node])
    }

    fn node(&mut self, p: DVec2, t: f64) -> usize {
        self.nodes.push((p, t));
        self.nodes.len() - 1
    }
}

/// Earliest time > `now` at which the edge leaving vertex `i` collapses.
fn edge_event(sim: &Sim, ring: &[Vert], i: usize, now: f64) -> Option<f64> {
    let m = ring.len();
    let j = (i + 1) % m;
    let d = sim.lines[ring[i].out].d;
    let len = |t: f64| d.dot(sim.vert_pos(ring, j, t) - sim.vert_pos(ring, i, t));
    let (l0, l1) = (len(now), len(now + 1.0));
    let rate = l1 - l0;
    if rate >= -1e-12 {
        return None; // not shrinking
    }
    let t = now - l0 / rate;
    (t > now - EPS).then_some(t.max(now))
}

/// Earliest split of reflex vertex `v` against a non-adjacent edge: (time, edge index in ring).
fn split_event(sim: &Sim, ring: &[Vert], v: usize, now: f64) -> Option<(f64, usize)> {
    let m = ring.len();
    let ein = sim.lines[ring[(v + m - 1) % m].out];
    let eout = sim.lines[ring[v].out];
    if cross(ein.d, eout.d) >= -1e-12 {
        return None; // convex vertex
    }
    let p0 = sim.vert_pos(ring, v, now);
    let vel = sim.vert_pos(ring, v, now + 1.0) - p0;
    let mut best: Option<(f64, usize)> = None;
    for e in 0..m {
        if e == v || (e + 1) % m == v {
            continue;
        }
        let line = sim.lines[ring[e].out];
        // n·(p0 + vel·τ) = k(now + τ)
        let denom = line.n.dot(vel) - line.speed;
        if denom.abs() < 1e-12 {
            continue;
        }
        let tau = (line.k(now) - line.n.dot(p0)) / denom;
        if tau < -EPS {
            continue;
        }
        let t = now + tau.max(0.0);
        let hit = p0 + vel * tau.max(0.0);
        // The hit must fall within the edge's current extent.
        let a = sim.vert_pos(ring, e, t);
        let b = sim.vert_pos(ring, (e + 1) % m, t);
        let along = line.d.dot(hit - a);
        let span = line.d.dot(b - a);
        if along < -1e-6 || along > span + 1e-6 {
            continue;
        }
        if best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, e));
        }
    }
    best
}

/// Compute the weighted straight skeleton faces of a counter-clockwise polygon.
/// `speeds[i]` belongs to the edge from `poly[i]` to `poly[i + 1]`.
pub fn skeleton(poly: &[DVec2], speeds: &[f64]) -> Vec<Face> {
    let n = poly.len();
    assert_eq!(n, speeds.len());
    // One sloping edge (a shed): its plane covers the whole outline, rising
    // with the distance from that edge.
    let moving: Vec<usize> = (0..n).filter(|i| speeds[*i] > 0.0).collect();
    if let [e] = moving[..] {
        let (a, b) = (poly[e], poly[(e + 1) % n]);
        let d = (b - a).normalize_or_zero();
        let inward = DVec2::new(-d.y, d.x);
        let height = |p: DVec2| inward.dot(p - a).max(0.0) / speeds[e];
        let mut faces = vec![Face { edge: e, polygon: poly.iter().map(|p| p.extend(height(*p))).collect() }];
        // The still (gable) edges: walls rising to the plane above them.
        for k in (0..n).filter(|k| *k != e) {
            let (p, q) = (poly[k], poly[(k + 1) % n]);
            let mut polygon = vec![p.extend(0.0), q.extend(0.0)];
            if height(q) > EPS {
                polygon.push(q.extend(height(q)));
            }
            if height(p) > EPS {
                polygon.push(p.extend(height(p)));
            }
            if polygon.len() >= 3 {
                faces.push(Face { edge: k, polygon });
            }
        }
        return faces;
    }
    let mut sim = Sim {
        lines: (0..n)
            .map(|i| {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                let d = (b - a).normalize_or_zero();
                let nrm = DVec2::new(-d.y, d.x);
                Line {
                    n: nrm,
                    d,
                    k0: nrm.dot(a),
                    speed: speeds[i].max(0.0),
                }
            })
            .collect(),
        nodes: Vec::new(),
        slabs: vec![Vec::new(); n],
    };
    let first: Vec<Vert> = (0..n)
        .map(|i| Vert {
            node: sim.node(poly[i], 0.0),
            out: i,
        })
        .collect();
    let mut rings: Vec<Vec<Vert>> = vec![first];
    let mut now = 0.0;

    for _ in 0..(20 * n + 50) {
        rings.retain(|r| r.len() >= 3);
        if rings.is_empty() {
            break;
        }
        // Find the earliest event over all rings.
        enum Ev {
            Edge(usize, usize),
            Split(usize, usize, usize),
        }
        let mut best: Option<(f64, Ev)> = None;
        for (ri, ring) in rings.iter().enumerate() {
            for i in 0..ring.len() {
                if let Some(t) = edge_event(&sim, ring, i, now)
                    && best.as_ref().is_none_or(|(bt, _)| t < *bt - EPS)
                {
                    best = Some((t, Ev::Edge(ri, i)));
                }
                if let Some((t, e)) = split_event(&sim, ring, i, now)
                    && best.as_ref().is_none_or(|(bt, _)| t < *bt - EPS)
                {
                    best = Some((t, Ev::Split(ri, i, e)));
                }
            }
        }
        let Some((t, ev)) = best else {
            break; // nothing left that shrinks (e.g. only gables remain)
        };

        // Advance every ring to `t`, recording one slab per edge.
        for ring in rings.iter_mut() {
            let m = ring.len();
            let new_nodes: Vec<usize> = (0..m)
                .map(|i| {
                    let p = sim.vert_pos(ring, i, t);
                    sim.node(p, t)
                })
                .collect();
            for i in 0..m {
                let j = (i + 1) % m;
                let face = ring[i].out;
                let slab = vec![ring[i].node, ring[j].node, new_nodes[j], new_nodes[i]];
                sim.slabs[face].push(slab);
            }
            for i in 0..m {
                ring[i].node = new_nodes[i];
            }
        }
        now = t;

        match ev {
            Ev::Edge(ri, i) => {
                let ring = &mut rings[ri];
                let m = ring.len();
                let j = (i + 1) % m;
                // Vertex i absorbs j: same position, takes j's outgoing edge.
                let node = ring[i].node;
                let p = sim.nodes[node].0;
                let merged = sim.node(p, t);
                ring[j].node = merged;
                ring.remove(i);
            }
            Ev::Split(ri, v, e) => {
                let ring = rings.swap_remove(ri);
                let m = ring.len();
                let hit = sim.nodes[ring[v].node].0;
                let node = sim.node(hit, t);
                // Ring A: v+1 .. e, then the split point with v's outgoing edge
                // (entered via edge e).
                let mut a = Vec::new();
                let mut k = (v + 1) % m;
                loop {
                    a.push(ring[k]);
                    if k == e {
                        break;
                    }
                    k = (k + 1) % m;
                }
                a.push(Vert {
                    node,
                    out: ring[v].out,
                });
                // Ring B: e+1 .. v-1, then the split point leaving along edge e.
                let mut b = Vec::new();
                let mut k = (e + 1) % m;
                while k != v {
                    b.push(ring[k]);
                    k = (k + 1) % m;
                }
                b.push(Vert {
                    node,
                    out: ring[e].out,
                });
                rings.push(a);
                rings.push(b);
            }
        }
    }

    // Merge each face's slabs into one boundary by cancelling shared edges.
    let point = |i: usize| -> DVec3 {
        let (p, t) = sim.nodes[i];
        DVec3::new(p.x, p.y, t)
    };
    let key = |i: usize| -> (i64, i64, i64) {
        let q = point(i) * 1e4;
        (q.x.round() as i64, q.y.round() as i64, q.z.round() as i64)
    };
    let mut faces = Vec::new();
    for (edge, slabs) in sim.slabs.iter().enumerate() {
        let mut edges: Vec<(Key, Key, usize, usize)> = Vec::new();
        for slab in slabs {
            let m = slab.len();
            for k in 0..m {
                let (a, b) = (slab[k], slab[(k + 1) % m]);
                let (ka, kb) = (key(a), key(b));
                if ka == kb {
                    continue;
                }
                if let Some(pos) = edges.iter().position(|e| e.0 == kb && e.1 == ka) {
                    edges.swap_remove(pos);
                } else {
                    edges.push((ka, kb, a, b));
                }
            }
        }
        if edges.is_empty() {
            continue;
        }
        // Chain the remaining directed edges into the largest loop.
        let mut polygon = vec![point(edges[0].2)];
        let start = edges[0].0;
        let mut cur = edges[0].1;
        let mut used = vec![false; edges.len()];
        used[0] = true;
        while cur != start {
            let Some(k) = (0..edges.len()).find(|&k| !used[k] && edges[k].0 == cur) else {
                break;
            };
            used[k] = true;
            polygon.push(point(edges[k].2));
            cur = edges[k].1;
        }
        // Drop collinear points.
        let cleaned = drop_collinear(&polygon);
        if cleaned.len() >= 3 {
            faces.push(Face {
                edge,
                polygon: cleaned,
            });
        }
    }
    faces
}

fn drop_collinear(poly: &[DVec3]) -> Vec<DVec3> {
    let n = poly.len();
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            (b - a).cross(c - b).length()
                > 1e-6 * (b - a).length().max(1.0) * (c - b).length().max(1.0)
        })
        .map(|i| poly[i])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area_xy(poly: &[DVec3]) -> f64 {
        let n = poly.len();
        (0..n)
            .map(|i| poly[i].x * poly[(i + 1) % n].y - poly[(i + 1) % n].x * poly[i].y)
            .sum::<f64>()
            / 2.0
    }

    fn rect(w: f64, h: f64) -> Vec<DVec2> {
        vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(w, 0.0),
            DVec2::new(w, h),
            DVec2::new(0.0, h),
        ]
    }

    #[test]
    fn hip_roof_on_a_rectangle() {
        let faces = skeleton(&rect(10.0, 6.0), &[1.0; 4]);
        assert_eq!(faces.len(), 4);
        // Faces tile the footprint and peak at half the width.
        let total: f64 = faces.iter().map(|f| area_xy(&f.polygon)).sum();
        assert!((total - 60.0).abs() < 1e-6, "{total}");
        let peak = faces
            .iter()
            .flat_map(|f| f.polygon.iter().map(|p| p.z))
            .fold(0.0, f64::max);
        assert!((peak - 3.0).abs() < 1e-9, "{peak}");
        // Long sides are trapezoids, short sides triangles.
        let mut sizes: Vec<usize> = faces.iter().map(|f| f.polygon.len()).collect();
        sizes.sort();
        assert_eq!(sizes, vec![3, 3, 4, 4]);
    }

    #[test]
    fn gable_roof_when_ends_do_not_move() {
        // Short ends (edges 1 and 3) are gables.
        let faces = skeleton(&rect(10.0, 6.0), &[1.0, 0.0, 1.0, 0.0]);
        let slopes: Vec<_> = faces.iter().filter(|f| f.edge % 2 == 0).collect();
        assert_eq!(slopes.len(), 2);
        for f in &slopes {
            assert_eq!(f.polygon.len(), 4, "{:?}", f.polygon);
            assert!((area_xy(&f.polygon) - 30.0).abs() < 1e-6);
        }
        // Gable faces are vertical triangles reaching the 3.0 ridge.
        let gables: Vec<_> = faces.iter().filter(|f| f.edge % 2 == 1).collect();
        assert_eq!(gables.len(), 2);
        for g in gables {
            assert_eq!(g.polygon.len(), 3);
            assert!(area_xy(&g.polygon).abs() < 1e-6);
            let top = g.polygon.iter().map(|p| p.z).fold(0.0, f64::max);
            assert!((top - 3.0).abs() < 1e-9);
        }
    }

    #[test]
    fn l_shape_splits_and_covers_the_footprint() {
        let l = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(12.0, 0.0),
            DVec2::new(12.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ];
        let faces = skeleton(&l, &[1.0; 6]);
        assert_eq!(faces.len(), 6);
        let total: f64 = faces.iter().map(|f| area_xy(&f.polygon)).sum();
        assert!((total - (12.0 * 4.0 + 4.0 * 6.0)).abs() < 1e-6, "{total}");
        let peak = faces
            .iter()
            .flat_map(|f| f.polygon.iter().map(|p| p.z))
            .fold(0.0, f64::max);
        assert!((peak - 2.0).abs() < 1e-9, "both wings are 4 wide: {peak}");
    }

    #[test]
    fn steeper_edge_pushes_the_ridge_away() {
        // Edge 0 is steeper (slower horizontal speed), so the ridge sits closer to it.
        let faces = skeleton(&rect(10.0, 6.0), &[0.5, 1.0, 1.0, 1.0]);
        let total: f64 = faces.iter().map(|f| area_xy(&f.polygon)).sum();
        assert!((total - 60.0).abs() < 1e-6, "{total}");
        let f0 = faces.iter().find(|f| f.edge == 0).unwrap();
        let max_y = f0.polygon.iter().map(|p| p.y).fold(0.0, f64::max);
        assert!(
            (max_y - 2.0).abs() < 1e-9,
            "ridge at y = 6 * 0.5 / 1.5: {max_y}"
        );
    }

    fn check_cover(poly: &[DVec2], speeds: &[f64]) -> Vec<Face> {
        let faces = skeleton(poly, speeds);
        let n = poly.len();
        let expected: f64 = (0..n)
            .map(|i| cross(poly[i], poly[(i + 1) % n]))
            .sum::<f64>()
            / 2.0;
        let total: f64 = faces.iter().map(|f| area_xy(&f.polygon)).sum();
        assert!(
            (total - expected).abs() < 1e-6 * expected.max(1.0),
            "covered {total} of {expected}"
        );
        assert_eq!(
            faces.iter().filter(|f| speeds[f.edge] > 0.0).count(),
            speeds.iter().filter(|s| **s > 0.0).count()
        );
        faces
    }

    #[test]
    fn t_and_u_shapes() {
        let t = [
            (0.0, 6.0),
            (0.0, 0.0),
            (14.0, 0.0),
            (14.0, 6.0),
            (9.0, 6.0),
            (9.0, 14.0),
            (5.0, 14.0),
            (5.0, 6.0),
        ];
        let mut t: Vec<DVec2> = t.iter().map(|(x, y)| DVec2::new(*x, *y)).collect();
        if crate::math::signed_area(&t) < 0.0 {
            t.reverse();
        }
        check_cover(&t, &vec![1.0; t.len()]);
        let u: Vec<DVec2> = [
            (0.0, 0.0),
            (12.0, 0.0),
            (12.0, 10.0),
            (8.0, 10.0),
            (8.0, 4.0),
            (4.0, 4.0),
            (4.0, 10.0),
            (0.0, 10.0),
        ]
        .iter()
        .map(|(x, y)| DVec2::new(*x, *y))
        .collect();
        let faces = check_cover(&u, &vec![1.0; u.len()]);
        let peak = faces
            .iter()
            .flat_map(|f| f.polygon.iter().map(|p| p.z))
            .fold(0.0, f64::max);
        assert!((peak - 2.0).abs() < 1e-6, "{peak}");
    }

    #[test]
    fn regular_octagon_meets_at_a_point() {
        let oct: Vec<DVec2> = (0..8)
            .map(|i| DVec2::from_angle(i as f64 * std::f64::consts::FRAC_PI_4 + 0.1) * 5.0)
            .collect();
        let faces = check_cover(&oct, &[1.0; 8]);
        assert!(faces.iter().all(|f| f.polygon.len() == 3));
    }
}
