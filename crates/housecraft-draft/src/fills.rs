//! Patterned fills for closed areas: solid, hatch, crosshatch and dots,
//! drawn as ordinary lines and polygons so every output shows them.

use housecraft_geom::DVec2;
use housecraft_geom::math::point_in_polygon;
use housecraft_model::{Color, FillPattern, FillStyle, HatchPattern, Id};

use crate::{Drawing, Fill, Ink, Prim, Stroke};

/// Fill `poly` (model mm) with `style`; `ink` is used when it has no colour.
/// `patterns` are the plan's own hatch patterns.
pub fn fill_area(out: &mut Drawing, id: Option<Id>, poly: &[DVec2], style: &FillStyle, ink: Ink, patterns: &[HatchPattern]) {
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
        FillPattern::UserPattern(i) => {
            let stroke = Stroke { ink, width: 0.13, dash: &[] };
            let spacing = style.spacing.mm().max(1.0);
            let lines = match patterns.get(i as usize) {
                Some(p) => p.lines.clone(),
                None => vec![housecraft_model::HatchLine::default()],
            };
            for l in &lines {
                let gap = (l.spacing * spacing).max(1.0);
                let angle = style.angle + l.angle;
                let d = DVec2::from_angle(angle.to_radians());
                let n = DVec2::new(-d.y, d.x);
                let dash: Vec<f64> = l.dash.iter().map(|x| x * spacing).collect();
                for (p, q) in hatch_lines_offset(poly, angle, gap, l.offset * spacing) {
                    // Alternate lines shift their dashes (brick bond).
                    let k = ((p.dot(n) - l.offset * spacing) / gap).round() as i64;
                    let shift = if k.rem_euclid(2) == 1 { l.stagger * spacing } else { 0.0 };
                    for (a, b) in dashed(p, q, d, &dash, shift) {
                        out.push(id, Prim::Polyline { points: vec![a, b], closed: false, stroke });
                    }
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

/// The drawn pieces of segment `p`→`q` (running along `d`) for a dash
/// pattern measured from the origin, shifted by `shift`; all of it when
/// there is no pattern.
pub fn dashed(p: DVec2, q: DVec2, d: DVec2, dash: &[f64], shift: f64) -> Vec<(DVec2, DVec2)> {
    let period: f64 = dash.iter().sum();
    if dash.is_empty() || period <= 1e-9 {
        return vec![(p, q)];
    }
    let (u0, u1) = (p.dot(d), q.dot(d));
    let base = p - d * u0;
    let mut out = Vec::new();
    // Start at the dash period containing u0.
    let mut start = ((u0 - shift) / period).floor() * period + shift;
    let mut guard = 0;
    while start < u1 && guard < 100_000 {
        let mut u = start;
        for (i, len) in dash.iter().enumerate() {
            let (a, b) = (u.max(u0), (u + len).min(u1));
            if i % 2 == 0 && b > a {
                out.push((base + d * a, base + d * b));
            }
            u += len;
        }
        start += period;
        guard += 1;
    }
    out
}

/// Parallel lines at `angle_deg`, `spacing` apart, clipped to `poly`.
pub fn hatch_lines(poly: &[DVec2], angle_deg: f64, spacing: f64) -> Vec<(DVec2, DVec2)> {
    hatch_lines_offset(poly, angle_deg, spacing, 0.0)
}

/// As [`hatch_lines`], the family shifted `offset` across the lines.
pub fn hatch_lines_offset(poly: &[DVec2], angle_deg: f64, spacing: f64, offset: f64) -> Vec<(DVec2, DVec2)> {
    let d = DVec2::from_angle(angle_deg.to_radians());
    let n = DVec2::new(-d.y, d.x);
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for p in poly {
        let t = p.dot(n);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    let mut out = Vec::new();
    let mut t = ((lo - offset) / spacing).ceil() * spacing + offset;
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

    #[test]
    fn dashes_and_user_patterns() {
        let pieces = dashed(DVec2::ZERO, DVec2::new(10.0, 0.0), DVec2::X, &[2.0, 1.0], 0.0);
        assert_eq!(pieces.len(), 4);
        assert_eq!(pieces[3], (DVec2::new(9.0, 0.0), DVec2::new(10.0, 0.0)));
        // A brick pattern: courses plus staggered joints.
        let brick = HatchPattern {
            name: "Brick".into(),
            lines: vec![
                housecraft_model::HatchLine::default(),
                housecraft_model::HatchLine { angle: 90.0, spacing: 2.0, dash: vec![1.0, 1.0], stagger: 1.0, ..Default::default() },
            ],
        };
        let sq = [DVec2::ZERO, DVec2::new(100.0, 0.0), DVec2::new(100.0, 100.0), DVec2::new(0.0, 100.0)];
        let mut d = Drawing::default();
        let style = FillStyle { pattern: FillPattern::UserPattern(0), spacing: housecraft_units::Length::from_mm(10.0), angle: 0.0, ..Default::default() };
        fill_area(&mut d, None, &sq, &style, Ink::Default, &[brick]);
        let n = d.items.len();
        assert!(n > 9 + 20, "{n} lines");
    }
}
