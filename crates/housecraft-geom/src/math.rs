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
        Shape::CrossBox { a, b, width } | Shape::BlockingBox { a, b, width } => {
            let c = detail_box(v2(*a), v2(*b), width.mm());
            let mut out = vec![(c.to_vec(), true), (vec![c[0], c[2]], false)];
            if matches!(shape, Shape::CrossBox { .. }) {
                out.push((vec![c[1], c[3]], false));
            }
            out
        }
        Shape::Insulation { a, b, width } => {
            let (a, b, w) = (v2(*a), v2(*b), width.mm());
            vec![
                (detail_box(a, b, w).to_vec(), true),
                (batt_points(a, b, w), false),
            ]
        }
    }
}

/// Corners of a box along `a`→`b`, `w` wide, centred on the line.
pub fn detail_box(a: DVec2, b: DVec2, w: f64) -> [DVec2; 4] {
    let n = perp((b - a).normalize_or_zero()) * (w / 2.0);
    [a - n, b - n, b + n, a + n]
}

/// The batt insulation squiggle: loops across the depth `w` every `w / 2`.
pub fn batt_points(a: DVec2, b: DVec2, w: f64) -> Vec<DVec2> {
    let len = a.distance(b);
    if len < 1e-6 || w <= 0.0 {
        return vec![a, b];
    }
    let d = (b - a) / len;
    let n = perp(d);
    let steps = ((len / w) * 24.0).ceil().max(8.0) as usize;
    (0..=steps)
        .map(|i| {
            let t = len * i as f64 / steps as f64;
            // A sine wave whose period is the depth reads as the batt symbol.
            let y = (w / 2.0) * 0.9 * (std::f64::consts::TAU * t / w).sin();
            a + d * t + n * y
        })
        .collect()
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
        Shape::CrossBox { a, b, .. }
        | Shape::BlockingBox { a, b, .. }
        | Shape::Insulation { a, b, .. } => {
            vec![(v2(*a), false), (v2(*b), false)]
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

/// Keep the part of `poly` on the left of the directed line a→b (or the right).
pub fn clip_half_plane(poly: &[DVec2], a: DVec2, b: DVec2, keep_left: bool) -> Vec<DVec2> {
    let side = |p: DVec2| {
        let c = cross(b - a, p - a);
        if keep_left { c >= -1e-9 } else { c <= 1e-9 }
    };
    let mut out = Vec::new();
    for j in 0..poly.len() {
        let (p, q) = (poly[j], poly[(j + 1) % poly.len()]);
        let (pi, qi) = (side(p), side(q));
        if pi {
            out.push(p);
        }
        if pi != qi
            && let Some(x) = line_intersection(p, q - p, a, b - a)
        {
            out.push(x);
        }
    }
    out
}

/// Convex pieces covering `poly` minus a set of convex `holes`.
pub fn minus_convex_holes(poly: &[DVec2], holes: &[Vec<DVec2>]) -> Vec<Vec<DVec2>> {
    let mut pieces: Vec<Vec<DVec2>> = if holes.is_empty() {
        return vec![poly.to_vec()];
    } else {
        crate::triangulate::triangulate(poly)
            .into_iter()
            .map(|t| vec![poly[t[0]], poly[t[1]], poly[t[2]]])
            .collect()
    };
    for hole in holes {
        let mut h = hole.clone();
        if signed_area(&h) < 0.0 {
            h.reverse();
        }
        let m = h.len();
        let mut next = Vec::new();
        for piece in pieces {
            // piece − H = ⋃ᵢ (piece ∩ right(eᵢ) ∩ left(e₀..eᵢ₋₁))
            let mut rest = piece;
            for i in 0..m {
                let (a, b) = (h[i], h[(i + 1) % m]);
                let outside = clip_half_plane(&rest, a, b, false);
                if outside.len() >= 3 && signed_area(&outside).abs() > 1.0 {
                    next.push(outside);
                }
                rest = clip_half_plane(&rest, a, b, true);
                if rest.len() < 3 {
                    break;
                }
            }
        }
        pieces = next;
    }
    pieces
}

/// Convex pieces covering `poly` ∩ `convex` (`poly` may be concave).
pub fn intersect_convex(poly: &[DVec2], convex: &[DVec2]) -> Vec<Vec<DVec2>> {
    let mut c = convex.to_vec();
    if signed_area(&c) < 0.0 {
        c.reverse();
    }
    let mut out = Vec::new();
    for t in crate::triangulate::triangulate(poly) {
        let mut piece = vec![poly[t[0]], poly[t[1]], poly[t[2]]];
        for i in 0..c.len() {
            piece = clip_half_plane(&piece, c[i], c[(i + 1) % c.len()], true);
            if piece.len() < 3 {
                break;
            }
        }
        if piece.len() >= 3 && signed_area(&piece).abs() > 1.0 {
            out.push(piece);
        }
    }
    out
}

/// The polygon's dominant direction (unit): the direction with the most total
/// edge length, so a long wall split by T-junctions still counts as one.
pub fn long_axis(poly: &[DVec2]) -> DVec2 {
    let mut buckets: Vec<(DVec2, f64)> = Vec::new();
    for i in 0..poly.len() {
        let d = poly[(i + 1) % poly.len()] - poly[i];
        let len = d.length();
        if len < 1e-6 {
            continue;
        }
        let mut u = d / len;
        // Opposite edges run the same way for this purpose.
        if u.x < -1e-9 || (u.x.abs() <= 1e-9 && u.y < 0.0) {
            u = -u;
        }
        match buckets.iter_mut().find(|(b, _)| b.dot(u) > 0.9998) {
            Some((_, total)) => *total += len,
            None => buckets.push((u, len)),
        }
    }
    buckets
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(DVec2::X, |(u, _)| u)
}

#[cfg(test)]
mod hole_tests {
    use super::*;

    #[test]
    fn convex_hole_is_subtracted() {
        let square = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(100.0, 0.0),
            DVec2::new(100.0, 100.0),
            DVec2::new(0.0, 100.0),
        ];
        let hole = vec![
            DVec2::new(20.0, 20.0),
            DVec2::new(20.0, 50.0),
            DVec2::new(60.0, 50.0),
            DVec2::new(60.0, 20.0),
        ];
        let pieces = minus_convex_holes(&square, &[hole]);
        let area: f64 = pieces.iter().map(|p| signed_area(p).abs()).sum();
        assert!((area - (10_000.0 - 1_200.0)).abs() < 1e-6, "{area}");
        // A hole hanging off the edge only removes the overlap.
        let edge = vec![
            DVec2::new(90.0, 0.0),
            DVec2::new(110.0, 0.0),
            DVec2::new(110.0, 10.0),
            DVec2::new(90.0, 10.0),
        ];
        let pieces = minus_convex_holes(&square, &[edge]);
        let area: f64 = pieces.iter().map(|p| signed_area(p).abs()).sum();
        assert!((area - 9_900.0).abs() < 1e-6, "{area}");
    }
}
