//! Column grids, and the heights drawn in elevation and section views
//! (elevation dimensions and story poles).

use housecraft_geom::v2;
use housecraft_model::*;
use housecraft_units::{Length, Point2};

use crate::{ApiError, Outcome};
use crate::ops::expect;

const GRID_LAYER: &str = "Grid Lines";

/// The plan's "Grid Lines" layer, made on first use.
fn grid_layer(doc: &mut Document) -> Id {
    if let Some(l) = doc.iter::<Layer>().find(|l| l.name == GRID_LAYER) {
        return l.id;
    }
    doc.insert(Layer {
        id: Id(0),
        name: GRID_LAYER.into(),
        color: Some(Color::rgb(90, 90, 160)),
        line_weight: 0.18,
        line_style: LineStyle::DashDot,
        text_style: None,
        in_materials_list: false,
        fill: None,
    })
}

/// Grid labels: 1, 2, 3… and A, B, … Z, AA, AB…
fn letters(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}

pub(crate) fn add_grid_line(doc: &mut Document, floor: Id, a: Point2, b: Point2, out: &mut Outcome) -> Result<(), ApiError> {
    if a.distance(b).mm() < 1.0 {
        return Err(ApiError::Invalid("a grid line needs two different points".into()));
    }
    let layer = grid_layer(doc);
    let line = doc.insert(Cad {
        id: Id(0),
        floor,
        layer,
        shape: Shape::Line { a, b, arrow_start: false, arrow_end: false },
        color: None,
        fill: None,
        fill_style: None,
    });
    // The bubble sits past the line's start.
    let (pa, pb) = (v2(a), v2(b));
    let at = pa - (pb - pa).normalize() * 600.0;
    let bubble = doc.insert(Callout {
        id: Id(0),
        floor,
        layer,
        text: String::new(),
        at: Point2::from_mm(at.x, at.y),
        target: Some(a),
        marker: Some(MarkerShape::Circle),
        height: None,
        link: Some(line),
        note: false,
    });
    out.created.push(line);
    out.created.push(bubble);
    relabel_grid(doc, floor, out);
    Ok(())
}

/// After any edit: drop bubbles whose grid line is gone and renumber
/// grids whose lines changed.
pub(crate) fn refresh(doc: &mut Document, out: &mut Outcome) {
    let Some(layer) = doc.iter::<Layer>().find(|l| l.name == GRID_LAYER).map(|l| l.id) else {
        return;
    };
    let bubbles: Vec<(Id, Id, Option<Id>)> = doc.iter::<Callout>().filter(|c| c.layer == layer).map(|c| (c.id, c.floor, c.link)).collect();
    let mut floors: Vec<Id> = Vec::new();
    for (id, floor, link) in bubbles {
        let touched = |x: &Id| out.created.contains(x) || out.modified.contains(x) || out.deleted.contains(x);
        if link.is_none_or(|l| doc.get::<Cad>(l).is_none()) {
            doc.remove(id);
            out.deleted.push(id);
        } else if !link.is_some_and(|l| touched(&l)) && !touched(&id) {
            continue;
        }
        if !floors.contains(&floor) {
            floors.push(floor);
        }
    }
    for f in floors {
        relabel_grid(doc, f, out);
    }
}

/// Number the floor's grid lines: mostly vertical ones 1, 2, 3 left to right;
/// mostly horizontal ones A, B, C bottom to top.
pub(crate) fn relabel_grid(doc: &mut Document, floor: Id, out: &mut Outcome) {
    let Some(layer) = doc.iter::<Layer>().find(|l| l.name == GRID_LAYER).map(|l| l.id) else {
        return;
    };
    let mut columns: Vec<(f64, Id)> = Vec::new();
    let mut rows: Vec<(f64, Id)> = Vec::new();
    for c in doc.iter::<Callout>().filter(|c| c.floor == floor && c.layer == layer) {
        let Some(Shape::Line { a, b, .. }) = c.link.and_then(|l| doc.get::<Cad>(l)).map(|l| l.shape.clone()) else {
            continue;
        };
        let (a, b) = (v2(a), v2(b));
        let mid = (a + b) / 2.0;
        if (b - a).y.abs() >= (b - a).x.abs() {
            columns.push((mid.x, c.id));
        } else {
            rows.push((mid.y, c.id));
        }
    }
    columns.sort_by(|p, q| p.0.total_cmp(&q.0));
    rows.sort_by(|p, q| p.0.total_cmp(&q.0));
    let labels = columns
        .iter()
        .enumerate()
        .map(|(i, (_, id))| (*id, (i + 1).to_string()))
        .chain(rows.iter().enumerate().map(|(i, (_, id))| (*id, letters(i))));
    for (id, text) in labels.collect::<Vec<_>>() {
        if let Some(c) = doc.get_mut::<Callout>(id)
            && c.text != text
        {
            c.text = text;
            if !out.created.contains(&id) {
                out.modified.push(id);
            }
        }
    }
}

/// A level worth marking in a view: name and height (model mm).
fn levels(doc: &Document) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for f in doc.floors_by_elevation() {
        let z = f.elevation.mm();
        out.push((f.name.clone(), z));
        if f.foundation.is_some() {
            continue;
        }
        out.push((format!("{} ceiling", f.name), z + f.ceiling_height.mm()));
    }
    let ridge = doc
        .iter::<RoofPlane>()
        .filter(|r| !r.gable)
        .flat_map(|r| r.outline.iter().map(|p| p.mm()[2]))
        .fold(f64::MIN, f64::max);
    if ridge > f64::MIN {
        out.push(("Ridge".into(), ridge));
    }
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out.dedup_by(|a, b| (a.1 - b.1).abs() < 1.0);
    out
}

/// Where the building starts along the view's line (its left end).
fn left_of_building(doc: &Document, v: &SectionView) -> f64 {
    let (a, b) = (v2(v.a), v2(v.b));
    let dir = (b - a).normalize_or_zero();
    doc.iter::<Wall>()
        .flat_map(|w| [v2(w.start), v2(w.end)])
        .map(|p| (p - a).dot(dir))
        .fold(f64::MAX, f64::min)
}

pub(crate) fn elevation_dimensions(doc: &mut Document, view: Id, out: &mut Outcome) -> Result<(), ApiError> {
    let v = expect::<SectionView>(doc, view)?.clone();
    let left = left_of_building(doc, &v);
    if left == f64::MAX {
        return Err(ApiError::Invalid("there are no walls to dimension".into()));
    }
    let levels = levels(doc);
    if levels.len() < 2 {
        return Err(ApiError::Invalid("nothing to dimension in this view".into()));
    }
    let d = doc.defaults_for(view).clone();
    let x = left - 1200.0;
    let mut add = |points: Vec<Point2>, offset: f64| {
        out.created.push(doc.insert(Dimension {
            id: Id(0),
            floor: view,
            layer: d.layers.dimensions,
            geometry: DimensionGeometry::Linear { points, orientation: DimOrientation::Vertical, offset: Length::from_mm(offset) },
            text_override: None,
            auto: false,
            style: d.dimension_style.clone(),
        }));
    };
    // Each level, then the overall height further out.
    add(levels.iter().map(|(_, z)| Point2::from_mm(x, *z)).collect(), 0.0);
    add(vec![Point2::from_mm(x, levels[0].1), Point2::from_mm(x, levels[levels.len() - 1].1)], -900.0);
    Ok(())
}

pub(crate) fn story_pole(doc: &mut Document, view: Id, out: &mut Outcome) -> Result<(), ApiError> {
    let v = expect::<SectionView>(doc, view)?.clone();
    let left = left_of_building(doc, &v);
    if left == f64::MAX {
        return Err(ApiError::Invalid("there are no walls to measure".into()));
    }
    let levels = levels(doc);
    if levels.is_empty() {
        return Err(ApiError::Invalid("nothing to mark in this view".into()));
    }
    let d = doc.defaults_for(view).clone();
    let unit = doc.settings.display_unit;
    let x = left - 3600.0;
    let (z0, z1) = (levels[0].1, levels[levels.len() - 1].1);
    let pt = |x: f64, z: f64| Point2::from_mm(x, z);
    let cad = |doc: &mut Document, out: &mut Outcome, a: Point2, b: Point2| {
        out.created.push(doc.insert(Cad {
            id: Id(0),
            floor: view,
            layer: d.layers.cad,
            shape: Shape::Line { a, b, arrow_start: false, arrow_end: false },
            color: None,
            fill: None,
            fill_style: None,
        }));
    };
    // The staff, then a mark and a label at every level.
    cad(doc, out, pt(x, z0 - 300.0), pt(x, z1 + 300.0));
    for (name, z) in &levels {
        cad(doc, out, pt(x - 250.0, *z), pt(x + 250.0, *z));
        out.created.push(doc.insert(Text {
            id: Id(0),
            floor: view,
            layer: d.layers.text,
            position: pt(x - 400.0, *z),
            text: format!("{name}  {}", Length::from_mm(*z).format(unit)),
            style: None,
            rotation: 0.0,
            justify: Justify::Right,
            height: None,
            bold: None,
            color: None,
            leaders: Vec::new(),
            border: false,
            wrap: None,
        }));
    }
    Ok(())
}

/// Datum markers at each storey's framing levels, right of the building.
pub(crate) fn framing_markers(doc: &mut Document, view: Id, out: &mut Outcome) -> Result<(), ApiError> {
    let v = expect::<SectionView>(doc, view)?.clone();
    let (a, b) = (v2(v.a), v2(v.b));
    let dir = (b - a).normalize_or_zero();
    let right = doc
        .iter::<Wall>()
        .flat_map(|w| [v2(w.start), v2(w.end)])
        .map(|p| (p - a).dot(dir))
        .fold(f64::MIN, f64::max);
    if right == f64::MIN {
        return Err(ApiError::Invalid("there are no walls to mark".into()));
    }
    let d = doc.defaults_for(view).clone();
    let fr = doc.active_defaults().framing.clone();
    let unit = doc.settings.display_unit;
    let x = right + 1500.0;
    let mut levels: Vec<(String, f64)> = Vec::new();
    for f in doc.floors_by_elevation() {
        let z = f.elevation.mm();
        if f.foundation.is_none() {
            levels.push((format!("{} T.O. joists", f.name), z - 19.0));
            levels.push((format!("{} T.O. subfloor", f.name), z));
            let plates = fr.top_plates.max(1) as f64 * fr.lumber_width.mm();
            levels.push((format!("{} T.O. plate", f.name), z + f.ceiling_height.mm()));
            levels.push((format!("{} T.O. studs", f.name), z + f.ceiling_height.mm() - plates));
        }
    }
    for (name, z) in levels {
        let s = 150.0;
        out.created.push(doc.insert(Cad {
            id: Id(0),
            floor: view,
            layer: d.layers.cad,
            shape: Shape::Polyline {
                points: vec![Point2::from_mm(x, z), Point2::from_mm(x - s, z + s * 1.4), Point2::from_mm(x + s, z + s * 1.4)],
                closed: true,
            },
            color: None,
            fill: None,
            fill_style: None,
        }));
        out.created.push(doc.insert(Cad {
            id: Id(0),
            floor: view,
            layer: d.layers.cad,
            shape: Shape::Line { a: Point2::from_mm(x - 600.0, z), b: Point2::from_mm(x + 300.0, z), arrow_start: false, arrow_end: false },
            color: None,
            fill: None,
            fill_style: None,
        }));
        out.created.push(doc.insert(Text {
            id: Id(0),
            floor: view,
            layer: d.layers.text,
            position: Point2::from_mm(x + 400.0, z),
            text: format!("{name}  {}", Length::from_mm(z).format(unit)),
            style: None,
            rotation: 0.0,
            justify: Justify::Left,
            height: None,
            bold: None,
            color: None,
            leaders: Vec::new(),
            border: false,
            wrap: None,
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::letters;

    #[test]
    fn grid_letters_roll_over() {
        assert_eq!([letters(0), letters(25), letters(26), letters(27)], ["A", "Z", "AA", "AB"]);
    }
}
