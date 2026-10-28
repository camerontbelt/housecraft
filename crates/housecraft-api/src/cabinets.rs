//! Placing cabinets and appliances.

use housecraft_geom::{pt, v2};
use housecraft_model::*;
use housecraft_units::{Length, Point2};

use crate::ApiError;
use crate::command::Outcome;
use crate::ops::invalid;
use serde_json::{Value, json};

/// How far from a wall a click still snaps onto it.
const REACH_FT: f64 = 3.0;

/// The back-centre position and facing for something `depth` deep clicked
/// at `at`: backed onto the nearest wall face within reach, else as given.
fn place(doc: &Document, floor: Id, at: Point2, facing: Option<f64>) -> (Point2, f64) {
    if let Some(f) = facing {
        return (at, f);
    }
    match housecraft_geom::electrical::snap_to_wall(
        doc,
        floor,
        v2(at),
        Length::from_feet(REACH_FT).mm(),
    ) {
        Some((q, angle)) => (pt(q), angle),
        None => (at, 90.0),
    }
}

/// Which end of a cabinet at `position` facing `facing` (its back to a
/// wall) touches a side wall: `Some(true)` for the left end (as you face
/// the cabinet), `Some(false)` for the right, `None` for neither.
fn corner_end(doc: &Document, floor: Id, position: Point2, facing: f64, width: Length) -> Option<bool> {
    let front = housecraft_geom::DVec2::from_angle(facing.to_radians());
    // Facing the cabinet, its left end is to the front direction's right.
    let side = housecraft_geom::math::perp(front);
    let c = v2(position);
    let half = width.mm() / 2.0;
    let touches = |end: housecraft_geom::DVec2| {
        housecraft_geom::walls::wall_geoms(doc, floor).iter().any(|g| {
            g.dir.dot(front).abs() > 0.9 && {
                let (q, _) = housecraft_geom::math::closest_on_segment(end, g.start, g.end);
                q.distance(end) < g.half + 50.0
            }
        })
    };
    let (left, right) = (c - side * (half + 1.0) + front * 100.0, c + side * (half + 1.0) + front * 100.0);
    if touches(left) {
        Some(true)
    } else if touches(right) {
        Some(false)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn add_cabinet(
    doc: &mut Document,
    d: &DefaultSet,
    floor: Id,
    kind: CabinetKind,
    at: Point2,
    facing: Option<f64>,
    [width, depth, height]: [Option<Length>; 3],
    shape: CabinetShape,
    sink: bool,
) -> Result<Id, ApiError> {
    let c = &d.cabinets;
    if sink && kind != CabinetKind::Base {
        return invalid("only base cabinets hold a sink");
    }
    let (w, dp, h, elevation) = match kind {
        CabinetKind::Base => (c.base_width, c.base_depth, c.base_height, Length::ZERO),
        CabinetKind::Wall => (c.wall_width, c.wall_depth, c.wall_height, c.wall_elevation),
        CabinetKind::Full => (c.base_width, c.full_depth, c.full_height, Length::ZERO),
    };
    let width = width.unwrap_or(if sink { Length::from_inches(36.0) } else { w });
    let depth = depth.unwrap_or(dp);
    let height = height.unwrap_or(h);
    if width.mm() <= 0.0 || depth.mm() <= 0.0 || height.mm() <= 0.0 {
        return invalid("cabinet sizes must be positive");
    }
    let (position, facing) = place(doc, floor, at, facing);
    let toe = (kind != CabinetKind::Wall).then_some((c.toe_kick_height, c.toe_kick_depth));
    let countertop = (kind == CabinetKind::Base).then_some(CabinetTop {
        thickness: c.countertop_thickness,
        overhang: c.countertop_overhang,
        edge: c.edge,
        backsplash: c.backsplash,
        corner: CornerTreatment::Square,
        corner_left: false,
        corner_right: false,
        back_overhang: Length::ZERO,
        side_overhang: Length::ZERO,
        side_splash_left: false,
        side_splash_right: false,
        waterfall_left: false,
        waterfall_right: false,
    });
    // Into a corner: a diagonal front on the end against the side wall.
    let shape = match shape {
        CabinetShape::Standard if c.corner_cut.mm() > 1.0 && kind != CabinetKind::Full => {
            match corner_end(doc, floor, position, facing, width) {
                Some(true) => CabinetShape::Angled { left: c.corner_cut, right: Length::ZERO },
                Some(false) => CabinetShape::Angled { left: Length::ZERO, right: c.corner_cut },
                None => shape,
            }
        }
        _ => shape,
    };
    Ok(doc.insert(Cabinet {
        id: Id(0),
        floor,
        layer: d.layers.cabinets.unwrap_or(d.layers.cad),
        cabinet_kind: kind,
        position,
        facing,
        width,
        depth,
        height,
        elevation,
        construction: c.construction,
        shape,
        face: c.face(kind, width, sink),
        door_style: c.door_style,
        hardware: c.hardware,
        handle_from_edge: None,
        shelves: if kind == CabinetKind::Base { 1 } else { 2 },
        toe_kick: toe,
        countertop,
        sink,
        filler_left: Length::ZERO,
        filler_right: Length::ZERO,
        no_auto_filler: false,
        renovation: Renovation::default(),
        build: CabinetBuild::default(),
        open: 0.0,
        blind: None,
        moldings: Vec::new(),
        ends: EndPanels::default(),
            accessories: Vec::new(),
            corner: None,
    }))
}

pub(crate) fn add_appliance(
    doc: &mut Document,
    d: &DefaultSet,
    floor: Id,
    kind: ApplianceKind,
    at: Point2,
    facing: Option<f64>,
    [width, depth, height]: [Option<Length>; 3],
) -> Result<Id, ApiError> {
    let (w, dp, h, e) = kind.typical();
    let i = Length::from_inches;
    let width = width.unwrap_or(i(w));
    let depth = depth.unwrap_or(i(dp));
    let height = height.unwrap_or(i(h));
    if width.mm() <= 0.0 || depth.mm() <= 0.0 || height.mm() <= 0.0 {
        return invalid("appliance sizes must be positive");
    }
    let (position, facing) = if kind.against_wall() {
        place(doc, floor, at, facing)
    } else {
        (at, facing.unwrap_or(90.0))
    };
    Ok(doc.insert(Appliance {
        id: Id(0),
        floor,
        layer: d.layers.cabinets.unwrap_or(d.layers.cad),
        appliance_kind: kind,
        position,
        facing,
        width,
        depth,
        height,
        elevation: i(e),
        renovation: Renovation::default(),
        reversed: false,
        draw_order: 0,
    }))
}

/// Fields that say where an object is rather than what it is.
const PLACEMENT: [&str; 8] = [
    "id",
    "floor",
    "layer",
    "position",
    "facing",
    "renovation",
    "filler_left",
    "filler_right",
];

/// `template` with `placement`'s placement fields, as an object of `kind`.
fn instantiate(kind: ObjectKind, template: &Value, placement: &Value) -> Result<Object, ApiError> {
    let mut v = template.clone();
    let Some(m) = v.as_object_mut() else {
        return invalid("a template must be a JSON object");
    };
    for k in PLACEMENT {
        m.remove(k);
        if let Some(p) = placement.get(k) {
            m.insert(k.into(), p.clone());
        }
    }
    m.insert("kind".into(), json!(kind));
    serde_json::from_value::<Object>(v)
        .map_err(|e| ApiError::Invalid(format!("the template doesn't make a {kind}: {e}")))
}

pub(crate) fn place_object(
    doc: &mut Document,
    d: &DefaultSet,
    floor: Id,
    kind: ObjectKind,
    template: &Value,
    at: Point2,
    facing: Option<f64>,
) -> Result<Id, ApiError> {
    if !matches!(kind, ObjectKind::Cabinet | ObjectKind::Appliance | ObjectKind::Symbol) {
        return invalid("only cabinets, appliances and symbols are placed from templates");
    }
    let (position, facing) = place(doc, floor, at, facing);
    let placement = json!({
        "id": 0,
        "floor": floor,
        "layer": d.layers.cabinets.unwrap_or(d.layers.cad),
        "position": position,
        "facing": facing,
    });
    Ok(doc.insert(instantiate(kind, template, &placement)?))
}

/// An object as JSON without where it is, for comparing.
fn essence(o: &Object) -> Value {
    let mut v = serde_json::to_value(o).expect("serialisable");
    if let Some(m) = v.as_object_mut() {
        for k in PLACEMENT {
            m.remove(k);
        }
    }
    v
}

pub(crate) fn replace_with(
    doc: &mut Document,
    ids: &[Id],
    template: &Value,
    all_identical: bool,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let mut targets: Vec<Id> = Vec::new();
    for id in ids {
        let o = doc.object(*id).ok_or(ApiError::NotFound(*id))?;
        if !matches!(o.kind(), ObjectKind::Cabinet | ObjectKind::Appliance) {
            return invalid(format!("{id} is a {}; only cabinets and appliances are replaced", o.kind()));
        }
        targets.push(*id);
    }
    if all_identical {
        let looks: Vec<Value> = targets
            .iter()
            .filter_map(|id| doc.object(*id))
            .map(essence)
            .collect();
        for o in doc.objects.values() {
            if !targets.contains(&o.id())
                && matches!(o.kind(), ObjectKind::Cabinet | ObjectKind::Appliance)
                && looks.contains(&essence(o))
            {
                targets.push(o.id());
            }
        }
    }
    // A template may be either kind; it says which by its fields.
    let kind = if template.get("cabinet_kind").is_some() {
        ObjectKind::Cabinet
    } else if template.get("appliance_kind").is_some() {
        ObjectKind::Appliance
    } else {
        return invalid("the template is neither a cabinet nor an appliance");
    };
    for id in targets {
        let placement = serde_json::to_value(doc.object(id).expect("checked")).expect("serialisable");
        let mut new = instantiate(kind, template, &placement)?;
        new.set_id(id);
        doc.objects.insert(id, new);
        out.modified.push(id);
    }
    Ok(())
}

/// Dimension runs of base and tall cabinets: cabinets facing the same way
/// with their backs on one line form a run; a chain dimension in front of
/// the run marks each cabinet's ends.
pub(crate) fn auto_dimensions(doc: &mut Document, d: &DefaultSet, floor: Id, out: &mut Outcome) {
    let cabs: Vec<Cabinet> = doc
        .iter::<Cabinet>()
        .filter(|c| c.floor == floor && c.cabinet_kind != CabinetKind::Wall)
        .cloned()
        .collect();
    let mut runs: Vec<Vec<&Cabinet>> = Vec::new();
    for c in &cabs {
        let f = housecraft_geom::cabinets::frame(c);
        let found = runs.iter_mut().find(|r| {
            let g = housecraft_geom::cabinets::frame(r[0]);
            (f.out.dot(g.out) - 1.0).abs() < 1e-6 && (f.origin - g.origin).dot(g.out).abs() < 25.0
        });
        match found {
            Some(r) => r.push(c),
            None => runs.push(vec![c]),
        }
    }
    for run in runs {
        let f = housecraft_geom::cabinets::frame(run[0]);
        let depth = run.iter().map(|c| c.depth.mm()).fold(0.0, f64::max);
        let mut ts: Vec<f64> = Vec::new();
        for c in &run {
            let o = (v2(c.position) - f.origin).dot(f.along);
            let h = c.width.mm() / 2.0;
            ts.push(o - h - c.filler_left.mm());
            ts.push(o + h + c.filler_right.mm());
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|a, b| (*a - *b).abs() < 3.0);
        if ts.len() < 2 {
            continue;
        }
        let points: Vec<Point2> = ts
            .iter()
            .map(|t| pt(f.origin + f.along * *t + f.out * depth))
            .collect();
        // Aligned offsets go to the left of first→last, which is toward the
        // wall; the dimension belongs in the room.
        out.created.push(doc.insert(Dimension {
            id: Id(0),
            floor,
            layer: d.layers.dimensions,
            geometry: DimensionGeometry::Linear {
                points,
                orientation: DimOrientation::Aligned,
                offset: Length::from_inches(-12.0),
            },
            text_override: None,
            auto: false,
            style: None,
        }));
    }
}

/// End panels on every cabinet side that is open: not against another
/// cabinet or a wall.
pub(crate) fn auto_end_panels(doc: &mut Document, floor: Option<Id>, out: &mut Outcome) {
    let cabinets: Vec<Cabinet> = doc.iter::<Cabinet>().filter(|c| floor.is_none_or(|f| f == c.floor)).cloned().collect();
    for c in cabinets {
        let f = housecraft_geom::cabinets::frame(&c);
        let half = c.width.mm() / 2.0;
        let d = c.depth.mm();
        let walls: Vec<Vec<housecraft_geom::DVec2>> = housecraft_geom::walls::wall_geoms(doc, c.floor).iter().map(|g| g.corners.to_vec()).collect();
        let open_at = |u: f64| {
            let p = f.world(housecraft_geom::DVec2::new(u, d / 2.0));
            let cabinet = doc.iter::<Cabinet>().any(|o| {
                o.id != c.id && o.floor == c.floor && (o.cabinet_kind == CabinetKind::Wall) == (c.cabinet_kind == CabinetKind::Wall) && housecraft_geom::math::point_in_polygon(p, &housecraft_geom::cabinets::outline(o))
            });
            let wall = walls.iter().any(|w| housecraft_geom::math::point_in_polygon(p, w));
            !cabinet && !wall
        };
        let left = open_at(-half - c.filler_left.mm() - 30.0);
        let right = open_at(half + c.filler_right.mm() + 30.0);
        if (left, right) != (c.ends.left, c.ends.right)
            && let Some(Object::Cabinet(m)) = doc.objects.get_mut(&c.id)
        {
            m.ends.left = left;
            m.ends.right = right;
            out.modified.push(c.id);
        }
    }
}
