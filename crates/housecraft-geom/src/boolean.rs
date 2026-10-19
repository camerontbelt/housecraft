//! Boolean operations on simple plan polygons (Greiner–Hormann clipping),
//! for joining, intersecting and cutting closed CAD shapes.

use glam::DVec2;

use crate::math::{cross, point_in_polygon, signed_area};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Intersect,
    /// The first polygon minus the second.
    Subtract,
}

/// A vertex in one polygon's ring: an original corner or a crossing.
#[derive(Clone, Copy)]
struct Node {
    p: DVec2,
    /// The crossing's index, for crossings.
    cross: Option<usize>,
    entry: bool,
}

fn ccw(mut p: Vec<DVec2>) -> Vec<DVec2> {
    if signed_area(&p) < 0.0 {
        p.reverse();
    }
    p
}

/// `a op b`; each result is counter-clockwise. A hole cut fully inside `a`
/// is joined to the outline by a hairline slit so it stays one outline.
pub fn boolean(a: &[DVec2], b: &[DVec2], op: BoolOp) -> Vec<Vec<DVec2>> {
    if a.len() < 3 || b.len() < 3 {
        return match op {
            BoolOp::Intersect => Vec::new(),
            _ => vec![ccw(a.to_vec())],
        };
    }
    let a = ccw(a.to_vec());
    let mut b = ccw(b.to_vec());
    // Crossings at corners or along shared edges confuse the walk; nudge
    // the second polygon a hair and try again.
    for attempt in 0..8 {
        if let Some(out) = clip(&a, &b, op) {
            return out
                .into_iter()
                .filter(|p| p.len() >= 3 && signed_area(p).abs() > 1e-3)
                .map(ccw)
                .collect();
        }
        let nudge = DVec2::new(1.0, 0.618) * 1e-4 * (attempt + 1) as f64;
        b.iter_mut().for_each(|p| *p += nudge);
    }
    vec![a]
}

/// Union of any number of polygons.
pub fn union_all(polys: &[Vec<DVec2>]) -> Vec<Vec<DVec2>> {
    let mut out: Vec<Vec<DVec2>> = Vec::new();
    for p in polys {
        let mut merged = ccw(p.clone());
        // Merge with any result it overlaps until none do.
        while let Some(i) = out
            .iter()
            .position(|r| boolean(r, &merged, BoolOp::Union).len() == 1)
        {
            let r = out.remove(i);
            merged = boolean(&r, &merged, BoolOp::Union).remove(0);
        }
        out.push(merged);
    }
    out
}

/// A crossing: (edge of a, t along it, edge of b, u along it, point).
type Crossing = (usize, f64, usize, f64, DVec2);

/// `None` when a crossing is degenerate.
fn clip(a: &[DVec2], b: &[DVec2], op: BoolOp) -> Option<Vec<Vec<DVec2>>> {
    let mut crossings: Vec<Crossing> = Vec::new();
    for i in 0..a.len() {
        let (p, r) = (a[i], a[(i + 1) % a.len()] - a[i]);
        for j in 0..b.len() {
            let (q, s) = (b[j], b[(j + 1) % b.len()] - b[j]);
            let d = cross(r, s);
            if d.abs() < 1e-9 * r.length() * s.length() {
                // Parallel: degenerate only if they overlap on one line.
                if cross(q - p, r).abs() < 1e-6 * r.length() {
                    let t0 = (q - p).dot(r) / r.length_squared();
                    let t1 = (q + s - p).dot(r) / r.length_squared();
                    if t0.max(t1) > -1e-9 && t0.min(t1) < 1.0 + 1e-9 {
                        return None;
                    }
                }
                continue;
            }
            let t = cross(q - p, s) / d;
            let u = cross(q - p, r) / d;
            let range = -1e-7..=1.0 + 1e-7;
            if range.contains(&t) && range.contains(&u) {
                let near = |x: f64| x.abs() < 1e-7 || (x - 1.0).abs() < 1e-7;
                if near(t) || near(u) {
                    return None;
                }
                crossings.push((i, t, j, u, p + r * t));
            }
        }
    }
    if crossings.is_empty() {
        let a_in_b = point_in_polygon(a[0], b);
        let b_in_a = point_in_polygon(b[0], a);
        let (a, b) = (a.to_vec(), b.to_vec());
        return Some(match op {
            BoolOp::Intersect if a_in_b => vec![a],
            BoolOp::Intersect if b_in_a => vec![b],
            BoolOp::Intersect => Vec::new(),
            BoolOp::Union if a_in_b => vec![b],
            BoolOp::Union if b_in_a => vec![a],
            BoolOp::Union => vec![a, b],
            BoolOp::Subtract if a_in_b => Vec::new(),
            BoolOp::Subtract if b_in_a => vec![with_hole(&a, &b)],
            BoolOp::Subtract => vec![a],
        });
    }
    let (flip_a, flip_b) = match op {
        BoolOp::Intersect => (false, false),
        BoolOp::Union => (true, true),
        BoolOp::Subtract => (true, false),
    };
    let ra = ring(a, b, &crossings, |c| (c.0, c.1), flip_a);
    let rb = ring(b, a, &crossings, |c| (c.2, c.3), flip_b);
    let pos = |r: &[Node], k: usize| r.iter().position(|n| n.cross == Some(k)).expect("crossing");
    let mut visited = vec![false; crossings.len()];
    let mut out = Vec::new();
    while let Some(start) = (0..crossings.len()).find(|k| !visited[*k]) {
        let mut poly = vec![crossings[start].4];
        let (mut on_a, mut k) = (true, start);
        for _ in 0..(ra.len() + rb.len()) * 2 {
            visited[k] = true;
            let r: &[Node] = if on_a { &ra } else { &rb };
            let mut i = pos(r, k);
            let forward = r[i].entry;
            loop {
                i = if forward { (i + 1) % r.len() } else { (i + r.len() - 1) % r.len() };
                poly.push(r[i].p);
                if let Some(next) = r[i].cross {
                    k = next;
                    break;
                }
            }
            on_a = !on_a;
            if visited[k] {
                break;
            }
        }
        poly.pop(); // back at the start
        out.push(poly);
    }
    Some(out)
}

/// `poly`'s ring with the crossings inserted in order along each edge,
/// each marked with whether walking forward from it enters `other`
/// (inverted when `flip`).
fn ring(
    poly: &[DVec2],
    other: &[DVec2],
    crossings: &[Crossing],
    edge_of: impl Fn(&Crossing) -> (usize, f64),
    flip: bool,
) -> Vec<Node> {
    let mut out = Vec::new();
    for (i, p) in poly.iter().enumerate() {
        out.push(Node {
            p: *p,
            cross: None,
            entry: false,
        });
        let mut here: Vec<(f64, usize)> = crossings
            .iter()
            .enumerate()
            .filter_map(|(k, c)| {
                let (e, t) = edge_of(c);
                (e == i).then_some((t, k))
            })
            .collect();
        here.sort_by(|x, y| x.0.total_cmp(&y.0));
        out.extend(here.into_iter().map(|(_, k)| Node {
            p: crossings[k].4,
            cross: Some(k),
            entry: false,
        }));
    }
    let mut inside = point_in_polygon(poly[0], other);
    for n in out.iter_mut().filter(|n| n.cross.is_some()) {
        n.entry = !inside != flip;
        inside = !inside;
    }
    out
}

/// `outer` with `hole` cut out, as one outline joined by a slit between
/// their nearest corners.
fn with_hole(outer: &[DVec2], hole: &[DVec2]) -> Vec<DVec2> {
    let mut hole = hole.to_vec();
    if signed_area(&hole) > 0.0 {
        hole.reverse();
    }
    let (mut bi, mut bj, mut best) = (0, 0, f64::MAX);
    for (i, p) in outer.iter().enumerate() {
        for (j, q) in hole.iter().enumerate() {
            let d = p.distance_squared(*q);
            if d < best {
                (bi, bj, best) = (i, j, d);
            }
        }
    }
    let mut out = outer[..=bi].to_vec();
    out.extend((0..=hole.len()).map(|k| hole[(bj + k) % hole.len()]));
    out.extend_from_slice(&outer[bi..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> Vec<DVec2> {
        vec![
            DVec2::new(x, y),
            DVec2::new(x + s, y),
            DVec2::new(x + s, y + s),
            DVec2::new(x, y + s),
        ]
    }

    fn area(polys: &[Vec<DVec2>]) -> f64 {
        polys.iter().map(|p| signed_area(p)).sum()
    }

    #[test]
    fn overlapping_squares() {
        let (a, b) = (square(0.0, 0.0, 2.0), square(1.0, 1.0, 2.0));
        let u = boolean(&a, &b, BoolOp::Union);
        assert_eq!(u.len(), 1);
        assert!((area(&u) - 7.0).abs() < 1e-6, "{}", area(&u));
        let i = boolean(&a, &b, BoolOp::Intersect);
        assert!((area(&i) - 1.0).abs() < 1e-6, "{}", area(&i));
        let d = boolean(&a, &b, BoolOp::Subtract);
        assert!((area(&d) - 3.0).abs() < 1e-6, "{}", area(&d));
        let d = boolean(&b, &a, BoolOp::Subtract);
        assert!((area(&d) - 3.0).abs() < 1e-6, "{}", area(&d));
    }

    #[test]
    fn shared_edges_and_holes() {
        // Side by side, sharing an edge.
        let u = boolean(&square(0.0, 0.0, 1.0), &square(1.0, 0.0, 1.0), BoolOp::Union);
        assert!((area(&u) - 2.0).abs() < 1e-3, "{}", area(&u));
        // A hole in the middle stays one outline.
        let d = boolean(&square(0.0, 0.0, 4.0), &square(1.0, 1.0, 1.0), BoolOp::Subtract);
        assert_eq!(d.len(), 1);
        assert!((area(&d) - 15.0).abs() < 1e-6, "{}", area(&d));
        // A cut right through splits it in two.
        let bar = vec![
            DVec2::new(-1.0, 1.0),
            DVec2::new(5.0, 1.0),
            DVec2::new(5.0, 2.0),
            DVec2::new(-1.0, 2.0),
        ];
        let d = boolean(&square(0.0, 0.0, 4.0), &bar, BoolOp::Subtract);
        assert_eq!(d.len(), 2);
        assert!((area(&d) - 12.0).abs() < 1e-6);
        let all = union_all(&[square(0.0, 0.0, 1.0), square(5.0, 0.0, 1.0), square(0.5, 0.0, 5.0)]);
        assert_eq!(all.len(), 1);
    }
}
