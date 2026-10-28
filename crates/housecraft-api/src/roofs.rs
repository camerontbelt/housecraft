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
    // What was set on the old planes carries over to the new plane in the
    // same place and pitch: rafter spacing, and skylights still inside it.
    let plan = |o: &[Point3]| -> Vec<housecraft_geom::DVec2> {
        o.iter().map(|p| {
            let [x, y, _] = p.mm();
            housecraft_geom::DVec2::new(x, y)
        }).collect()
    };
    for g in &generated {
        let outline = to_points(&g.outline);
        let poly = plan(&outline);
        let c = housecraft_geom::math::centroid(&poly);
        let old = existing
            .iter()
            .filter(|r| r.gable == g.gable && (r.pitch - g.pitch).abs() < 0.5)
            .filter(|r| housecraft_geom::math::point_in_polygon(c, &plan(&r.outline)) || housecraft_geom::math::point_in_polygon(housecraft_geom::math::centroid(&plan(&r.outline)), &poly))
            .min_by(|a, b| {
                let d = |r: &RoofPlane| housecraft_geom::math::centroid(&plan(&r.outline)).distance(c);
                d(a).total_cmp(&d(b))
            });
        let rafter_spacing = old.and_then(|r| r.rafter_spacing);
        let rafter_depth = old.and_then(|r| r.rafter_depth);
        let skylights: Vec<Skylight> = old.map_or(Vec::new(), |r| {
            r.skylights.iter().filter(|s| housecraft_geom::math::point_in_polygon(housecraft_geom::v2(s.center), &poly)).cloned().collect()
        });
        out.created.push(doc.insert(RoofPlane {
            id: Id(0),
            floor,
            outline,
            pitch: g.pitch,
            thickness: if g.gable {
                Length::ZERO
            } else {
                defaults.roof.total_thickness()
            },
            gable: g.gable,
            auto: true,
            layer,
            skylights,
            rafter_spacing,
            rafter_depth,
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
        thickness: defaults.roof.total_thickness(),
        gable: false,
        auto: false,
        layer: defaults.layers.roofs.unwrap_or(defaults.layers.walls),
        skylights: Vec::new(),
        rafter_spacing: None,
        rafter_depth: None,
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

/// Apply a whole-roof style to a floor's exterior walls and build the roof.
pub(crate) fn set_style(
    doc: &mut Document,
    floor: Option<Id>,
    style: RoofStyle,
    pitch: Option<f64>,
    lower_pitch: Option<f64>,
    break_height: Option<Length>,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let floor = roof_floor(doc, floor)?;
    let pitch = pitch.unwrap_or(doc.active_defaults().roof.pitch);
    if !(1.0..=85.0).contains(&pitch) {
        return Err(ApiError::Invalid(
            "pitch must be between 1 and 85 degrees".into(),
        ));
    }
    let footprints = housecraft_geom::rooms::detect(doc, floor).footprints;
    let Some(fp) = footprints.iter().max_by(|a, b| {
        housecraft_geom::math::signed_area(a)
            .abs()
            .total_cmp(&housecraft_geom::math::signed_area(b).abs())
    }) else {
        return Err(ApiError::Invalid(
            "the floor needs exterior walls that enclose an area before a roof can be built".into(),
        ));
    };
    // Sides run along the footprint's long axis; ends are the walls across it.
    let axis = housecraft_geom::math::long_axis(fp);
    let across = housecraft_geom::math::perp(axis);
    let (lo, hi) = fp
        .iter()
        .map(|p| p.dot(across))
        .fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
    let half_width = (hi - lo) / 2.0;
    let steep = lower_pitch.unwrap_or(match style {
        RoofStyle::Mansard => 70.0,
        _ => 60.0,
    });
    let break_at = break_height.map(|b| b.mm()).unwrap_or(match style {
        // Dutch gable: the little gable is half the end's width.
        RoofStyle::DutchGable => half_width / 2.0 * pitch.to_radians().tan(),
        // Mansard and gambrel: the steep part climbs 40% of the way in.
        _ => (half_width * 0.4 * steep.to_radians().tan()).min(Length::from_feet(6.0).mm()),
    });
    let walls: Vec<Wall> = doc
        .walls_on(floor)
        .filter(|w| w.renovation != Renovation::Demolish)
        .cloned()
        .collect();
    // The shed slopes up from the side wall facing most nearly south.
    let shed_low = walls
        .iter()
        .filter(|w| ((v2(w.end) - v2(w.start)).normalize_or_zero().dot(axis)).abs() > 0.7)
        .min_by(|a, b| {
            ((v2(a.start) + v2(a.end)) / 2.0)
                .y
                .total_cmp(&((v2(b.start) + v2(b.end)) / 2.0).y)
        })
        .map(|w| w.id);
    for w in walls {
        let dir = (v2(w.end) - v2(w.start)).normalize_or_zero();
        let mid = (v2(w.start) + v2(w.end)) / 2.0;
        // Interior walls (well inside the footprint) don't carry roof edges.
        if !footprint_edge(fp, mid) {
            continue;
        }
        let is_side = dir.dot(axis).abs() > 0.7;
        let upper = |p: f64, gable: bool| {
            Some(UpperRoof {
                break_height: Length::from_mm(break_at.max(1.0)),
                pitch: p,
                gable,
                from_floor: false,
            })
        };
        let (gable, edge_pitch, up) = match style {
            RoofStyle::Hip => (false, pitch, None),
            RoofStyle::Gable => (!is_side, pitch, None),
            RoofStyle::Shed => (Some(w.id) != shed_low, pitch, None),
            RoofStyle::Mansard => (false, steep, upper(pitch, false)),
            RoofStyle::Gambrel if is_side => (false, steep, upper(pitch, false)),
            RoofStyle::Gambrel => (true, pitch, None),
            RoofStyle::DutchGable if is_side => (false, pitch, None),
            RoofStyle::DutchGable => (false, pitch, upper(pitch, true)),
        };
        if let Some(wall) = doc.get_mut::<Wall>(w.id) {
            wall.roof.gable = gable;
            wall.roof.pitch = Some(edge_pitch);
            wall.roof.upper = up;
            // A steep lower slope carried out a full eave overhang would drop
            // far below the plate; mansards and gambrels keep a short one.
            wall.roof.overhang =
                (up.is_some() && edge_pitch > 50.0).then(|| Length::from_inches(4.0));
            out.modified.push(w.id);
        }
    }
    build(doc, Some(floor), true, out)
}

/// Whether `p` lies on the footprint outline (within a wall's thickness).
fn footprint_edge(fp: &[DVec2], p: DVec2) -> bool {
    (0..fp.len()).any(|i| {
        housecraft_geom::math::distance_to_segment(p, fp[i], fp[(i + 1) % fp.len()]) < 250.0
    })
}
