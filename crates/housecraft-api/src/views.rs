//! Creating elevation and section views.

use housecraft_geom::{DVec2, pt, rooms, v2};
use housecraft_model::*;
use housecraft_units::Length;

use crate::ApiError;
use crate::command::Outcome;
use crate::ops::expect;

/// The floor a view's callout goes on: the given one, else the first floor
/// above any foundation (where people look for elevation callouts).
pub(crate) fn callout_floor(doc: &Document, floor: Option<Id>) -> Result<Id, ApiError> {
    match floor {
        Some(f) => expect::<Floor>(doc, f).map(|f| f.id),
        None => doc
            .floors_by_elevation()
            .into_iter()
            .find(|f| f.foundation.is_none())
            .or_else(|| doc.floors_by_elevation().into_iter().next())
            .map(|f| f.id)
            .ok_or_else(|| ApiError::Invalid("the document has no floors".into())),
    }
}

/// Four exterior elevations, each looking at one side of the building's
/// outline from 6' beyond it, named by compass direction.
pub(crate) fn auto_elevations(
    doc: &mut Document,
    floor: Option<Id>,
    d: &DefaultSet,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let floor = callout_floor(doc, floor)?;
    let pts: Vec<DVec2> = doc
        .floors_by_elevation()
        .iter()
        .flat_map(|f| rooms::detect(doc, f.id).footprints.into_iter().flatten())
        .collect();
    if pts.is_empty() {
        return Err(ApiError::Invalid(
            "draw exterior walls that enclose an area before creating elevations".into(),
        ));
    }
    let (lo, hi) = pts.iter().fold(
        (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let gap = Length::from_feet(6.0).mm();
    let (lo, hi) = (lo - DVec2::splat(gap), hi + DVec2::splat(gap));
    // Plan direction of north, and the compass name of a side whose outward
    // normal points along `n`.
    let north = DVec2::from_angle(doc.settings.north.to_radians());
    let compass = |n: DVec2| {
        let east = DVec2::new(north.y, -north.x);
        if n.dot(north).abs() >= n.dot(east).abs() {
            if n.dot(north) > 0.0 { "North" } else { "South" }
        } else if n.dot(east) > 0.0 {
            "East"
        } else {
            "West"
        }
    };
    // Lines run so the view looks left of a→b, back toward the building.
    let sides = [
        (DVec2::new(lo.x, lo.y), DVec2::new(hi.x, lo.y), DVec2::NEG_Y),
        (DVec2::new(hi.x, lo.y), DVec2::new(hi.x, hi.y), DVec2::X),
        (DVec2::new(hi.x, hi.y), DVec2::new(lo.x, hi.y), DVec2::Y),
        (DVec2::new(lo.x, hi.y), DVec2::new(lo.x, lo.y), DVec2::NEG_X),
    ];
    for (a, b, outward) in sides {
        out.created.push(doc.insert(SectionView {
            id: Id(0),
            name: format!("{} Elevation", compass(outward)),
            floor,
            layer: d.layers.text,
            a: pt(a),
            b: pt(b),
            section_kind: SectionKind::Elevation,
            depth: None,
            clip_sides: false,
        }));
    }
    Ok(())
}

/// Four interior elevations of a room: section lines just inside each side
/// of its bounding box, looking across the room, clipped to the room.
pub(crate) fn room_elevations(
    doc: &mut Document,
    room: Id,
    d: &DefaultSet,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let r = expect::<Room>(doc, room)?.clone();
    let regions = rooms::detect(doc, r.floor);
    let Some(i) = regions.room_at(v2(r.anchor)) else {
        return Err(ApiError::Invalid("the room has no enclosing walls".into()));
    };
    let poly = &regions.rooms[i].polygon;
    let (lo, hi) = poly.iter().fold(
        (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let inset = Length::from_inches(6.0).mm();
    let (lo, hi) = (lo + DVec2::splat(inset), hi - DVec2::splat(inset));
    let name = doc.room_name(&r);
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    // Each looks at the wall beyond the opposite side of the room.
    let views = [
        ("A", DVec2::new(lo.x, lo.y), DVec2::new(hi.x, lo.y), h),
        ("B", DVec2::new(hi.x, lo.y), DVec2::new(hi.x, hi.y), w),
        ("C", DVec2::new(hi.x, hi.y), DVec2::new(lo.x, hi.y), h),
        ("D", DVec2::new(lo.x, hi.y), DVec2::new(lo.x, lo.y), w),
    ];
    for (tag, a, b, depth) in views {
        out.created.push(doc.insert(SectionView {
            id: Id(0),
            name: format!("{name} Elevation {tag}"),
            floor: r.floor,
            layer: d.layers.text,
            a: pt(a),
            b: pt(b),
            section_kind: SectionKind::Section,
            depth: Some(Length::from_mm(
                depth + 2.0 * inset + Length::from_feet(1.0).mm(),
            )),
            clip_sides: true,
        }));
    }
    Ok(())
}
