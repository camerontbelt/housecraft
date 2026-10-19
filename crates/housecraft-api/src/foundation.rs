//! Foundations: a floor under the lowest storey whose concrete walls follow
//! that storey's exterior outline, rebuilt automatically when it changes.

use housecraft_geom::math::{inset_polygon, simplify_collinear};
use housecraft_geom::{pt, rooms};
use housecraft_model::*;
use housecraft_units::{DisplayUnit, Length, Point2};

use crate::ApiError;
use crate::command::Outcome;

/// The lowest ordinary floor, which the foundation supports.
fn supported_floor(doc: &Document) -> Option<Floor> {
    doc.floors_by_elevation()
        .into_iter()
        .find(|f| f.foundation.is_none())
        .cloned()
}

fn foundation_floor(doc: &Document) -> Option<Floor> {
    doc.iter::<Floor>()
        .find(|f| f.foundation.is_some())
        .cloned()
}

/// Wall centrelines (start, end) for the foundation under `above`, drawn
/// clockwise so the exterior face is on the left.
fn desired_walls(doc: &Document, above: Id, spec: &Foundation) -> Vec<(Point2, Point2)> {
    let mut out = Vec::new();
    for footprint in rooms::detect(doc, above).footprints {
        let outline = simplify_collinear(&footprint);
        if outline.len() < 3 {
            continue;
        }
        // The footprint is the exterior face of the walls above; the foundation's
        // exterior face lines up with it.
        let center = inset_polygon(&outline, spec.wall_thickness.mm() / 2.0);
        let n = center.len();
        for k in 0..n {
            let a = center[(n - k) % n];
            let b = center[(2 * n - k - 1) % n];
            out.push((pt(a), pt(b)));
        }
    }
    out
}

fn foundation_wall_type(doc: &mut Document, thickness: Length, out: &mut Outcome) -> Id {
    let name = format!(
        "Concrete Foundation {}",
        thickness.format(DisplayUnit::Inches { denominator: 8 })
    );
    if let Some(t) = doc.find_named::<WallType>(&name) {
        return t.id;
    }
    let id = doc.insert(WallType {
        id: Id(0),
        name,
        layers: vec![WallLayer {
            name: "Concrete".into(),
            thickness,
            material: "Concrete".into(),
            main: false,
        }],
        railing: None,
        cladding: None,
        corner_trim: None,
    });
    out.created.push(id);
    id
}

fn key(p: Point2) -> (i64, i64) {
    (
        (p.x().mm() * 10.0).round() as i64,
        (p.y().mm() * 10.0).round() as i64,
    )
}

/// Make the foundation floor's walls match `desired`. Returns whether anything changed.
fn rebuild_walls(
    doc: &mut Document,
    floor: Id,
    above: Id,
    spec: &Foundation,
    out: &mut Outcome,
) -> bool {
    // Piers carry beams, not walls: the foundation floor has no walls.
    let desired = if spec.kind == FoundationKind::Piers {
        Vec::new()
    } else {
        desired_walls(doc, above, spec)
    };
    if desired.is_empty() && spec.kind != FoundationKind::Piers {
        return false;
    }
    let footing =
        (spec.footing_width.mm() > 0.0 && spec.footing_depth.mm() > 0.0).then_some(Footing {
            width: spec.footing_width,
            depth: spec.footing_depth,
        });
    let wall_type = foundation_wall_type(doc, spec.wall_thickness, out);
    let existing: Vec<Wall> = doc.walls_on(floor).cloned().collect();
    let mut want: Vec<_> = desired.iter().map(|(a, b)| (key(*a), key(*b))).collect();
    let mut have: Vec<_> = existing
        .iter()
        .map(|w| (key(w.start), key(w.end)))
        .collect();
    want.sort();
    have.sort();
    let same = want == have
        && existing
            .iter()
            .all(|w| w.wall_type == wall_type && w.footing == footing && w.height.is_none());
    if same {
        return false;
    }
    for w in &existing {
        let openings: Vec<Id> = doc.openings_in(w.id).map(|o| o.id).collect();
        for o in openings {
            doc.remove(o);
            out.deleted.push(o);
        }
        doc.remove(w.id);
        out.deleted.push(w.id);
    }
    let layer = doc.active_defaults().layers.walls;
    for (start, end) in desired {
        out.created.push(doc.insert(Wall {
            id: Id(0),
            floor,
            start,
            end,
            wall_type,
            height: None,
            layer,
            footing,
            roof: Default::default(),
            renovation: Default::default(),
            shape: Default::default(),
        }));
    }
    true
}

/// Elevation of the foundation floor so its walls meet the bottom of the platform above.
fn elevation_for(above: &Floor, spec: &Foundation) -> Length {
    above.elevation - above.floor_structure - spec.wall_height
}

pub(crate) fn build(
    doc: &mut Document,
    spec: Foundation,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let Some(mut above) = supported_floor(doc) else {
        return Err(ApiError::Invalid(
            "there is no floor to build a foundation under".into(),
        ));
    };
    if desired_walls(doc, above.id, &spec).is_empty() {
        return Err(ApiError::Invalid(format!(
            "{} needs exterior walls that enclose an area before a foundation can be built",
            above.name
        )));
    }
    // On a slab, the slab is the floor: no framed platform above it.
    if spec.kind == FoundationKind::Slab && above.floor_structure.mm() != 0.0 {
        if let Some(f) = doc.get_mut::<Floor>(above.id) {
            f.floor_structure = Length::ZERO;
        }
        above.floor_structure = Length::ZERO;
        out.modified.push(above.id);
    }
    let elevation = elevation_for(&above, &spec);
    let floor = match foundation_floor(doc) {
        Some(f) => {
            let fm = doc.get_mut::<Floor>(f.id).expect("exists");
            fm.elevation = elevation;
            fm.ceiling_height = spec.wall_height;
            fm.foundation = Some(spec.clone());
            out.modified.push(f.id);
            f.id
        }
        None => {
            let id = doc.insert(Floor {
                id: Id(0),
                name: "Foundation".into(),
                elevation,
                ceiling_height: spec.wall_height,
                floor_structure: Length::ZERO,
                foundation: Some(spec.clone()),
                auto_roof: false,
                platform: Vec::new(),
            });
            out.created.push(id);
            id
        }
    };
    rebuild_walls(doc, floor, above.id, &spec, out);
    Ok(())
}

/// Keep auto-rebuilding foundations in step with the floor above.
pub(crate) fn sync(doc: &mut Document, out: &mut Outcome) -> Result<(), ApiError> {
    let Some(f) = foundation_floor(doc) else {
        return Ok(());
    };
    let spec = f.foundation.clone().expect("foundation floor");
    if !spec.auto_rebuild {
        return Ok(());
    }
    let Some(above) = supported_floor(doc) else {
        return Ok(());
    };
    let elevation = elevation_for(&above, &spec);
    if (elevation.mm() - f.elevation.mm()).abs() > 0.01
        || (f.ceiling_height.mm() - spec.wall_height.mm()).abs() > 0.01
    {
        let fm = doc.get_mut::<Floor>(f.id).expect("exists");
        fm.elevation = elevation;
        fm.ceiling_height = spec.wall_height;
        out.modified.push(f.id);
    }
    rebuild_walls(doc, f.id, above.id, &spec, out);
    Ok(())
}
