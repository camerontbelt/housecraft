//! Roof commands and keeping automatic roofs in step with their walls.

use housecraft_geom::{DVec2, roof, v2};
use housecraft_model::*;
use housecraft_units::{Length, Point2, Point3};

use crate::ApiError;
use crate::command::Outcome;
use crate::ops::expect;

/// The floor a roof goes on: the given one, or the highest storey.
pub(crate) fn roof_floor(doc: &Document, floor: Option<Id>) -> Result<Id, ApiError> {
    match floor {
        Some(f) => expect::<Floor>(doc, f).map(|f| f.id),
        None => doc
            .floors_by_elevation()
            .into_iter()
            .rev()
            .find(|f| f.foundation.is_none())
            .map(|f| f.id)
            .ok_or_else(|| ApiError::Invalid("there is no floor to put a roof on".into())),
    }
}

fn key(outline: &[Point3]) -> Vec<(i64, i64, i64)> {
    let mut k: Vec<_> = outline
        .iter()
        .map(|p| {
            let [x, y, z] = p.mm();
            (
                (x * 10.0).round() as i64,
                (y * 10.0).round() as i64,
                (z * 10.0).round() as i64,
            )
        })
        .collect();
    k.sort();
    k
}

/// Replace `floor`'s automatic roof planes with freshly generated ones if they differ.
fn regenerate(doc: &mut Document, floor: Id, out: &mut Outcome) -> usize {
    let Some(f) = doc.get::<Floor>(floor).cloned() else {
        return 0;
    };
    let generated = roof::generate(doc, &f);
    let defaults = doc.active_defaults().clone();
    let layer = defaults.layers.roofs.unwrap_or(defaults.layers.walls);
    let to_points = |o: &[housecraft_geom::DVec3]| -> Vec<Point3> {
        o.iter().map(|p| Point3::from_mm(p.x, p.y, p.z)).collect()
    };
    let mut want: Vec<_> = generated
        .iter()
        .map(|g| (key(&to_points(&g.outline)), g.gable))
        .collect();
    let existing: Vec<RoofPlane> = doc
        .iter::<RoofPlane>()
        .filter(|r| r.floor == floor && r.auto)
        .cloned()
        .collect();
    let mut have: Vec<_> = existing
        .iter()
        .map(|r| (key(&r.outline), r.gable))
        .collect();
    want.sort();
    have.sort();
    if want == have {
        return generated.len();
    }
    for r in &existing {
        doc.remove(r.id);
        out.deleted.push(r.id);
    }
    for g in &generated {
        out.created.push(doc.insert(RoofPlane {
            id: Id(0),
            floor,
            outline: to_points(&g.outline),
            pitch: g.pitch,
            thickness: if g.gable {
                Length::ZERO
            } else {
                defaults.roof.thickness
            },
            gable: g.gable,
            auto: true,
            layer,
        }));
    }
    generated.len()
}

pub(crate) fn build(
    doc: &mut Document,
    floor: Option<Id>,
    auto: bool,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let floor = roof_floor(doc, floor)?;
    if let Some(f) = doc.get_mut::<Floor>(floor) {
        f.auto_roof = auto;
    }
    out.modified.push(floor);
    if regenerate(doc, floor, out) == 0 {
        return Err(ApiError::Invalid(
            "the floor needs exterior walls that enclose an area before a roof can be built".into(),
        ));
    }
    Ok(())
}

pub(crate) fn remove(
    doc: &mut Document,
    floor: Option<Id>,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let floor = roof_floor(doc, floor)?;
    if let Some(f) = doc.get_mut::<Floor>(floor) {
        f.auto_roof = false;
    }
    let ids: Vec<Id> = doc
        .iter::<RoofPlane>()
        .filter(|r| r.floor == floor)
        .map(|r| r.id)
        .collect();
    for id in ids {
        doc.remove(id);
        out.deleted.push(id);
    }
    Ok(())
}

/// A hand-drawn roof plane: `points` in plan, rising at `pitch` from the eave line.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_plane(
    doc: &mut Document,
    floor: Option<Id>,
    points: &[Point2],
    eave_a: Point2,
    eave_b: Point2,
    eave_height: Length,
    pitch: f64,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    if points.len() < 3 {
        return Err(ApiError::Invalid(
            "a roof plane needs at least three points".into(),
        ));
    }
    if !(pitch > 0.0 && pitch < 90.0) {
        return Err(ApiError::Invalid(
            "pitch must be between 0 and 90 degrees".into(),
        ));
    }
    let floor = roof_floor(doc, floor)?;
    let base = doc.get::<Floor>(floor).expect("checked").elevation + eave_height;
    let (a, b) = (v2(eave_a), v2(eave_b));
    let dir = (b - a).normalize_or_zero();
    if dir == DVec2::ZERO {
        return Err(ApiError::Invalid(
            "the eave line needs two distinct points".into(),
        ));
    }
    // The plane rises toward the side of the eave where the polygon lies.
    let centroid = points.iter().map(|p| v2(*p)).sum::<DVec2>() / points.len() as f64;
    let mut up = DVec2::new(-dir.y, dir.x);
    if up.dot(centroid - a) < 0.0 {
        up = -up;
    }
    let slope = pitch.to_radians().tan();
    let outline = points
        .iter()
        .map(|p| {
            let q = v2(*p);
            Point3::from_mm(q.x, q.y, base.mm() + up.dot(q - a) * slope)
        })
        .collect();
    let defaults = doc.active_defaults().clone();
    out.created.push(doc.insert(RoofPlane {
        id: Id(0),
        floor,
        outline,
        pitch,
        thickness: defaults.roof.thickness,
        gable: false,
        auto: false,
        layer: defaults.layers.roofs.unwrap_or(defaults.layers.walls),
    }));
    Ok(())
}

/// Rebuild automatic roofs whose walls changed.
pub(crate) fn sync(doc: &mut Document, out: &mut Outcome) {
    let floors: Vec<Id> = doc
        .iter::<Floor>()
        .filter(|f| f.auto_roof)
        .map(|f| f.id)
        .collect();
    for f in floors {
        regenerate(doc, f, out);
    }
}
