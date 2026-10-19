//! 3D solids and boolean operations on closed CAD shapes.

use housecraft_geom::DVec2;
use housecraft_geom::boolean::{BoolOp, boolean, union_all};
use housecraft_geom::math::{pt, shape_polylines};
use housecraft_model::*;
use housecraft_units::{Length, Point2};

use crate::ApiError;
use crate::command::{Outcome, ShapeOp};
use crate::ops::{self, expect, invalid};

/// A CAD object and its closed outline.
fn outline(doc: &Document, id: Id) -> Result<(Cad, Vec<DVec2>), ApiError> {
    let cad = expect::<Cad>(doc, id)?.clone();
    let poly = shape_polylines(&cad.shape)
        .into_iter()
        .find(|(p, closed)| *closed && p.len() >= 3)
        .ok_or_else(|| ApiError::Invalid(format!("{id} is not a closed shape")))?
        .0;
    Ok((cad, poly))
}

pub(crate) fn cad_boolean(doc: &mut Document, ids: &[Id], op: ShapeOp, out: &mut Outcome) -> Result<(), ApiError> {
    if ids.len() < 2 {
        return invalid("choose at least two closed shapes");
    }
    let shapes = ids.iter().map(|id| outline(doc, *id)).collect::<Result<Vec<_>, _>>()?;
    let polys: Vec<Vec<DVec2>> = shapes.iter().map(|(_, p)| p.clone()).collect();
    let fold = |op: BoolOp| {
        polys[1..].iter().fold(vec![polys[0].clone()], |acc, cutter| {
            acc.iter().flat_map(|r| boolean(r, cutter, op)).collect()
        })
    };
    let result = match op {
        ShapeOp::Union => union_all(&polys),
        ShapeOp::Intersect => fold(BoolOp::Intersect),
        ShapeOp::Subtract => fold(BoolOp::Subtract),
    };
    let first = shapes[0].0.clone();
    ops::delete(doc, ids, out)?;
    for poly in result {
        out.created.push(doc.insert(Cad {
            id: Id(0),
            shape: Shape::Polyline {
                points: poly.into_iter().map(pt).collect(),
                closed: true,
            },
            ..first.clone()
        }));
    }
    Ok(())
}

fn on_floor(doc: &Document, cad: &Cad) -> Result<(), ApiError> {
    if doc.kind_of(cad.floor) == Some(ObjectKind::Floor) {
        Ok(())
    } else {
        invalid("solids are made from shapes drawn on a floor plan")
    }
}

pub(crate) fn extrude(
    doc: &mut Document,
    d: &DefaultSet,
    ids: &[Id],
    height: Option<Length>,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    if ids.is_empty() {
        return invalid("choose closed shapes to extrude");
    }
    let shapes = ids.iter().map(|id| outline(doc, *id)).collect::<Result<Vec<_>, _>>()?;
    for (cad, _) in &shapes {
        on_floor(doc, cad)?;
    }
    ops::delete(doc, ids, out)?;
    for (cad, poly) in shapes {
        out.created.push(doc.insert(Solid {
            id: Id(0),
            floor: cad.floor,
            layer: cad.layer,
            shape: SolidShape::Extrude {
                points: poly.into_iter().map(pt).collect(),
                height: height.unwrap_or(d.solids.height),
            },
            elevation: Length::ZERO,
            color: cad.fill.or(cad.color).unwrap_or(d.solids.color),
        }));
    }
    Ok(())
}

/// The polyline's leftmost x is the axis and its lowest y the base: each
/// point gives (radius, height).
pub(crate) fn revolve(
    doc: &mut Document,
    d: &DefaultSet,
    id: Id,
    sweep: Option<f64>,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let cad = expect::<Cad>(doc, id)?.clone();
    on_floor(doc, &cad)?;
    let pts = shape_polylines(&cad.shape)
        .into_iter()
        .map(|(p, _)| p)
        .find(|p| p.len() >= 2)
        .ok_or_else(|| ApiError::Invalid("draw the profile as a polyline".into()))?;
    let lo = pts.iter().copied().reduce(|a, b| a.min(b)).expect("points");
    let sweep = sweep.unwrap_or(360.0);
    if !(sweep > 0.0 && sweep <= 360.0) {
        return invalid("sweep must be more than 0 and at most 360 degrees");
    }
    ops::delete(doc, &[id], out)?;
    out.created.push(doc.insert(Solid {
        id: Id(0),
        floor: cad.floor,
        layer: cad.layer,
        shape: SolidShape::Revolve {
            center: Point2(Length::from_mm(lo.x), Length::from_mm(lo.y)),
            profile: pts
                .iter()
                .map(|p| [Length::from_mm(p.x - lo.x), Length::from_mm(p.y - lo.y)])
                .collect(),
            sweep,
        },
        elevation: Length::ZERO,
        color: cad.fill.or(cad.color).unwrap_or(d.solids.color),
    }));
    Ok(())
}
