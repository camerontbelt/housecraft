//! Turning plan objects into one 3D symbol.

use housecraft_geom::mesh::build;
use housecraft_model::*;
use housecraft_units::{Length, Point2};

use crate::ApiError;
use crate::command::Outcome;
use crate::ops::{self, invalid};

/// Replace `ids` with a placed copy of a new symbol made from their 3D
/// shapes (coloured as they look now).
pub(crate) fn convert(doc: &mut Document, ids: &[Id], name: &str, out: &mut Outcome) -> Result<(), ApiError> {
    if ids.is_empty() {
        return invalid("choose the objects to make into a symbol");
    }
    for id in ids {
        if doc.object(*id).is_none() {
            return Err(ApiError::NotFound(*id));
        }
    }
    let floor = ids
        .iter()
        .filter_map(|id| doc.floor_of(*id))
        .find(|f| doc.kind_of(*f) == Some(ObjectKind::Floor))
        .ok_or_else(|| ApiError::Invalid("the objects must be on a floor plan".into()))?;
    let floor_z = doc.get::<Floor>(floor).map_or(0.0, |f| f.elevation.mm());
    let mesh = build(doc, Some(floor));
    // Every triangle of the chosen objects, in millimetres, with its colour.
    let mut tris: Vec<([[f64; 3]; 3], [u8; 3])> = Vec::new();
    for part in mesh.parts.iter().filter(|p| ids.contains(&p.object)) {
        let range = part.first_index as usize..(part.first_index + part.index_count) as usize;
        for t in mesh.indices[range].as_chunks::<3>().0 {
            let p = |i: u32| mesh.positions[i as usize].map(|v| v as f64 * 1000.0);
            let c = mesh.colors[t[0] as usize];
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            tris.push(([p(t[0]), p(t[1]), p(t[2])], [byte(c[0]), byte(c[1]), byte(c[2])]));
        }
    }
    if tris.is_empty() {
        return invalid("the selection has nothing in 3D to make a symbol from");
    }
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for p in tris.iter().flat_map(|(t, _)| t) {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    // Symbols are stored front-facing −y with the origin at the bottom
    // centre of the back; placed facing 270° the plan is unchanged.
    let origin = [(lo[0] + hi[0]) / 2.0, hi[1], lo[2]];
    tris.sort_by_key(|(_, c)| *c);
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    let mut groups: Vec<SymbolGroup> = Vec::new();
    for (i, (t, c)) in tris.iter().enumerate() {
        let base = positions.len() as u32;
        positions.extend(t.iter().map(|p| [0, 1, 2].map(|k| (p[k] - origin[k]) as f32)));
        triangles.push([base, base + 1, base + 2]);
        let color = Color::rgb(c[0], c[1], c[2]);
        match groups.last_mut() {
            Some(g) if g.color == color => g.count += 1,
            _ => groups.push(SymbolGroup {
                first: i as u32,
                count: 1,
                color,
                name: String::new(),
            }),
        }
    }
    let layer = ids
        .iter()
        .find_map(|id| doc.layer_of(*id))
        .unwrap_or(doc.active_defaults().layers.cad);
    ops::delete(doc, ids, out)?;
    let def = doc.insert(SymbolDef {
        id: Id(0),
        name: if name.trim().is_empty() { "Symbol".into() } else { name.to_string() },
        positions,
        triangles,
        groups,
        behavior: SymbolBehavior::Plain,
    });
    out.created.push(def);
    let len = Length::from_mm;
    out.created.push(doc.insert(Symbol {
        id: Id(0),
        floor,
        layer,
        def,
        position: Point2(len(origin[0]), len(origin[1])),
        facing: 270.0,
        elevation: len(lo[2] - floor_z),
        width: len(hi[0] - lo[0]),
        depth: len(hi[1] - lo[1]),
        height: len(hi[2] - lo[2]),
        renovation: Renovation::default(),
    }));
    Ok(())
}
