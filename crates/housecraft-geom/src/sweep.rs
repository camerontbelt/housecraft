//! Sweeping a 2D profile along a path: moldings, trim, cladding.

use glam::{DVec2, DVec3};
use housecraft_model::{MoldingShape, Surface};

use crate::math::perp;
use crate::mesh::Mesh;

/// A molding profile as (out, up) points in mm, counter-clockwise, with
/// out = 0 at the surface it is fixed to and up = 0 at its bottom.
pub fn profile(shape: MoldingShape, height: f64, projection: f64) -> Vec<DVec2> {
    let (h, p) = (height, projection);
    match shape {
        MoldingShape::Flat => vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(p, 0.0),
            DVec2::new(p, h),
            DVec2::new(0.0, h),
        ],
        MoldingShape::Crown => vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(p * 0.15, 0.0),
            DVec2::new(p * 0.25, h * 0.15),
            DVec2::new(p * 0.85, h * 0.8),
            DVec2::new(p, h * 0.85),
            DVec2::new(p, h),
            DVec2::new(0.0, h),
        ],
        MoldingShape::Cove => {
            let mut pts = vec![DVec2::new(0.0, 0.0)];
            // A concave quarter circle from the wall out to the top edge.
            for k in 0..=6 {
                let a = k as f64 / 6.0 * std::f64::consts::FRAC_PI_2;
                pts.push(DVec2::new(p * (1.0 - a.cos()), h * a.sin()));
            }
            pts.push(DVec2::new(p, h));
            pts.push(DVec2::new(0.0, h));
            pts.dedup();
            pts
        }
        MoldingShape::Bead => {
            let mut pts = vec![DVec2::new(0.0, 0.0)];
            for k in 0..=6 {
                let a = -std::f64::consts::FRAC_PI_2 + k as f64 / 6.0 * std::f64::consts::PI;
                let r = p.min(h / 2.0);
                pts.push(DVec2::new(p - r + r * a.cos(), r + r * a.sin()));
            }
            pts.push(DVec2::new(p * 0.5, h));
            pts.push(DVec2::new(0.0, h));
            pts
        }
    }
}

/// Sweep `profile` along the plan path `path` at base height `z`. The
/// profile's `out` axis points to the right of the path's direction (so a
/// counter-clockwise outline gets its moldings on the outside when the path
/// is traversed clockwise, and vice versa); corners are mitred. Open paths
/// get end caps.
pub fn sweep(mesh: &mut Mesh, path: &[DVec2], closed: bool, profile: &[DVec2], z: f64, surface: Surface) {
    let n = path.len();
    if n < 2 || profile.len() < 3 {
        return;
    }
    let seg_normal = |i: usize| -perp((path[(i + 1) % n] - path[i]).normalize_or(DVec2::X));
    // The mitre offset direction (scaled so the profile keeps its depth) at each point.
    let miter: Vec<DVec2> = (0..n)
        .map(|i| {
            let prev = if i == 0 {
                if closed { Some(seg_normal(n - 1)) } else { None }
            } else {
                Some(seg_normal(i - 1))
            };
            let next = if i == n - 1 && !closed { None } else { Some(seg_normal(i)) };
            match (prev, next) {
                (Some(a), Some(b)) => {
                    let m = (a + b).normalize_or(a);
                    m / m.dot(a).max(0.2)
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => DVec2::Y,
            }
        })
        .collect();
    let ring = |i: usize| -> Vec<DVec3> {
        profile
            .iter()
            .map(|q| (path[i] + miter[i] * q.x).extend(z + q.y))
            .collect()
    };
    let rings: Vec<Vec<DVec3>> = (0..n).map(ring).collect();
    let segs = if closed { n } else { n - 1 };
    let m = profile.len();
    let center = |r: &[DVec3]| r.iter().copied().sum::<DVec3>() / r.len() as f64;
    for i in 0..segs {
        let (a, b) = (&rings[i], &rings[(i + 1) % n]);
        let axis = (center(a) + center(b)) / 2.0;
        for k in 0..m {
            let k2 = (k + 1) % m;
            let quad = [a[k], b[k], b[k2], a[k2]];
            let mid = quad.iter().copied().sum::<DVec3>() / 4.0;
            oriented(mesh, &quad, mid - axis, surface);
        }
    }
    if !closed {
        let t0 = (path[0] - path[1]).extend(0.0);
        let t1 = (path[n - 1] - path[n - 2]).extend(0.0);
        oriented(mesh, &rings[0], t0, surface);
        oriented(mesh, &rings[n - 1], t1, surface);
    }
}

/// Add a polygon with its normal turned toward `want`.
pub fn oriented(mesh: &mut Mesh, pts: &[DVec3], want: DVec3, surface: Surface) {
    if crate::roof::newell(pts).dot(want) < 0.0 {
        let rev: Vec<DVec3> = pts.iter().rev().copied().collect();
        mesh.polygon(&rev, surface);
    } else {
        mesh.polygon(pts, surface);
    }
}
