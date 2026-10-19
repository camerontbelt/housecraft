//! Reading survey points for the terrain.

use housecraft_units::DisplayUnit;

use crate::ApiError;

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
