//! Patterned fills for closed areas: solid, hatch, crosshatch and dots,
//! drawn as ordinary lines and polygons so every output shows them.

use housecraft_geom::DVec2;
use housecraft_geom::math::point_in_polygon;
use housecraft_model::{Color, FillPattern, FillStyle, Id};

use crate::{Drawing, Fill, Ink, Prim, Stroke};

/// Fill `poly` (model mm) with `style`; `ink` is used when it has no colour.
pub fn fill_area(out: &mut Drawing, id: Option<Id>, poly: &[DVec2], style: &FillStyle, ink: Ink) {
    if poly.len() < 3 {
        return;
    }
    let ink = style.color.map_or(ink, Ink::Color);
    match style.pattern {
        FillPattern::Solid => {
            let fill = match (ink, style.color) {
                (_, Some(c)) => {
                    let a = (style.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
                    Fill::Ink(Ink::Color(Color([c.0[0], c.0[1], c.0[2], a])))
                }
                (ink, None) => Fill::Ink(ink),
            };
            out.push(id, Prim::Polygon { points: poly.to_vec(), fill });
        }
        FillPattern::Hatch | FillPattern::Crosshatch => {
            let stroke = Stroke { ink, width: 0.13, dash: &[] };
            let spacing = style.spacing.mm().max(1.0);
            let mut angles = vec![style.angle];
            if style.pattern == FillPattern::Crosshatch {
                angles.push(style.angle + 90.0);
            }
            for a in angles {
                for (p, q) in hatch_lines(poly, a, spacing) {
                    out.push(id, Prim::Polyline { points: vec![p, q], closed: false, stroke });
                }
            }
        }
        FillPattern::Dots => {
            let spacing = style.spacing.mm().max(1.0);
            let Some((lo, hi)) = housecraft_geom::math::bounds(poly.iter().copied()) else {
                return;
            };
            let r = spacing * 0.06;
            let mut n = 0;
            let mut y = (lo.y / spacing).floor() * spacing;
            while y <= hi.y && n < 20_000 {
                let mut x = (lo.x / spacing).floor() * spacing;
                while x <= hi.x && n < 20_000 {
                    let c = DVec2::new(x, y);
                    if point_in_polygon(c, poly) {
                        let dot = (0..6).map(|i| c + DVec2::from_angle(i as f64 * std::f64::consts::TAU / 6.0) * r).collect();
                        out.push(id, Prim::Polygon { points: dot, fill: Fill::Ink(ink) });
                        n += 1;
                    }
                    x += spacing;
                }
                y += spacing;
            }
        }
    }
}

/// Parallel lines at `angle_deg`, `spacing` apart, clipped to `poly`.
pub fn hatch_lines(poly: &[DVec2], angle_deg: f64, spacing: f64) -> Vec<(DVec2, DVec2)> {
    let d = DVec2::from_angle(angle_deg.to_radians());
    let n = DVec2::new(-d.y, d.x);
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for p in poly {
        let t = p.dot(n);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    let mut out = Vec::new();
    let mut t = (lo / spacing).ceil() * spacing;
    let mut count = 0;
    while t < hi && count < 5000 {
        // Where each edge crosses the line {x : x·n = t}, measured along d.
        let mut us: Vec<f64> = Vec::new();
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (ta, tb) = (a.dot(n) - t, b.dot(n) - t);
            if (ta < 0.0) != (tb < 0.0) {
                let p = a + (b - a) * (ta / (ta - tb));
                us.push(p.dot(d));
            }
        }
        us.sort_by(f64::total_cmp);
        for pair in us.chunks(2) {
            if let [u0, u1] = pair {
                out.push((n * t + d * *u0, n * t + d * *u1));
            }
        }
        t += spacing;
        count += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hatch_stays_inside() {
        let sq = [DVec2::ZERO, DVec2::new(100.0, 0.0), DVec2::new(100.0, 100.0), DVec2::new(0.0, 100.0)];
        let lines = hatch_lines(&sq, 0.0, 10.0);
        assert_eq!(lines.len(), 9, "lines strictly inside, 10 apart");
        for (a, b) in lines {
            assert!((a.x - 0.0).abs() < 1e-9 && (b.x - 100.0).abs() < 1e-9);
        }
        assert!(!hatch_lines(&sq, 45.0, 10.0).is_empty());
    }
}
