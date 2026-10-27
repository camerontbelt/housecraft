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
/// The outlines of the building on `floor`, leaving out outdoor rooms
/// (porches, decks): walls that only enclose them don't count.
fn building_footprints(doc: &Document, floor: Id) -> Vec<Vec<housecraft_geom::DVec2>> {
    let regions = rooms::detect(doc, floor);
    let outdoor = |i: usize| {
        doc.rooms_on(floor)
            .find(|r| regions.room_at(housecraft_geom::v2(r.anchor)) == Some(i))
            .and_then(|r| doc.get::<RoomType>(r.room_type))
            .is_some_and(RoomType::is_exterior)
    };
    let (mut outside, mut inside) = (Vec::new(), Vec::new());
    for (i, region) in regions.rooms.iter().enumerate() {
        if outdoor(i) { &mut outside } else { &mut inside }.extend(region.walls.iter().copied());
    }
    let only_outdoor: Vec<Id> = outside.into_iter().filter(|w| !inside.contains(w)).collect();
    if only_outdoor.is_empty() {
        return regions.footprints;
    }
    let mut without = doc.clone();
    for w in only_outdoor {
        without.remove(w);
    }
    rooms::detect(&without, floor).footprints
}

fn desired_walls(doc: &Document, above: Id, spec: &Foundation) -> Vec<(Point2, Point2)> {
    let mut out = Vec::new();
    for footprint in building_footprints(doc, above) {
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
            r_value: 0.0,
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

/// Make the foundation floor's outside walls follow the floor above.
/// Walls already where they should be are kept as they are (their type,
/// footing and openings); outside walls no longer wanted are removed and
/// missing ones added. Walls drawn inside the foundation are never touched.
/// `reset` (Build Foundation) also resets the kept walls' type and footing.
/// Returns whether anything changed.
fn rebuild_walls(
    doc: &mut Document,
    floor: Id,
    above: Id,
    spec: &Foundation,
    reset: bool,
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
    let pair = |a: Point2, b: Point2| {
        let (ka, kb) = (key(a), key(b));
        if ka <= kb { (ka, kb) } else { (kb, ka) }
    };
    // Outside walls: their centreline runs along the foundation's outline.
    let regions = housecraft_geom::rooms::detect(doc, floor);
    let outside = |w: &Wall| {
        let half = doc.get::<WallType>(w.wall_type).map_or(100.0, |t| t.thickness().mm() / 2.0);
        let (a, b) = (housecraft_geom::v2(w.start), housecraft_geom::v2(w.end));
        regions.footprints.iter().any(|fp| {
            let near = |p: housecraft_geom::DVec2| {
                (0..fp.len()).any(|i| (housecraft_geom::math::distance_to_segment(p, fp[i], fp[(i + 1) % fp.len()]) - half).abs() < 2.0)
            };
            // Along its length (its ends can sit at an inside corner).
            [0.25, 0.5, 0.75].iter().all(|t| near(a + (b - a) * *t))
        })
    };
    let want: Vec<_> = desired.iter().map(|(a, b)| pair(*a, *b)).collect();
    let existing: Vec<Wall> = doc.walls_on(floor).cloned().collect();
    let outside: Vec<Id> = existing.iter().filter(|w| outside(w)).map(|w| w.id).collect();
    let mut kept = Vec::new();
    let mut removed: Vec<Wall> = Vec::new();
    let mut changed = false;
    for w in &existing {
        let k = pair(w.start, w.end);
        if want.contains(&k) && !kept.contains(&k) {
            kept.push(k);
            if reset && (w.wall_type != wall_type || w.footing != footing || w.height.is_some()) {
                let wm = doc.get_mut::<Wall>(w.id).expect("exists");
                wm.wall_type = wall_type;
                wm.footing = footing;
                wm.height = None;
                out.modified.push(w.id);
                changed = true;
            }
            continue;
        }
        if !outside.contains(&w.id) {
            continue;
        }
        let openings: Vec<Id> = doc.openings_in(w.id).map(|o| o.id).collect();
        for o in openings {
            doc.remove(o);
            out.deleted.push(o);
        }
        doc.remove(w.id);
        out.deleted.push(w.id);
        removed.push(w.clone());
        changed = true;
    }
    let layer = doc.active_defaults().layers.walls;
    for (start, end) in desired {
        if kept.contains(&pair(start, end)) {
            continue;
        }
        // A wall replaced along the same line passes on its type, footing and height.
        let (a, b) = (housecraft_geom::v2(start), housecraft_geom::v2(end));
        let dir = (b - a).normalize_or_zero();
        let along_line = |w: &&Wall| {
            let (p, q) = (housecraft_geom::v2(w.start), housecraft_geom::v2(w.end));
            let off = |x: housecraft_geom::DVec2| (x - a).perp_dot(dir).abs();
            let (lo, hi) = ((p - a).dot(dir).min((q - a).dot(dir)), (p - a).dot(dir).max((q - a).dot(dir)));
            off(p) < 1.0 && off(q) < 1.0 && hi > 1.0 && lo < (b - a).length() - 1.0
        };
        let was = (!reset).then(|| removed.iter().find(along_line)).flatten();
        out.created.push(doc.insert(Wall {
            id: Id(0),
            floor,
            start,
            end,
            wall_type: was.map_or(wall_type, |w| w.wall_type),
            height: was.and_then(|w| w.height),
            layer,
            footing: was.map_or(footing, |w| w.footing),
            roof: Default::default(),
            renovation: Default::default(),
            shape: Default::default(),
            invisible: false,
        }));
        changed = true;
    }
    changed
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
                joist_spacing: None,
                rim_material: None,
                platform: Vec::new(),
            });
            out.created.push(id);
            id
        }
    };
    rebuild_walls(doc, floor, above.id, &spec, true, out);
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
    rebuild_walls(doc, f.id, above.id, &spec, false, out);
    Ok(())
}
