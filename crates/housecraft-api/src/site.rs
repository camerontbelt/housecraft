//! Reading survey points for the terrain.

use housecraft_units::DisplayUnit;

use crate::ApiError;
use housecraft_model::{Cad, Document, Id, Justify, Shape, Text};

/// Survey points as (x, y, elevation) in mm: text with one "x, y, z" per
/// line in `unit`, or a GPX file's track points and waypoints (latitude,
/// longitude, metres) laid out around the first point.
pub(crate) fn parse_points(text: &str, gpx: bool, unit: DisplayUnit) -> Result<Vec<(f64, f64, f64)>, ApiError> {
    if gpx {
        return Ok(parse_gpx(text));
    }
    let mm = match unit {
        DisplayUnit::FeetInches { .. } => 304.8,
        DisplayUnit::Inches { .. } => 25.4,
        DisplayUnit::Millimeters => 1.0,
        DisplayUnit::Meters { .. } => 1000.0,
    };
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let nums: Vec<f64> = line
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        match nums.as_slice() {
            // A point number first is allowed: "12, x, y, z".
            [x, y, z] | [_, x, y, z, ..] => out.push((x * mm, y * mm, z * mm)),
            // A header line.
            [] if n == 0 => {}
            _ => {
                return Err(ApiError::Invalid(format!(
                    "line {}: expected x, y and elevation, got {line:?}",
                    n + 1
                )));
            }
        }
    }
    Ok(out)
}

fn attr(tag: &str, name: &str) -> Option<f64> {
    let key = format!("{name}=");
    let rest = &tag[tag.find(&key)? + key.len()..];
    let quote = rest.chars().next()?;
    let rest = &rest[1..];
    rest[..rest.find(quote)?].parse().ok()
}

fn parse_gpx(text: &str) -> Vec<(f64, f64, f64)> {
    let mut raw = Vec::new();
    for kind in ["<trkpt", "<wpt", "<rtept"] {
        let mut rest = text;
        while let Some(i) = rest.find(kind) {
            rest = &rest[i + kind.len()..];
            let tag_end = rest.find('>').unwrap_or(rest.len());
            let tag = &rest[..tag_end];
            let (Some(lat), Some(lon)) = (attr(tag, "lat"), attr(tag, "lon")) else {
                continue;
            };
            // The point's own <ele>, before the next point starts.
            let ele = rest
                .find("<ele>")
                .filter(|e| *e < rest.find(kind).unwrap_or(usize::MAX))
                .and_then(|e| {
                    let s = &rest[e + 5..];
                    s[..s.find('<')?].trim().parse::<f64>().ok()
                })
                .unwrap_or(0.0);
            raw.push((lat, lon, ele));
        }
    }
    let Some(&(lat0, lon0, _)) = raw.first() else {
        return Vec::new();
    };
    let k = lat0.to_radians().cos();
    raw.iter()
        .map(|(lat, lon, ele)| {
            (
                (lon - lon0) * k * 111_320.0 * 1000.0,
                (lat - lat0) * 110_540.0 * 1000.0,
                ele * 1000.0,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survey_text_and_gpx() {
        let pts = parse_points("x,y,z\n0, 0, 1\n10 20 2.5\n7, 1, 2, 3\n", false, DisplayUnit::Meters { decimals: 2 }).unwrap();
        assert_eq!(pts, vec![(0.0, 0.0, 1000.0), (10000.0, 20000.0, 2500.0), (1000.0, 2000.0, 3000.0)]);
        assert!(parse_points("1, 2\n", false, DisplayUnit::Millimeters).is_err());
        let gpx = r#"<gpx><trk><trkseg>
            <trkpt lat="45.0" lon="-122.0"><ele>100</ele></trkpt>
            <trkpt lat="45.001" lon="-122.0"><ele>103.5</ele></trkpt>
        </trkseg></trk></gpx>"#;
        let pts = parse_points(gpx, true, DisplayUnit::Millimeters).unwrap();
        assert_eq!(pts.len(), 2);
        assert!((pts[1].1 - 110_540.0).abs() < 1.0, "about 110 m north");
        assert!((pts[1].2 - 103_500.0).abs() < 1e-6);
    }
}

/// A surveyor's bearing ("N 45°30'15\" E", "S12-30W", "n 7 e") as a unit
/// direction in plan (+y is north).
pub(crate) fn parse_bearing(s: &str) -> Result<housecraft_geom::DVec2, ApiError> {
    let t = s.trim().to_uppercase();
    let bad = || ApiError::Invalid(format!("can't read the bearing {s:?} (use e.g. N 45°30' E)"));
    let ns = t.chars().next().ok_or_else(bad)?;
    let ew = t.chars().last().ok_or_else(bad)?;
    if !matches!(ns, 'N' | 'S') || !matches!(ew, 'E' | 'W') {
        return Err(bad());
    }
    let inner = &t[1..t.len() - 1];
    let parts: Vec<f64> = inner
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f64>().map_err(|_| bad()))
        .collect::<Result<_, _>>()?;
    let deg = match parts.as_slice() {
        [d] => *d,
        [d, m] => d + m / 60.0,
        [d, m, sec] => d + m / 60.0 + sec / 3600.0,
        _ => return Err(bad()),
    };
    if !(0.0..=90.0).contains(&deg) {
        return Err(bad());
    }
    let a = deg.to_radians();
    let x = if ew == 'E' { a.sin() } else { -a.sin() };
    let y = if ns == 'N' { a.cos() } else { -a.cos() };
    Ok(housecraft_geom::DVec2::new(x, y))
}

#[cfg(test)]
mod bearing_tests {
    use super::*;

    #[test]
    fn bearings() {
        let d = parse_bearing("N 90°00'00\" E").unwrap();
        assert!((d.x - 1.0).abs() < 1e-9 && d.y.abs() < 1e-9);
        let d = parse_bearing("s45-0-0w").unwrap();
        assert!((d.x - d.y).abs() < 1e-9 && d.x < 0.0 && d.y < 0.0);
        assert!(parse_bearing("NE").is_err());
        assert!(parse_bearing("N 95 E").is_err());
    }
}

/// A site plan's shapes: the terrain outline, filled building footprints,
/// site features and plant canopies, with a label on each building.
pub(crate) fn site_plan(doc: &housecraft_model::Document) -> (Vec<housecraft_model::DetailShape>, Vec<crate::DetailText>) {
    use housecraft_geom::{pt, v2};
    use housecraft_model::*;
    let mut shapes = Vec::new();
    let mut texts = Vec::new();
    let poly = |pts: Vec<housecraft_geom::DVec2>, closed: bool, fill: Option<Color>| DetailShape {
        shape: Shape::Polyline { points: pts.into_iter().map(pt).collect(), closed },
        color: None,
        fill,
    };
    for t in doc.iter::<Terrain>() {
        shapes.push(poly(t.perimeter.iter().map(|p| v2(*p)).collect(), true, None));
    }
    if let Some(floor) = doc.floors_by_elevation().first() {
        for f in housecraft_geom::rooms::detect(doc, floor.id).footprints {
            let c = housecraft_geom::math::centroid(&f);
            let area = housecraft_geom::math::signed_area(&f).abs();
            texts.push(crate::DetailText {
                position: pt(c),
                text: format!("House ({})", housecraft_units::format_area(area, doc.settings.display_unit)),
                height: None,
                rotation: 0.0,
                justify: Justify::Center,
            });
            shapes.push(poly(f, true, Some(Color::rgb(200, 200, 200))));
        }
    }
    for f in doc.iter::<SiteFeature>() {
        let pts: Vec<housecraft_geom::DVec2> = f.points.iter().map(|p| v2(*p)).collect();
        shapes.push(poly(pts, f.feature.closed(), None));
    }
    for p in doc.iter::<Plant>() {
        shapes.push(DetailShape {
            shape: Shape::Circle { center: p.at, radius: p.spread * 0.5 },
            color: Some(Color::rgb(60, 120, 60)),
            fill: None,
        });
    }
    (shapes, texts)
}

/// Split a closed lot into `count` equal-area lots with parallel lines.
pub(crate) fn subdivide(doc: &mut Document, lot: Id, count: u32, angle: Option<f64>, out: &mut crate::Outcome) -> Result<(), ApiError> {
    use housecraft_geom::math::{centroid, shape_polylines, signed_area};
    use housecraft_geom::DVec2;
    let cad = doc.get::<Cad>(lot).cloned().ok_or(ApiError::NotFound(lot))?;
    let Some((poly, true)) = shape_polylines(&cad.shape).into_iter().next() else {
        return Err(ApiError::Invalid("the lot must be a closed outline".into()));
    };
    if !(2..=200).contains(&count) {
        return Err(ApiError::Invalid("divide into 2 to 200 lots".into()));
    }
    // Lot lines run across the longest side by default.
    let dir = match angle {
        Some(a) => DVec2::from_angle(a.to_radians()),
        None => {
            let n = poly.len();
            let (a, b) = (0..n).map(|i| (poly[i], poly[(i + 1) % n])).max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1))).expect("edges");
            let along = (b - a).normalize_or_zero();
            DVec2::new(-along.y, along.x)
        }
    };
    // Offsets across the lines; area on the low side of each offset.
    let across = DVec2::new(-dir.y, dir.x);
    let proj: Vec<f64> = poly.iter().map(|p| p.dot(across)).collect();
    let (lo, hi) = (proj.iter().copied().fold(f64::MAX, f64::min), proj.iter().copied().fold(f64::MIN, f64::max));
    let clip_below = |t: f64| -> Vec<DVec2> {
        let n = poly.len();
        let mut res = Vec::new();
        for i in 0..n {
            let (p, q) = (poly[i], poly[(i + 1) % n]);
            let (fp, fq) = (t - p.dot(across), t - q.dot(across));
            if fp >= 0.0 {
                res.push(p);
            }
            if (fp >= 0.0) != (fq >= 0.0) {
                res.push(p + (q - p) * (fp / (fp - fq)));
            }
        }
        res
    };
    let area = |t: f64| { let c = clip_below(t); if c.len() < 3 { 0.0 } else { signed_area(&c).abs() } };
    let total = signed_area(&poly).abs();
    let mut cuts = Vec::new();
    for k in 1..count {
        let want = total * k as f64 / count as f64;
        let (mut a, mut b) = (lo, hi);
        for _ in 0..60 {
            let m = (a + b) / 2.0;
            if area(m) < want { a = m } else { b = m }
        }
        cuts.push((a + b) / 2.0);
    }
    let floor = cad.floor;
    let d = doc.defaults_for(floor).clone();
    for t in &cuts {
        let o = across * *t;
        for (t0, t1) in housecraft_geom::framing::clip_line(&poly, o, dir) {
            out.created.push(doc.insert(Cad {
                id: Id(0),
                floor,
                layer: cad.layer,
                shape: Shape::Line { a: housecraft_geom::pt(o + dir * t0), b: housecraft_geom::pt(o + dir * t1), arrow_start: false, arrow_end: false },
                color: None,
                fill: None,
                fill_style: None,
            }));
        }
    }
    // Labels: "Lot n" and its area, in each strip.
    let mut bounds = vec![lo - 1.0];
    bounds.extend(cuts.iter().copied());
    bounds.push(hi + 1.0);
    for k in 0..count as usize {
        let below = clip_below(bounds[k + 1]);
        let above_poly: Vec<DVec2> = {
            let n = below.len();
            let mut res = Vec::new();
            for i in 0..n {
                let (p, q) = (below[i], below[(i + 1) % n]);
                let (fp, fq) = (p.dot(across) - bounds[k], q.dot(across) - bounds[k]);
                if fp >= 0.0 {
                    res.push(p);
                }
                if (fp >= 0.0) != (fq >= 0.0) {
                    res.push(p + (q - p) * (fp / (fp - fq)));
                }
            }
            res
        };
        if above_poly.len() < 3 {
            continue;
        }
        let a = signed_area(&above_poly).abs();
        out.created.push(doc.insert(Text {
            id: Id(0),
            floor,
            layer: d.layers.text,
            position: housecraft_geom::pt(centroid(&above_poly)),
            text: format!("Lot {}\n{}", k + 1, housecraft_units::format_area(a, doc.settings.display_unit)),
            style: None,
            rotation: 0.0,
            justify: Justify::Center,
            height: None,
            bold: None,
            color: None,
            leaders: Vec::new(),
            border: false,
            border_margin: None,
            wrap: None,
        }));
    }
    Ok(())
}
