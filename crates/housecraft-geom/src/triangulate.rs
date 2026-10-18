//! Ear-clipping triangulation of simple polygons.

use glam::DVec2;

use crate::math::{cross, signed_area};

/// Triangulate a simple polygon (either winding). Returns index triples into
/// `poly`, wound counter-clockwise. Degenerate input yields fewer triangles.
pub fn triangulate(poly: &[DVec2]) -> Vec<[usize; 3]> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    let mut idx: Vec<usize> = (0..n).collect();
    if signed_area(poly) < 0.0 {
        idx.reverse();
    }
    let mut out = Vec::with_capacity(n - 2);
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let (a, b, c) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (pa, pb, pc) = (poly[a], poly[b], poly[c]);
            let turn = cross(pb - pa, pc - pb);
            if turn <= 1e-9 {
                continue; // reflex or collinear
            }
            let contains = idx
                .iter()
                .any(|&j| j != a && j != b && j != c && in_triangle(poly[j], pa, pb, pc));
            if !contains {
                out.push([a, b, c]);
                idx.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            // Only collinear runs left; drop the flattest vertex and continue.
            let m = idx.len();
            let flattest = (0..m)
                .min_by(|&i, &j| {
                    let t = |k: usize| {
                        let (a, b, c) = (
                            poly[idx[(k + m - 1) % m]],
                            poly[idx[k]],
                            poly[idx[(k + 1) % m]],
                        );
                        cross(b - a, c - b).abs()
                    };
                    t(i).total_cmp(&t(j))
                })
                .expect("non-empty");
            idx.remove(flattest);
        }
    }
    if idx.len() == 3 {
        let (a, b, c) = (idx[0], idx[1], idx[2]);
        if cross(poly[b] - poly[a], poly[c] - poly[b]) > 1e-9 {
            out.push([a, b, c]);
        }
    }
    out
}

fn in_triangle(p: DVec2, a: DVec2, b: DVec2, c: DVec2) -> bool {
    let d1 = cross(b - a, p - a);
    let d2 = cross(c - b, p - b);
    let d3 = cross(a - c, p - c);
    d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangulates_concave_polygon_fully() {
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(100.0, 0.0),
            DVec2::new(100.0, 10.0),
            DVec2::new(10.0, 10.0),
            DVec2::new(10.0, 100.0),
            DVec2::new(0.0, 100.0),
        ];
        let tris = triangulate(&l);
        assert_eq!(tris.len(), 4);
        let area: f64 = tris
            .iter()
            .map(|t| signed_area(&[l[t[0]], l[t[1]], l[t[2]]]))
            .sum();
        assert!((area - signed_area(&l)).abs() < 1e-6);
        // Clockwise input gives the same coverage.
        let mut cw = l.to_vec();
        cw.reverse();
        assert_eq!(triangulate(&cw).len(), 4);
    }
}
