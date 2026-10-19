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
