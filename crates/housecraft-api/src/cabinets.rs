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
        waterfall_left: false,
        waterfall_right: false,
    });
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
        }));
    }
}
