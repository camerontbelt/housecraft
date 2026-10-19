//! The terrain surface: heights from elevation data, its 3D mesh and
//! contour lines.

use glam::{DVec2, DVec3};
use housecraft_model::{Document, ElevationData, ElevationShape, Terrain};

use crate::boolean::{BoolOp, boolean};
use crate::math::{point_in_polygon, signed_area, spline_points, v2};
use crate::mesh::{Category, Look, Mesh};
use crate::triangulate::triangulate;

/// Heights on a regular grid over the terrain's outline.
#[derive(Debug, Clone)]
pub struct Surface {
    pub perimeter: Vec<DVec2>,
    pub origin: DVec2,
    pub cell: f64,
    pub nx: usize,
    pub ny: usize,
    /// (nx + 1) × (ny + 1) heights in mm, row by row.
    pub heights: Vec<f64>,
}

impl Surface {
    fn at(&self, i: usize, j: usize) -> f64 {
        self.heights[j * (self.nx + 1) + i]
    }

    fn point(&self, i: usize, j: usize) -> DVec2 {
        self.origin + DVec2::new(i as f64, j as f64) * self.cell
    }

    /// The ground height at `p` (mm), interpolated.
    pub fn height_at(&self, p: DVec2) -> f64 {
        let q = (p - self.origin) / self.cell;
        let i = (q.x.floor().max(0.0) as usize).min(self.nx.saturating_sub(1));
        let j = (q.y.floor().max(0.0) as usize).min(self.ny.saturating_sub(1));
        let (fx, fy) = ((q.x - i as f64).clamp(0.0, 1.0), (q.y - j as f64).clamp(0.0, 1.0));
        let (h00, h10, h01, h11) = (self.at(i, j), self.at(i + 1, j), self.at(i, j + 1), self.at(i + 1, j + 1));
        h00 * (1.0 - fx) * (1.0 - fy) + h10 * fx * (1.0 - fy) + h01 * (1.0 - fx) * fy + h11 * fx * fy
    }

    pub fn range(&self) -> (f64, f64) {
        let lo = self.heights.iter().copied().fold(f64::MAX, f64::min);
        let hi = self.heights.iter().copied().fold(f64::MIN, f64::max);
        (lo, hi)
    }
}

/// Points along a polyline about `step` apart.
fn resample(points: &[DVec2], closed: bool, step: f64) -> Vec<DVec2> {
    let n = points.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    let mut out = Vec::new();
    for i in 0..count {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let k = ((a.distance(b) / step).ceil() as usize).max(1);
        out.extend((0..k).map(|s| a.lerp(b, s as f64 / k as f64)));
    }
    if !closed && let Some(last) = points.last() {
        out.push(*last);
    }
    out
}

/// The least-squares plane h = a + b·x + c·y through the samples (level at
/// their mean when they don't span an area).
fn fit_plane(samples: &[(DVec2, f64)]) -> impl Fn(DVec2) -> f64 {
    let n = samples.len() as f64;
    let mean_p = samples.iter().map(|(p, _)| *p).sum::<DVec2>() / n;
    let mean_h = samples.iter().map(|(_, h)| h).sum::<f64>() / n;
    let (mut sxx, mut sxy, mut syy, mut sxh, mut syh) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (p, h) in samples {
        let d = *p - mean_p;
        let dh = h - mean_h;
        sxx += d.x * d.x;
        sxy += d.x * d.y;
        syy += d.y * d.y;
        sxh += d.x * dh;
        syh += d.y * dh;
    }
    let det = sxx * syy - sxy * sxy;
    let (b, c) = if det.abs() > 1e-6 * (sxx * syy).max(1.0) {
        ((sxh * syy - syh * sxy) / det, (syh * sxx - sxh * sxy) / det)
    } else if sxx > 1e-6 && syy < 1e-6 {
        (sxh / sxx, 0.0)
    } else if syy > 1e-6 && sxx < 1e-6 {
        (0.0, syh / syy)
    } else {
        (0.0, 0.0)
    };
    move |p: DVec2| mean_h + b * (p.x - mean_p.x) + c * (p.y - mean_p.y)
}

/// The terrain's surface shaped by every piece of elevation data in the plan.
pub fn surface(doc: &Document, t: &Terrain) -> Option<Surface> {
    let mut perimeter: Vec<DVec2> = t.perimeter.iter().map(|p| v2(*p)).collect();
    if perimeter.len() < 3 {
        return None;
    }
    if signed_area(&perimeter) < 0.0 {
        perimeter.reverse();
    }
    let (lo, hi) = crate::math::bounds(perimeter.iter().copied())?;
    let size = hi - lo;
    let cell = (size.max_element() / 80.0).max(300.0);
    let (nx, ny) = (((size.x / cell).ceil() as usize).max(1), ((size.y / cell).ceil() as usize).max(1));
    let data: Vec<&ElevationData> = doc.iter::<ElevationData>().collect();
    // Height samples from points, lines and region edges.
    let mut samples: Vec<(DVec2, f64)> = Vec::new();
    for d in &data {
        match &d.shape {
            ElevationShape::Point { at, elevation } => samples.push((v2(*at), elevation.mm())),
            ElevationShape::Line { points, elevation, smooth } => {
                let pts: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
                let pts = if *smooth { spline_points(&pts, false) } else { pts };
                samples.extend(resample(&pts, false, cell / 2.0).into_iter().map(|p| (p, elevation.mm())));
            }
            ElevationShape::Region { points, elevation } => {
                let pts: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
                samples.extend(resample(&pts, true, cell / 2.0).into_iter().map(|p| (p, elevation.mm())));
            }
            _ => {}
        }
    }
    // Retaining walls hold the ground at their top along them.
    for f in doc.iter::<housecraft_model::SiteFeature>() {
        if let housecraft_model::SiteKind::RetainingWall { top, .. } = f.feature {
            let pts: Vec<DVec2> = f.points.iter().map(|p| v2(*p)).collect();
            samples.extend(resample(&pts, false, cell / 2.0).into_iter().map(|p| (p, top.mm())));
        }
    }
    let base = t.base_elevation.mm();
    let mut heights = vec![base; (nx + 1) * (ny + 1)];
    if !samples.is_empty() {
        // A tilted plane through the data carries the overall slope;
        // inverse-distance weighting adds the local differences, so a few
        // spot heights make a slope rather than bumps.
        let plane = fit_plane(&samples);
        let residual: Vec<(DVec2, f64)> = samples.iter().map(|(q, h)| (*q, h - plane(*q))).collect();
        for j in 0..=ny {
            for i in 0..=nx {
                let p = lo + DVec2::new(i as f64, j as f64) * cell;
                let (mut num, mut den) = (0.0, 0.0);
                let mut exact = None;
                for (q, h) in &residual {
                    let d2 = p.distance_squared(*q);
                    if d2 < 1.0 {
                        exact = Some(*h);
                        break;
                    }
                    let w = 1.0 / d2;
                    num += w * h;
                    den += w;
                }
                heights[j * (nx + 1) + i] = plane(p) + exact.unwrap_or(num / den);
            }
        }
    }
    let mut s = Surface {
        perimeter,
        origin: lo,
        cell,
        nx,
        ny,
        heights,
    };
    for _ in 0..t.smoothing.min(20) {
        let old = s.heights.clone();
        for j in 0..=ny {
            for i in 0..=nx {
                let (mut sum, mut n) = (0.0, 0.0);
                for (di, dj) in [(0i64, 0i64), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (a, b) = (i as i64 + di, j as i64 + dj);
                    if a >= 0 && b >= 0 && a <= nx as i64 && b <= ny as i64 {
                        sum += old[b as usize * (nx + 1) + a as usize];
                        n += 1.0;
                    }
                }
                s.heights[j * (nx + 1) + i] = sum / n;
            }
        }
    }
    // Flat regions, then hills and valleys on top.
    for d in &data {
        let point_of = |i: usize, j: usize| lo + DVec2::new(i as f64, j as f64) * cell;
        match &d.shape {
            ElevationShape::Region { points, elevation } => {
                let poly: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
                for j in 0..=ny {
                    for i in 0..=nx {
                        if point_in_polygon(point_of(i, j), &poly) {
                            s.heights[j * (nx + 1) + i] = elevation.mm();
                        }
                    }
                }
            }
            ElevationShape::Hill { center, radius, height } | ElevationShape::Valley { center, radius, depth: height } => {
                let sign = if matches!(d.shape, ElevationShape::Hill { .. }) { 1.0 } else { -1.0 };
                let (c, r) = (v2(*center), radius.mm().max(1.0));
                for j in 0..=ny {
                    for i in 0..=nx {
                        let dist = point_of(i, j).distance(c);
                        if dist < r {
                            let bump = 0.5 * (1.0 + (std::f64::consts::PI * dist / r).cos());
                            s.heights[j * (nx + 1) + i] += sign * height.mm() * bump;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Some(s)
}

/// The terrain in 3D, with the building footprints cut out and a skirt
/// down its sides.
pub fn terrain_mesh(doc: &Document, t: &Terrain, mesh: &mut Mesh) {
    let Some(s) = surface(doc, t) else {
        return;
    };
    let [r, g, b, _] = t.color.0;
    let f = |v: u8| v as f32 / 255.0;
    let look = mesh.painted_whole().unwrap_or(Look {
        color: [f(r), f(g), f(b), 1.0],
        pbr: [0.95, 0.0, 0.0, 1.0],
        surface: None,
        tex: None,
    });
    let holes: Vec<Vec<DVec2>> = doc
        .floors_by_elevation()
        .first()
        .map(|f| crate::rooms::detect(doc, f.id).footprints)
        .unwrap_or_default();
    let hole_bounds: Vec<(DVec2, DVec2)> = holes
        .iter()
        .filter_map(|h| crate::math::bounds(h.iter().copied()))
        .collect();
    mesh.begin(t.id, Category::Terrain);
    for j in 0..s.ny {
        for i in 0..s.nx {
            let (a, c) = (s.point(i, j), s.point(i + 1, j + 1));
            let square = vec![a, DVec2::new(c.x, a.y), c, DVec2::new(a.x, c.y)];
            let inside = square.iter().all(|p| point_in_polygon(*p, &s.perimeter))
                && !s.perimeter.iter().any(|p| p.x > a.x && p.x < c.x && p.y > a.y && p.y < c.y);
            let mut pieces = if inside {
                vec![square]
            } else {
                boolean(&square, &s.perimeter, BoolOp::Intersect)
            };
            for (k, hole) in holes.iter().enumerate() {
                let (hl, hh) = hole_bounds[k];
                if hh.x < a.x || hl.x > c.x || hh.y < a.y || hl.y > c.y {
                    continue;
                }
                pieces = pieces.iter().flat_map(|p| boolean(p, hole, BoolOp::Subtract)).collect();
            }
            for piece in pieces {
                let pts: Vec<DVec3> = piece.iter().map(|p| p.extend(s.height_at(*p))).collect();
                for tri in triangulate(&piece) {
                    mesh.polygon_look(&[pts[tri[0]], pts[tri[1]], pts[tri[2]]], look);
                }
            }
        }
    }
    // Skirt and bottom.
    let (lo, _) = s.range();
    let bottom = lo - t.skirt.mm().max(10.0);
    let edge = resample(&s.perimeter, true, s.cell);
    let side = Look {
        color: [look.color[0] * 0.55, look.color[1] * 0.45, look.color[2] * 0.35, 1.0],
        ..look
    };
    for k in 0..edge.len() {
        let (p, q) = (edge[k], edge[(k + 1) % edge.len()]);
        mesh.polygon_look(
            &[p.extend(bottom), q.extend(bottom), q.extend(s.height_at(q)), p.extend(s.height_at(p))],
            side,
        );
    }
    let under: Vec<DVec3> = s.perimeter.iter().rev().map(|p| p.extend(bottom)).collect();
    mesh.polygon_look(&under, side);
    mesh.end();
}

/// Contour segments at every multiple of `interval` (mm): (from, to, height).
pub fn contours(s: &Surface, interval: f64) -> Vec<(DVec2, DVec2, f64)> {
    let mut out = Vec::new();
    if interval <= 0.0 {
        return out;
    }
    let (lo, hi) = s.range();
    let first = (lo / interval).ceil() as i64;
    let last = (hi / interval).floor() as i64;
    if last - first > 500 {
        return out;
    }
    for k in first..=last {
        let level = k as f64 * interval;
        for j in 0..s.ny {
            for i in 0..s.nx {
                let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
                let mut cross = Vec::new();
                for e in 0..4 {
                    let (a, b) = (corners[e], corners[(e + 1) % 4]);
                    let (ha, hb) = (s.at(a.0, a.1), s.at(b.0, b.1));
                    if (ha < level) != (hb < level) {
                        let t = (level - ha) / (hb - ha);
                        cross.push(s.point(a.0, a.1).lerp(s.point(b.0, b.1), t));
                    }
                }
                for pair in cross.as_chunks::<2>().0 {
                    let mid = (pair[0] + pair[1]) / 2.0;
                    if point_in_polygon(mid, &s.perimeter) {
                        out.push((pair[0], pair[1], level));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::*;
    use housecraft_units::{Length, Point2};

    fn p(x: f64, y: f64) -> Point2 {
        Point2(Length::from_feet(x), Length::from_feet(y))
    }

    #[test]
    fn hills_points_and_flat_regions_shape_the_ground() {
        let mut doc = Document::new("t");
        let floor = doc.floors_by_elevation()[0].id;
        let layer = doc.active_defaults().layers.cad;
        let mut t = Terrain::new(floor, layer, vec![p(-50.0, -50.0), p(50.0, -50.0), p(50.0, 50.0), p(-50.0, 50.0)]);
        t.smoothing = 0;
        let flat = surface(&doc, &t).unwrap();
        assert!((flat.height_at(DVec2::ZERO) - t.base_elevation.mm()).abs() < 1e-6);
        let mut add = |shape| {
            doc.insert(ElevationData { id: Id(0), floor, layer, shape });
        };
        add(ElevationShape::Point { at: p(-40.0, 0.0), elevation: Length::from_feet(10.0) });
        add(ElevationShape::Point { at: p(40.0, 0.0), elevation: Length::ZERO });
        add(ElevationShape::Hill { center: p(0.0, 40.0), radius: Length::from_feet(8.0), height: Length::from_feet(5.0) });
        add(ElevationShape::Region { points: vec![p(-5.0, -45.0), p(5.0, -45.0), p(5.0, -35.0), p(-5.0, -35.0)], elevation: Length::from_feet(3.0) });
        let s = surface(&doc, &t).unwrap();
        let ft = |v: f64| Length::from_feet(v).mm();
        assert!(s.height_at(v2(p(-38.0, 0.0))) > s.height_at(v2(p(38.0, 0.0))) + ft(5.0), "slopes down to the east");
        assert!(s.height_at(v2(p(0.0, 40.0))) > s.height_at(v2(p(0.0, 30.0))) + ft(3.0), "the hill");
        assert!((s.height_at(v2(p(0.0, -40.0))) - ft(3.0)).abs() < 1.0, "the pad is flat");
        assert!(!contours(&s, ft(1.0)).is_empty());
        let mut mesh = Mesh::for_document(&doc);
        terrain_mesh(&doc, &t, &mut mesh);
        assert!(mesh.indices.len() > 1000);
    }
}
