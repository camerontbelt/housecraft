//! 2D primitives: conversions, lines, polygons, curve tessellation.

use glam::DVec2;
use housecraft_model::Shape;
use housecraft_units::{Length, Point2};

/// Distances below this (mm) are treated as coincident.
pub const EPS: f64 = 1.0;

pub fn v2(p: Point2) -> DVec2 {
    DVec2::new(p.x().mm(), p.y().mm())
}

pub fn pt(v: DVec2) -> Point2 {
    Point2::from_mm(v.x, v.y)
}

pub fn len(mm: f64) -> Length {
    Length::from_mm(mm)
}

/// Left-hand perpendicular.
pub fn perp(d: DVec2) -> DVec2 {
    DVec2::new(-d.y, d.x)
}

pub fn cross(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Intersection of the infinite lines `p + t*d` and `q + s*e`.
pub fn line_intersection(p: DVec2, d: DVec2, q: DVec2, e: DVec2) -> Option<DVec2> {
    let denom = cross(d, e);
    if denom.abs() < 1e-12 * d.length() * e.length() {
        return None;
    }
    let t = cross(q - p, e) / denom;
    Some(p + d * t)
}

/// Intersection of segments `a0-a1` and `b0-b1`, with parameters along each.
pub fn segment_intersection(
    a0: DVec2,
    a1: DVec2,
    b0: DVec2,
    b1: DVec2,
) -> Option<(DVec2, f64, f64)> {
    let d = a1 - a0;
    let e = b1 - b0;
    let denom = cross(d, e);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = cross(b0 - a0, e) / denom;
    let s = cross(b0 - a0, d) / denom;
    let tol_t = 1e-9;
    if (-tol_t..=1.0 + tol_t).contains(&t) && (-tol_t..=1.0 + tol_t).contains(&s) {
        Some((a0 + d * t, t, s))
    } else {
        None
    }
}

/// Closest point on segment `a-b` to `p`, and its parameter in [0, 1].
pub fn closest_on_segment(p: DVec2, a: DVec2, b: DVec2) -> (DVec2, f64) {
    let d = b - a;
    let l2 = d.length_squared();
    if l2 < 1e-12 {
        return (a, 0.0);
    }
    let t = ((p - a).dot(d) / l2).clamp(0.0, 1.0);
    (a + d * t, t)
}

pub fn distance_to_segment(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    closest_on_segment(p, a, b).0.distance(p)
}

pub fn distance_to_polyline(p: DVec2, points: &[DVec2], closed: bool) -> f64 {
    let mut best = f64::INFINITY;
    for w in points.windows(2) {
        best = best.min(distance_to_segment(p, w[0], w[1]));
    }
    if closed && points.len() > 2 {
        best = best.min(distance_to_segment(p, points[points.len() - 1], points[0]));
    }
    if points.len() == 1 {
        best = p.distance(points[0]);
    }
    best
}

/// Signed area; positive for counter-clockwise polygons.
pub fn signed_area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| cross(poly[i], poly[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

pub fn centroid(poly: &[DVec2]) -> DVec2 {
    let a = signed_area(poly);
    if a.abs() < 1e-9 {
        return poly.iter().copied().sum::<DVec2>() / poly.len().max(1) as f64;
    }
    let n = poly.len();
    let mut c = DVec2::ZERO;
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        c += (p + q) * cross(p, q);
    }
    c / (6.0 * a)
}

pub fn point_in_polygon(p: DVec2, poly: &[DVec2]) -> bool {
    let n = poly.len();
    let mut inside = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Drop vertices that sit on a straight run (e.g. T-junction nodes in an outline).
pub fn simplify_collinear(poly: &[DVec2]) -> Vec<DVec2> {
    let n = poly.len();
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            let (u, v) = ((b - a).normalize_or_zero(), (c - b).normalize_or_zero());
            cross(u, v).abs() > 1e-6 || u.dot(v) < 0.0
        })
        .map(|i| poly[i])
        .collect()
}

/// Offset a counter-clockwise polygon inward by `d` (negative = outward),
/// mitring corners. Keeps the vertex count; very acute corners are capped.
pub fn inset_polygon(poly: &[DVec2], d: f64) -> Vec<DVec2> {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (p0, p1, p2) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            let d0 = (p1 - p0).normalize_or_zero();
            let d1 = (p2 - p1).normalize_or_zero();
            let a = p1 + perp(d0) * d;
            let b = p1 + perp(d1) * d;
            line_intersection(a, d0, b, d1)
                .filter(|c| c.distance(p1) < 6.0 * d.abs().max(1.0))
                .unwrap_or(b)
        })
        .collect()
}

/// A point well inside a polygon: the centroid if it is inside, otherwise the
/// sampled point farthest from the boundary.
pub fn interior_point(poly: &[DVec2]) -> DVec2 {
    let c = centroid(poly);
    if point_in_polygon(c, poly) && distance_to_polyline(c, poly, true) > EPS {
        return c;
    }
    let (min, max) = bounds(poly.iter().copied()).unwrap_or((c, c));
    let mut best = (c, -1.0);
    let steps = 24;
    for i in 0..=steps {
        for j in 0..=steps {
            let p = min + (max - min) * DVec2::new(i as f64, j as f64) / steps as f64;
            if point_in_polygon(p, poly) {
                let d = distance_to_polyline(p, poly, true);
                if d > best.1 {
                    best = (p, d);
                }
            }
        }
    }
    best.0
}

pub fn bounds(points: impl IntoIterator<Item = DVec2>) -> Option<(DVec2, DVec2)> {
    let mut it = points.into_iter();
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}

/// Points along an arc; angles in degrees, sweep positive counter-clockwise.
pub fn arc_points(center: DVec2, radius: f64, start_deg: f64, sweep_deg: f64) -> Vec<DVec2> {
    let segments = ((sweep_deg.abs() / 5.0).ceil() as usize).clamp(2, 180);
    (0..=segments)
        .map(|i| {
            let a = (start_deg + sweep_deg * i as f64 / segments as f64).to_radians();
            center + DVec2::new(a.cos(), a.sin()) * radius
        })
        .collect()
}

pub fn ellipse_points(center: DVec2, rx: f64, ry: f64, rotation_deg: f64) -> Vec<DVec2> {
    let rot = DVec2::from_angle(rotation_deg.to_radians());
    (0..72)
        .map(|i| {
            let a = (i as f64 * 5.0).to_radians();
            center + rot.rotate(DVec2::new(a.cos() * rx, a.sin() * ry))
        })
        .collect()
}

/// Centripetal-free uniform Catmull-Rom curve through `points`.
pub fn spline_points(points: &[DVec2], closed: bool) -> Vec<DVec2> {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    let get = |i: isize| -> DVec2 {
        if closed {
            points[i.rem_euclid(n as isize) as usize]
        } else {
            points[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let spans = if closed { n } else { n - 1 };
    let per = 12;
    let mut out = Vec::with_capacity(spans * per + 1);
    for s in 0..spans as isize {
        let (p0, p1, p2, p3) = (get(s - 1), get(s), get(s + 1), get(s + 2));
        for k in 0..per {
            let t = k as f64 / per as f64;
            let (t2, t3) = (t * t, t * t * t);
            out.push(
                0.5 * ((2.0 * p1)
                    + (-p0 + p2) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3),
            );
        }
    }
    if !closed {
        out.push(points[n - 1]);
    }
    out
}

/// Tessellate a CAD shape into polylines: (points, closed).
pub fn shape_polylines(shape: &Shape) -> Vec<(Vec<DVec2>, bool)> {
    match shape {
        Shape::Point { at } => vec![(vec![v2(*at)], false)],
        Shape::Line { a, b, .. } => vec![(vec![v2(*a), v2(*b)], false)],
        Shape::Polyline { points, closed } => {
            vec![(points.iter().map(|p| v2(*p)).collect(), *closed)]
        }
        Shape::Arc {
            center,
            radius,
            start_angle,
            sweep,
        } => vec![(
            arc_points(v2(*center), radius.mm(), *start_angle, *sweep),
            false,
        )],
        Shape::Circle { center, radius } => {
            let mut p = arc_points(v2(*center), radius.mm(), 0.0, 360.0);
            p.pop();
            vec![(p, true)]
        }
        Shape::Ellipse {
            center,
            rx,
            ry,
            rotation,
        } => vec![(
            ellipse_points(v2(*center), rx.mm(), ry.mm(), *rotation),
            true,
        )],
        Shape::Spline { points, closed } => {
            let pts: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
            vec![(spline_points(&pts, *closed), *closed)]
        }
    }
}

/// Characteristic points of a shape for snapping: (point, is_center).
pub fn shape_snap_points(shape: &Shape) -> Vec<(DVec2, bool)> {
    match shape {
        Shape::Point { at } => vec![(v2(*at), false)],
        Shape::Line { a, b, .. } => vec![(v2(*a), false), (v2(*b), false)],
        Shape::Polyline { points, .. } | Shape::Spline { points, .. } => {
            points.iter().map(|p| (v2(*p), false)).collect()
        }
        Shape::Arc {
            center,
            radius,
            start_angle,
            sweep,
        } => {
            let pts = arc_points(v2(*center), radius.mm(), *start_angle, *sweep);
            vec![
                (v2(*center), true),
                (pts[0], false),
                (*pts.last().expect("arc has points"), false),
            ]
        }
        Shape::Circle { center, .. } | Shape::Ellipse { center, .. } => {
            vec![(v2(*center), true)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_basics() {
        let sq = [
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 10.0),
            DVec2::new(0.0, 10.0),
        ];
        assert_eq!(signed_area(&sq), 100.0);
        assert_eq!(centroid(&sq), DVec2::new(5.0, 5.0));
        assert!(point_in_polygon(DVec2::new(5.0, 5.0), &sq));
        assert!(!point_in_polygon(DVec2::new(15.0, 5.0), &sq));
        // An L shape whose centroid is outside it.
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(100.0, 0.0),
            DVec2::new(100.0, 10.0),
            DVec2::new(10.0, 10.0),
            DVec2::new(10.0, 100.0),
            DVec2::new(0.0, 100.0),
        ];
        assert!(point_in_polygon(interior_point(&l), &l));
    }

    #[test]
    fn intersections() {
        let p = line_intersection(DVec2::ZERO, DVec2::X, DVec2::new(5.0, -5.0), DVec2::Y).unwrap();
        assert_eq!(p, DVec2::new(5.0, 0.0));
        assert!(line_intersection(DVec2::ZERO, DVec2::X, DVec2::Y, DVec2::X).is_none());
        let (_, t, s) = segment_intersection(
            DVec2::ZERO,
            DVec2::new(10.0, 0.0),
            DVec2::new(5.0, -5.0),
            DVec2::new(5.0, 5.0),
        )
        .unwrap();
        assert!((t - 0.5).abs() < 1e-12 && (s - 0.5).abs() < 1e-12);
    }
}
