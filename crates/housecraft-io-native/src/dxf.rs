//! Reading 2D DXF drawings (ASCII): lines, polylines, circles, arcs and
//! text become HouseCraft CAD shapes and notes. Blocks (INSERT) and
//! hatches are skipped.

use housecraft_model::Shape;
use housecraft_units::{Length, Point2};

/// A note read from the DXF.
#[derive(Debug, Clone, PartialEq)]
pub struct DxfText {
    pub layer: String,
    pub at: Point2,
    pub text: String,
    pub height: Length,
    pub rotation: f64,
}

/// What a DXF file holds, in model millimetres.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DxfDrawing {
    pub shapes: Vec<(String, Shape)>,
    pub texts: Vec<DxfText>,
    /// Entities that were not understood, by type, with counts.
    pub skipped: Vec<(String, usize)>,
    /// Millimetres per drawing unit used.
    pub unit_mm: f64,
}

/// Millimetres per unit for a `$INSUNITS` code; inches when unset.
fn unit_mm(code: i32) -> f64 {
    match code {
        2 => 304.8,
        4 => 1.0,
        5 => 10.0,
        6 => 1000.0,
        _ => 25.4,
    }
}

/// Parse DXF text. `unit_mm` overrides the file's own units.
pub fn read(text: &str, unit_override: Option<f64>) -> Result<DxfDrawing, String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    if lines.len() < 2 {
        return Err("not a DXF file".into());
    }
    let pairs: Vec<(i32, &str)> = lines
        .chunks(2)
        .filter(|c| c.len() == 2)
        .filter_map(|c| c[0].parse::<i32>().ok().map(|k| (k, c[1])))
        .collect();
    if !pairs.iter().any(|(k, v)| *k == 0 && *v == "SECTION") {
        return Err("not a DXF file (no SECTION)".into());
    }
    // Units from the header.
    let mut units = 0;
    if let Some(i) = pairs.iter().position(|(k, v)| *k == 9 && *v == "$INSUNITS")
        && let Some((70, v)) = pairs.get(i + 1)
    {
        units = v.parse().unwrap_or(0);
    }
    let k = unit_override.unwrap_or(unit_mm(units));
    let pt = |x: f64, y: f64| Point2(Length::from_mm(x * k), Length::from_mm(y * k));

    // Entities: from "2 ENTITIES" to its ENDSEC, split at each code 0.
    let Some(start) = pairs.iter().position(|(c, v)| *c == 2 && *v == "ENTITIES") else {
        return Ok(DxfDrawing { unit_mm: k, ..Default::default() });
    };
    let mut entities: Vec<(&str, Vec<(i32, &str)>)> = Vec::new();
    for &(code, value) in &pairs[start + 1..] {
        if code == 0 {
            if value == "ENDSEC" {
                break;
            }
            entities.push((value, Vec::new()));
        } else if let Some(e) = entities.last_mut() {
            e.1.push((code, value));
        }
    }

    let mut out = DxfDrawing { unit_mm: k, ..Default::default() };
    let mut skip = |name: &str| match out.skipped.iter_mut().find(|(n, _)| n == name) {
        Some(s) => s.1 += 1,
        None => out.skipped.push((name.to_string(), 1)),
    };
    let mut shapes = Vec::new();
    let mut texts = Vec::new();
    let mut i = 0;
    while i < entities.len() {
        let (kind, codes) = &entities[i];
        let get = |c: i32| codes.iter().find(|(k, _)| *k == c).and_then(|(_, v)| v.parse::<f64>().ok());
        let layer = codes.iter().find(|(c, _)| *c == 8).map_or("0", |(_, v)| v).to_string();
        let flags = get(70).unwrap_or(0.0) as i32;
        match *kind {
            "LINE" => {
                if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (get(10), get(20), get(11), get(21)) {
                    shapes.push((layer, Shape::Line { a: pt(x1, y1), b: pt(x2, y2), arrow_start: false, arrow_end: false }));
                }
            }
            "LWPOLYLINE" => {
                let xs: Vec<f64> = codes.iter().filter(|(c, _)| *c == 10).filter_map(|(_, v)| v.parse().ok()).collect();
                let ys: Vec<f64> = codes.iter().filter(|(c, _)| *c == 20).filter_map(|(_, v)| v.parse().ok()).collect();
                let points: Vec<Point2> = xs.iter().zip(&ys).map(|(x, y)| pt(*x, *y)).collect();
                if points.len() >= 2 {
                    shapes.push((layer, Shape::Polyline { points, closed: flags & 1 == 1 }));
                }
            }
            "POLYLINE" => {
                let mut points = Vec::new();
                i += 1;
                while i < entities.len() && entities[i].0 == "VERTEX" {
                    let v = &entities[i].1;
                    let g = |c: i32| v.iter().find(|(k, _)| *k == c).and_then(|(_, s)| s.parse::<f64>().ok());
                    if let (Some(x), Some(y)) = (g(10), g(20)) {
                        points.push(pt(x, y));
                    }
                    i += 1;
                }
                if points.len() >= 2 {
                    shapes.push((layer, Shape::Polyline { points, closed: flags & 1 == 1 }));
                }
                // `i` now sits on SEQEND (or past the end).
            }
            "CIRCLE" => {
                if let (Some(x), Some(y), Some(r)) = (get(10), get(20), get(40)) {
                    shapes.push((layer, Shape::Circle { center: pt(x, y), radius: Length::from_mm(r * k) }));
                }
            }
            "ARC" => {
                if let (Some(x), Some(y), Some(r), Some(a0), Some(a1)) = (get(10), get(20), get(40), get(50), get(51)) {
                    let sweep = (a1 - a0).rem_euclid(360.0);
                    shapes.push((
                        layer,
                        Shape::Arc {
                            center: pt(x, y),
                            radius: Length::from_mm(r * k),
                            start_angle: a0,
                            sweep: if sweep == 0.0 { 360.0 } else { sweep },
                        },
                    ));
                }
            }
            "POINT" => {
                if let (Some(x), Some(y)) = (get(10), get(20)) {
                    shapes.push((layer, Shape::Point { at: pt(x, y) }));
                }
            }
            "TEXT" | "MTEXT" => {
                let raw: String = codes
                    .iter()
                    .filter(|(c, _)| *c == 3 || *c == 1)
                    .map(|(_, v)| *v)
                    .collect();
                let text = if *kind == "MTEXT" { plain_mtext(&raw) } else { raw };
                // Aligned text keeps its anchor in 11/21 when justified.
                let (x, y) = match (get(72).unwrap_or(0.0) as i32, get(11), get(21)) {
                    (j, Some(x), Some(y)) if j != 0 => (x, y),
                    _ => (get(10).unwrap_or(0.0), get(20).unwrap_or(0.0)),
                };
                if !text.trim().is_empty() {
                    texts.push(DxfText {
                        layer,
                        at: pt(x, y),
                        text,
                        height: Length::from_mm(get(40).unwrap_or(1.0) * k),
                        rotation: get(50).unwrap_or(0.0),
                    });
                }
            }
            "SEQEND" | "VERTEX" => {}
            other => skip(other),
        }
        i += 1;
    }
    out.shapes = shapes;
    out.texts = texts;
    Ok(out)
}

/// Survey data in a DXF: contour lines (level polylines and lines),
/// break lines (3D polylines whose height changes), and spot points, in
/// model millimetres (heights too).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DxfTerrain {
    pub contours: Vec<(Vec<Point2>, Length)>,
    pub breaks: Vec<(Vec<Point2>, Vec<Length>)>,
    pub points: Vec<(Point2, Length)>,
}

/// Read the survey data (anything with a height) from DXF text.
pub fn read_terrain(text: &str, unit_override: Option<f64>) -> Result<DxfTerrain, String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let pairs: Vec<(i32, &str)> = lines
        .chunks(2)
        .filter(|c| c.len() == 2)
        .filter_map(|c| c[0].parse::<i32>().ok().map(|k| (k, c[1])))
        .collect();
    if !pairs.iter().any(|(k, v)| *k == 0 && *v == "SECTION") {
        return Err("not a DXF file (no SECTION)".into());
    }
    let mut units = 0;
    if let Some(i) = pairs.iter().position(|(k, v)| *k == 9 && *v == "$INSUNITS")
        && let Some((70, v)) = pairs.get(i + 1)
    {
        units = v.parse().unwrap_or(0);
    }
    let k = unit_override.unwrap_or(unit_mm(units));
    let pt = |x: f64, y: f64| Point2(Length::from_mm(x * k), Length::from_mm(y * k));
    let len = |z: f64| Length::from_mm(z * k);
    let Some(start) = pairs.iter().position(|(c, v)| *c == 2 && *v == "ENTITIES") else {
        return Ok(DxfTerrain::default());
    };
    let mut entities: Vec<(&str, Vec<(i32, &str)>)> = Vec::new();
    for &(code, value) in &pairs[start + 1..] {
        if code == 0 {
            if value == "ENDSEC" {
                break;
            }
            entities.push((value, Vec::new()));
        } else if let Some(e) = entities.last_mut() {
            e.1.push((code, value));
        }
    }
    let mut out = DxfTerrain::default();
    // A line of points with heights: a contour when level, else a break.
    let push_line = |out: &mut DxfTerrain, pts: Vec<Point2>, zs: Vec<f64>| {
        if pts.len() < 2 {
            return;
        }
        let (lo, hi) = zs.iter().fold((f64::MAX, f64::MIN), |(a, b), z| (a.min(*z), b.max(*z)));
        if hi - lo < 1e-6 {
            if lo.abs() > 1e-9 {
                out.contours.push((pts, len(lo)));
            }
        } else {
            out.breaks.push((pts, zs.iter().map(|z| len(*z)).collect()));
        }
    };
    let mut i = 0;
    while i < entities.len() {
        let (kind, codes) = &entities[i];
        let get = |c: i32| codes.iter().find(|(k, _)| *k == c).and_then(|(_, v)| v.parse::<f64>().ok());
        match *kind {
            "LINE" => {
                if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (get(10), get(20), get(11), get(21)) {
                    push_line(&mut out, vec![pt(x1, y1), pt(x2, y2)], vec![get(30).unwrap_or(0.0), get(31).unwrap_or(0.0)]);
                }
            }
            "LWPOLYLINE" => {
                let xs: Vec<f64> = codes.iter().filter(|(c, _)| *c == 10).filter_map(|(_, v)| v.parse().ok()).collect();
                let ys: Vec<f64> = codes.iter().filter(|(c, _)| *c == 20).filter_map(|(_, v)| v.parse().ok()).collect();
                let z = get(38).unwrap_or(0.0);
                let pts: Vec<Point2> = xs.iter().zip(&ys).map(|(x, y)| pt(*x, *y)).collect();
                let n = pts.len();
                push_line(&mut out, pts, vec![z; n]);
            }
            "POLYLINE" => {
                let (mut pts, mut zs) = (Vec::new(), Vec::new());
                i += 1;
                while i < entities.len() && entities[i].0 == "VERTEX" {
                    let v = &entities[i].1;
                    let g = |c: i32| v.iter().find(|(k, _)| *k == c).and_then(|(_, s)| s.parse::<f64>().ok());
                    if let (Some(x), Some(y)) = (g(10), g(20)) {
                        pts.push(pt(x, y));
                        zs.push(g(30).unwrap_or(0.0));
                    }
                    i += 1;
                }
                push_line(&mut out, pts, zs);
            }
            "POINT" => {
                if let (Some(x), Some(y), Some(z)) = (get(10), get(20), get(30)) {
                    out.points.push((pt(x, y), len(z)));
                }
            }
            _ => {}
        }
        i += 1;
    }
    Ok(out)
}

/// MTEXT without its formatting codes: `\P` is a new line; `{\fArial;…}`
/// style groups and `\X…;` codes are dropped.
fn plain_mtext(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('P') | Some('p') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some('{') => out.push('{'),
                Some('}') => out.push('}'),
                Some(_) => {
                    // Skip to the ';' that ends a formatting code.
                    for c in chars.by_ref() {
                        if c == ';' {
                            break;
                        }
                    }
                }
                None => {}
            },
            '{' | '}' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_entities_in_inches() {
        let dxf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
0\nLINE\n8\nWALLS\n10\n0\n20\n0\n11\n10\n21\n0\n\
0\nLWPOLYLINE\n8\nA\n70\n1\n10\n0\n20\n0\n10\n1\n20\n0\n10\n1\n20\n1\n\
0\nCIRCLE\n8\n0\n10\n5\n20\n5\n40\n2\n\
0\nMTEXT\n8\nT\n10\n1\n20\n2\n40\n0.5\n1\n{\\fArial;Hello}\\Pworld\n\
0\nINSERT\n8\n0\n0\nENDSEC\n0\nEOF\n";
        let d = read(dxf, None).unwrap();
        assert_eq!(d.unit_mm, 25.4);
        assert_eq!(d.shapes.len(), 3);
        assert!(matches!(&d.shapes[0].1, Shape::Line { b, .. } if (b.0.mm() - 254.0).abs() < 1e-9));
        assert!(matches!(&d.shapes[1].1, Shape::Polyline { closed: true, points } if points.len() == 3));
        assert_eq!(d.texts[0].text, "Hello\nworld");
        assert_eq!(d.skipped, [("INSERT".to_string(), 1)]);
    }
}

#[cfg(test)]
mod terrain_tests {
    use super::*;

    #[test]
    fn survey_lines_points_and_breaks() {
        let dxf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n2\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
0\nLWPOLYLINE\n8\nTOPO\n90\n2\n38\n102\n10\n0\n20\n0\n10\n50\n20\n0\n\
0\nLINE\n8\nTOPO\n10\n0\n20\n10\n30\n100\n11\n50\n21\n10\n31\n104\n\
0\nPOINT\n8\nTOPO\n10\n5\n20\n5\n30\n101.5\n\
0\nLINE\n8\nFLAT\n10\n0\n20\n0\n11\n1\n21\n1\n0\nENDSEC\n0\nEOF\n";
        let t = read_terrain(dxf, None).unwrap();
        assert_eq!(t.contours.len(), 1);
        assert!((t.contours[0].1.mm() - 102.0 * 304.8).abs() < 1e-6);
        assert_eq!(t.breaks.len(), 1);
        assert!((t.breaks[0].1[1].mm() - 104.0 * 304.8).abs() < 1e-6);
        assert_eq!(t.points.len(), 1);
    }
}
