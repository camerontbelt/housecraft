//! Placing cabinets and appliances.

use housecraft_geom::{pt, v2};
use housecraft_model::*;
use housecraft_units::{Length, Point2};

use crate::ApiError;
use crate::ops::invalid;

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
