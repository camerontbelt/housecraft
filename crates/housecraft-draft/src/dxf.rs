//! 2D drawings as DXF (R12 ASCII), which every CAD program reads: lines,
//! polylines and text, on one DXF layer per HouseCraft layer.

use std::fmt::Write;

use housecraft_geom::DVec2;
use housecraft_model::{Document, Justify, Layer};

use crate::{Drawing, Prim};

/// DXF units for the drawing: inches for imperial plans, else millimetres.
pub struct DxfOptions {
    pub imperial: bool,
}

/// A DXF layer name: characters DXF forbids become `_`.
fn layer_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if "<>/\\\":;?*|=`,".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    if s.trim().is_empty() { "0".into() } else { s }
}

/// `drawing` as a DXF file. Image prims are left out.
pub fn to_dxf(doc: &Document, drawing: &Drawing, opts: &DxfOptions) -> String {
    let k = if opts.imperial { 1.0 / 25.4 } else { 1.0 };
    let layer_of = |object: Option<housecraft_model::Id>| {
        object
            .and_then(|o| doc.layer_of(o))
            .and_then(|l| doc.get::<Layer>(l))
            .map_or_else(|| "0".to_string(), |l| layer_name(&l.name))
    };
    let mut layers: Vec<String> = vec!["0".into()];
    for item in &drawing.items {
        let l = layer_of(item.object);
        if !layers.contains(&l) {
            layers.push(l);
        }
    }

    let mut s = String::new();
    let mut pair = |code: i32, value: &str| {
        let _ = write!(s, "{code}\r\n{value}\r\n");
    };
    let num = |v: f64| format!("{:.6}", v);
    pair(0, "SECTION");
    pair(2, "HEADER");
    pair(9, "$ACADVER");
    pair(1, "AC1009");
    pair(9, "$INSUNITS");
    pair(70, if opts.imperial { "1" } else { "4" });
    pair(0, "ENDSEC");
    pair(0, "SECTION");
    pair(2, "TABLES");
    pair(0, "TABLE");
    pair(2, "LAYER");
    pair(70, &layers.len().to_string());
    for l in &layers {
        pair(0, "LAYER");
        pair(2, l);
        pair(70, "0");
        pair(62, "7");
        pair(6, "CONTINUOUS");
    }
    pair(0, "ENDTAB");
    pair(0, "ENDSEC");
    pair(0, "SECTION");
    pair(2, "ENTITIES");
    let point = |pair: &mut dyn FnMut(i32, &str), base: i32, p: DVec2| {
        pair(base, &num(p.x * k));
        pair(base + 10, &num(p.y * k));
        pair(base + 20, "0.0");
    };
    for item in &drawing.items {
        let layer = layer_of(item.object);
        match &item.prim {
            Prim::Polyline { points, .. } | Prim::Polygon { points, .. } if points.len() >= 2 => {
                let closed = match &item.prim {
                    Prim::Polyline { closed, .. } => *closed,
                    _ => true,
                };
                if points.len() == 2 && !closed {
                    pair(0, "LINE");
                    pair(8, &layer);
                    point(&mut pair, 10, points[0]);
                    point(&mut pair, 11, points[1]);
                } else {
                    pair(0, "POLYLINE");
                    pair(8, &layer);
                    pair(66, "1");
                    pair(70, if closed { "1" } else { "0" });
                    for p in points {
                        pair(0, "VERTEX");
                        pair(8, &layer);
                        point(&mut pair, 10, *p);
                    }
                    pair(0, "SEQEND");
                    pair(8, &layer);
                }
            }
            Prim::Text { at, text, height, rotation, justify, middle, .. } => {
                for (i, line) in text.lines().enumerate() {
                    let down = DVec2::from_angle((rotation - 90.0).to_radians()) * height * 1.4 * i as f64;
                    let p = *at + down;
                    pair(0, "TEXT");
                    pair(8, &layer);
                    point(&mut pair, 10, p);
                    pair(40, &num(height * k));
                    pair(1, line);
                    if *rotation != 0.0 {
                        pair(50, &num(*rotation));
                    }
                    let h = match justify {
                        Justify::Left => 0,
                        Justify::Center => 1,
                        Justify::Right => 2,
                    };
                    if h != 0 || *middle {
                        pair(72, &h.to_string());
                        point(&mut pair, 11, p);
                        if *middle {
                            pair(73, "2");
                        }
                    }
                }
            }
            _ => {}
        }
    }
    pair(0, "ENDSEC");
    pair(0, "EOF");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ink, Stroke};

    #[test]
    fn writes_lines_polylines_and_text() {
        let doc = Document::new("t");
        let mut d = Drawing::default();
        let stroke = Stroke { ink: Ink::Default, width: 0.25, dash: &[] };
        d.push(None, Prim::Polyline { points: vec![DVec2::ZERO, DVec2::new(254.0, 0.0)], closed: false, stroke });
        d.push(None, Prim::Polyline { points: vec![DVec2::ZERO, DVec2::X, DVec2::ONE], closed: true, stroke });
        d.push(
            None,
            Prim::Text { at: DVec2::ZERO, text: "Hi".into(), height: 25.4, rotation: 0.0, justify: Justify::Left, middle: false, bold: false, ink: Ink::Default },
        );
        let s = to_dxf(&doc, &d, &DxfOptions { imperial: true });
        assert!(s.contains("LINE\r\n8\r\n0\r\n10\r\n0.000000\r\n20\r\n0.000000\r\n30\r\n0.0\r\n11\r\n10.000000"));
        assert!(s.contains("POLYLINE") && s.contains("SEQEND") && s.contains("TEXT"));
        assert!(s.trim_end().ends_with("EOF"));
    }
}
