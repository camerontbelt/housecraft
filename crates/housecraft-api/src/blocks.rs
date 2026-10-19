//! CAD blocks: named groups of CAD shapes placed as single objects.

use housecraft_geom::math::{bounds, pt, shape_polylines};
use housecraft_geom::transform::Transform2;
use housecraft_geom::{DVec2, v2};
use housecraft_model::*;
use housecraft_units::Point2;

use crate::ApiError;
use crate::command::Outcome;
use crate::ops::{self, expect, invalid};

/// Make block `name` from CAD shapes `ids` (moved into the block and
/// replaced by a placed copy) or from `shapes` given relative to the block's
/// insertion point.
pub(crate) fn add_block(
    doc: &mut Document,
    d: &DefaultSet,
    name: &str,
    ids: &[Id],
    base: Option<Point2>,
    shapes: &[DetailShape],
    out: &mut Outcome,
) -> Result<(), ApiError> {
    if name.trim().is_empty() {
        return invalid("a block needs a name");
    }
    if doc.iter::<Block>().any(|b| b.name == name) {
        return invalid(format!("there is already a block named {name:?}"));
    }
    if ids.is_empty() == shapes.is_empty() {
        return invalid("make a block from either `ids` (CAD shapes) or `shapes`");
    }
    let mut cads = Vec::new();
    for id in ids {
        let c = expect::<Cad>(doc, *id)?;
        if doc.kind_of(c.floor) == Some(ObjectKind::Block) {
            return invalid(format!("{id} is already inside a block"));
        }
        cads.push(c.clone());
    }
    let sheet = cads.first().map(|c| c.floor);
    if cads.iter().any(|c| Some(c.floor) != sheet) {
        return invalid("the shapes for a block must all be on the same floor or view");
    }
    let base = base.map(v2).unwrap_or_else(|| {
        bounds(cads.iter().flat_map(|c| {
            shape_polylines(&c.shape)
                .into_iter()
                .flat_map(|(points, _)| points)
        }))
        .map_or(DVec2::ZERO, |(lo, _)| lo)
    });
    let block = doc.insert(Block {
        id: Id(0),
        name: name.to_string(),
    });
    out.created.push(block);
    let to_local = Transform2::translate(-base);
    for c in &cads {
        let cad = doc.get_mut::<Cad>(c.id).expect("checked above");
        cad.floor = block;
        cad.shape = to_local.apply_shape(&c.shape);
        out.modified.push(c.id);
    }
    for s in shapes {
        doc.insert(Cad {
            id: Id(0),
            floor: block,
            layer: d.layers.cad,
            shape: s.shape.clone(),
            color: s.color,
            fill: s.fill,
            fill_style: None,
        });
    }
    if let Some(sheet) = sheet {
        let layer = cads[0].layer;
        out.created.push(doc.insert(BlockRef {
            id: Id(0),
            floor: sheet,
            layer,
            block,
            at: pt(base),
            rotation: 0.0,
            scale: 1.0,
            mirrored: false,
        }));
    }
    Ok(())
}

/// Block `id`'s shapes, relative to its insertion point (to save in a library).
pub fn block_shapes(doc: &Document, id: Id) -> Vec<DetailShape> {
    doc.iter::<Cad>()
        .filter(|c| c.floor == id)
        .map(|c| DetailShape {
            shape: c.shape.clone(),
            color: c.color,
            fill: c.fill,
        })
        .collect()
}

/// Where a placed block's shapes land.
pub fn placement(b: &BlockRef) -> Transform2 {
    Transform2::placement(v2(b.at), b.rotation, b.scale, b.mirrored)
}

/// Replace placed block `id` with copies of its shapes.
pub(crate) fn explode(doc: &mut Document, id: Id, out: &mut Outcome) -> Result<(), ApiError> {
    let b = expect::<BlockRef>(doc, id)?.clone();
    let t = placement(&b);
    let parts: Vec<Cad> = doc.iter::<Cad>().filter(|c| c.floor == b.block).cloned().collect();
    for c in parts {
        out.created.push(doc.insert(Cad {
            id: Id(0),
            floor: b.floor,
            shape: t.apply_shape(&c.shape),
            ..c
        }));
    }
    ops::delete(doc, &[id], out)
}
