use std::collections::BTreeMap;

use housecraft_model::*;
use housecraft_units::{DisplayUnit, Length, Point2, Point3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ApiError;
use crate::ops::{self, expect, invalid, positive};

/// A single edit to a document. Serialized with an `"op"` tag, e.g.
/// `{"op": "add_wall", "start": [0, 0], "end": ["20'", 0]}`.
///
/// Objects created by `add_*` commands take their style, size and layer from
/// the active default set unless the command says otherwise. Any property of
/// any object can be changed with `update`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Command {
    /// Rename the document.
    SetDocumentName { name: String },
    /// Change how lengths are displayed.
    SetDisplayUnit { unit: DisplayUnit },
    /// Add a floor. By default it goes on top: its elevation is the highest
    /// floor's ceiling plus this floor's structure. With `below`, it is inserted
    /// under that floor and every floor from there up moves up to make room.
    AddFloor {
        name: Option<String>,
        elevation: Option<Length>,
        /// Defaults to the highest floor's ceiling height, or 8'.
        ceiling_height: Option<Length>,
        /// Floor platform depth (subfloor + joists); default 12 5/8".
        #[serde(default)]
        floor_structure: Option<Length>,
        #[serde(default)]
        below: Option<Id>,
    },
    /// Build (or rebuild) the foundation under the lowest floor from its
    /// exterior walls. Omitted sizes use typical values for the kind.
    BuildFoundation {
        kind: FoundationKind,
        wall_height: Option<Length>,
        wall_thickness: Option<Length>,
        footing_width: Option<Length>,
        footing_depth: Option<Length>,
        slab_thickness: Option<Length>,
        /// Keep the foundation in step with the floor above (default true).
        #[serde(default = "yes")]
        auto_rebuild: bool,
        /// Pier foundations: spacing along the beams (default 6').
        pier_spacing: Option<Length>,
        /// Pier foundations: pier diameter (default 12").
        pier_diameter: Option<Length>,
    },
    /// Build a roof over a floor's exterior walls (default: the top storey).
    /// Each wall's `roof` settings choose gable or hip, pitch and overhang.
    /// With `auto_rebuild` the roof follows later wall changes.
    BuildRoof {
        floor: Option<Id>,
        #[serde(default = "yes")]
        auto_rebuild: bool,
    },
    /// Set every exterior wall's roof edge for a whole-roof style (gable ends
    /// go on the short sides), then build the roof and keep it automatic.
    /// `lower_pitch` and `break_height` shape mansards and gambrels.
    SetRoofStyle {
        floor: Option<Id>,
        style: RoofStyle,
        pitch: Option<f64>,
        lower_pitch: Option<f64>,
        break_height: Option<Length>,
    },
    /// Delete a floor's roof planes and stop rebuilding them.
    RemoveRoof { floor: Option<Id> },
    /// Add a hand-drawn roof plane: `points` outline it in plan; it rises at
    /// `pitch` degrees from the line `eave_a`–`eave_b`, which sits
    /// `eave_height` above the floor.
    AddRoofPlane {
        floor: Option<Id>,
        points: Vec<Point2>,
        eave_a: Point2,
        eave_b: Point2,
        eave_height: Length,
        pitch: f64,
    },
    /// Add a stair from a floor up to the floor above. Its risers come from the
    /// floor-to-floor height (or `rise`) and the default maximum riser. For L
    /// and U shapes, `first_treads: 0` splits the flights evenly.
    AddStair {
        floor: Option<Id>,
        /// Centre of the bottom riser.
        start: Point2,
        /// Direction of travel going up, degrees counter-clockwise from east.
        direction: f64,
        #[serde(default = "straight")]
        shape: StairShape,
        width: Option<Length>,
        rise: Option<Length>,
        railing_left: Option<bool>,
        railing_right: Option<bool>,
        /// Cut an opening in the floor above (default true).
        #[serde(default = "yes")]
        stairwell: bool,
    },
    /// Place an electrical device or fixture. Wall devices (outlets,
    /// switches, wall lights) snap to the nearest wall face within 2' and
    /// face into the room unless `rotation` is given.
    AddElectrical {
        floor: Option<Id>,
        electrical_kind: ElectricalKind,
        at: Point2,
        rotation: Option<f64>,
        height: Option<Length>,
        /// Fixtures a switch controls.
        #[serde(default)]
        controls: Vec<Id>,
    },
    /// Wire a switch to fixtures (or unwire with `disconnect`). Switches that
    /// share a fixture become 3-way and 4-way automatically.
    ConnectSwitch {
        switch: Id,
        fixtures: Vec<Id>,
        #[serde(default)]
        disconnect: bool,
    },
    /// Mark walls, doors, windows and electrical as existing, new or to be
    /// demolished. Other kinds in `ids` are rejected.
    SetRenovation { ids: Vec<Id>, status: Renovation },
    /// Orient the plan: true north as an angle counter-clockwise from +x
    /// (90 = up), and where to draw the north pointer (`null` hides it).
    SetNorth { angle: f64, pointer: Option<Point2> },
    /// Position the sun for 3D: give `azimuth` / `altitude` directly, or
    /// `latitude`, `day_of_year` and solar `hour` to compute them.
    SetSun {
        azimuth: Option<f64>,
        altitude: Option<f64>,
        latitude: Option<f64>,
        day_of_year: Option<u32>,
        hour: Option<f64>,
        shadows: Option<bool>,
    },
    /// Place a schedule table (door, window, room, wall, fixture, stair or
    /// roof plane) with its top-left corner at `position`.
    AddSchedule {
        floor: Option<Id>,
        schedule_kind: ScheduleKind,
        position: Point2,
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        all_floors: bool,
    },
    /// Exchange everything drawn on two floors (walls, rooms, CAD, text...),
    /// keeping the floors' own heights.
    SwapFloors { a: Id, b: Id },
    /// Add a layered wall type.
    AddWallType {
        name: String,
        /// Layers listed exterior to interior.
        layers: Vec<WallLayer>,
    },
    /// Add a drawing layer.
    AddLayer {
        name: String,
        color: Option<Color>,
        /// Pen width on paper in mm (default 0.25).
        line_weight: Option<f64>,
        #[serde(default)]
        line_style: LineStyle,
    },
    /// Add a layer set, optionally copying another.
    AddLayerSet { name: String, copy_from: Option<Id> },
    /// Change how a layer looks in one layer set; omitted fields are left alone.
    SetLayerDisplay {
        layer_set: Id,
        layer: Id,
        visible: Option<bool>,
        color: Option<Color>,
        line_weight: Option<f64>,
        line_style: Option<LineStyle>,
    },
    /// Add a default set, copying `copy_from` or the active set.
    AddDefaultSet { name: String, copy_from: Option<Id> },
    /// Choose the default set used by commands.
    SetActiveDefaultSet { id: Id },
    /// Add a text style.
    AddTextStyle {
        name: String,
        height: Length,
        font: Option<String>,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        italic: bool,
        color: Option<Color>,
    },
    /// Add a room type.
    AddRoomType {
        name: String,
        ceiling_height: Option<Length>,
        #[serde(default)]
        floor_finish: String,
        #[serde(default = "yes")]
        living_area: bool,
    },
    /// Add one straight wall along a centreline. Wall-type layers run exterior
    /// to interior with the exterior on the left looking from start to end, so
    /// draw exterior walls clockwise around the building.
    AddWall {
        /// Defaults to the lowest floor.
        floor: Option<Id>,
        start: Point2,
        end: Point2,
        /// Defaults to the default set's exterior (or interior) wall type.
        wall_type: Option<Id>,
        /// Use the default interior wall type instead of the exterior one.
        #[serde(default)]
        interior: bool,
        /// Defaults to the floor's ceiling height.
        height: Option<Length>,
    },
    /// Add a connected run of walls through `points`; `closed` adds the closing wall.
    AddWalls {
        floor: Option<Id>,
        points: Vec<Point2>,
        #[serde(default)]
        closed: bool,
        wall_type: Option<Id>,
        #[serde(default)]
        interior: bool,
        height: Option<Length>,
    },
    /// Move one or both wall endpoints. Walls joined at a moved endpoint follow
    /// unless `detach` is set.
    MoveWall {
        id: Id,
        start: Option<Point2>,
        end: Option<Point2>,
        #[serde(default)]
        detach: bool,
    },
    /// Set a wall's length, keeping `anchor` fixed. Joined walls follow.
    SetWallLength {
        id: Id,
        length: Length,
        #[serde(default)]
        anchor: LengthAnchor,
    },
    /// Add a door to a wall. Place it with `position` (distance from the wall
    /// start to the door centre) or `at` (a point projected onto the wall);
    /// default is the wall's midpoint.
    AddDoor {
        wall: Id,
        position: Option<Length>,
        at: Option<Point2>,
        style: Option<DoorStyle>,
        width: Option<Length>,
        height: Option<Length>,
        hinge: Option<Hinge>,
        /// Swing toward the wall's right (interior) side. Default true.
        swing_in: Option<bool>,
    },
    /// Add a window to a wall; placement as for `add_door`.
    AddWindow {
        wall: Id,
        position: Option<Length>,
        at: Option<Point2>,
        style: Option<WindowStyle>,
        width: Option<Length>,
        height: Option<Length>,
        /// Height of the window bottom above the floor; default from head height.
        sill: Option<Length>,
    },
    /// Add a CAD shape (line, polyline/box, arc, circle, ellipse, spline, point).
    AddCad {
        floor: Option<Id>,
        layer: Option<Id>,
        shape: Shape,
        color: Option<Color>,
        fill: Option<Color>,
    },
    /// Add a dimension.
    AddDimension {
        floor: Option<Id>,
        layer: Option<Id>,
        geometry: DimensionGeometry,
        text_override: Option<String>,
    },
    /// Add overall and per-jog dimensions around the outside of the building.
    AddExteriorDimensions { floor: Option<Id> },
    /// Add a text note.
    AddText {
        floor: Option<Id>,
        layer: Option<Id>,
        position: Point2,
        text: String,
        style: Option<Id>,
        #[serde(default)]
        rotation: f64,
        #[serde(default)]
        justify: Justify,
        height: Option<Length>,
    },
    /// Save a plan view (floor, layer set, default set).
    AddPlanView {
        name: String,
        floor: Option<Id>,
        layer_set: Option<Id>,
        default_set: Option<Id>,
        zoom: Option<ViewRect>,
        #[serde(default)]
        reference_floor: bool,
        /// Reference floors to show (instead of the one below).
        #[serde(default)]
        references: Vec<Id>,
        #[serde(default)]
        phase: Phase,
    },
    /// Save a 3D camera.
    AddCamera {
        name: String,
        projection: Projection,
        eye: Point3,
        target: Point3,
        layer_set: Option<Id>,
    },
    /// Change any properties of any object with a JSON merge patch over its
    /// JSON form (see `get_object`); `null` clears an optional field.
    Update {
        id: Id,
        set: serde_json::Map<String, serde_json::Value>,
    },
    /// Move, rotate, mirror or scale objects. With `copies` > 0 the originals
    /// stay and that many copies are made, each transformed once more than the
    /// last (replicate at intervals). Openings alone slide along their wall.
    Transform {
        ids: Vec<Id>,
        transform: TransformOp,
        #[serde(default)]
        copies: u32,
    },
    /// Copy a CAD line/polyline/circle/arc or a wall parallel to itself;
    /// positive distance is to the left (outward for circles and arcs).
    Offset { id: Id, distance: Length },
    /// Trim or extend a CAD line or wall so its nearer end meets the line
    /// through `boundary` (a CAD line or wall).
    ExtendTo { id: Id, boundary: Id },
    /// Join two CAD lines (or two walls with radius 0) at their intersection,
    /// rounding the corner with an arc of `radius`.
    Fillet {
        a: Id,
        b: Id,
        #[serde(default)]
        radius: Length,
    },
    /// Cut the corner between two CAD lines with a straight segment.
    Chamfer { a: Id, b: Id, distance: Length },
    /// Insert copies of objects (as returned by `get_object`), e.g. from a
    /// clipboard. They get fresh ids; a door or window whose wall is in the
    /// same batch follows the new wall. Placed objects move to `floor` and
    /// shift by `dx`/`dy`.
    InsertObjects {
        objects: Vec<serde_json::Value>,
        floor: Option<Id>,
        #[serde(default)]
        dx: Length,
        #[serde(default)]
        dy: Length,
    },
    /// Delete objects. Floors take their contents with them; walls take their
    /// doors and windows. Objects still referenced elsewhere cannot be deleted.
    Delete { ids: Vec<Id> },
}

fn yes() -> bool {
    true
}

fn straight() -> StairShape {
    StairShape::Straight
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LengthAnchor {
    #[default]
    Start,
    End,
    Center,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformOp {
    Translate {
        dx: Length,
        dy: Length,
    },
    /// Degrees counter-clockwise about `center`.
    Rotate {
        center: Point2,
        angle: f64,
    },
    /// Reflect across the line through `a` and `b`.
    Mirror {
        a: Point2,
        b: Point2,
    },
    Scale {
        center: Point2,
        factor: f64,
    },
}

/// What a command or batch changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Outcome {
    pub created: Vec<Id>,
    pub modified: Vec<Id>,
    pub deleted: Vec<Id>,
}

impl Outcome {
    pub(crate) fn merge(&mut self, other: Outcome) {
        self.created.extend(other.created);
        self.modified.extend(other.modified);
        self.deleted.extend(other.deleted);
    }

    /// Remove duplicates; objects created or deleted are not also "modified".
    pub(crate) fn dedup(&mut self) {
        for v in [&mut self.created, &mut self.modified, &mut self.deleted] {
            let mut seen = std::collections::BTreeSet::new();
            v.retain(|id| seen.insert(*id));
        }
        let created: std::collections::BTreeSet<Id> = self.created.iter().copied().collect();
        let deleted: std::collections::BTreeSet<Id> = self.deleted.iter().copied().collect();
        self.modified
            .retain(|id| !created.contains(id) && !deleted.contains(id));
        // Created then deleted within one transaction never existed.
        self.created.retain(|id| !deleted.contains(id));
        self.deleted.retain(|id| !created.contains(id));
    }
}

fn first_layer_set(doc: &Document, given: Option<Id>) -> Result<Id, ApiError> {
    match given {
        Some(s) => Ok(s),
        None => doc
            .iter::<LayerSet>()
            .next()
            .map(|s| s.id)
            .ok_or_else(|| ApiError::Invalid("the document has no layer sets".into())),
    }
}

impl Command {
    /// The `op` tag, for error messages.
    pub fn op(&self) -> &'static str {
        match self {
            Command::SetDocumentName { .. } => "set_document_name",
            Command::SetDisplayUnit { .. } => "set_display_unit",
            Command::AddFloor { .. } => "add_floor",
            Command::SwapFloors { .. } => "swap_floors",
            Command::BuildFoundation { .. } => "build_foundation",
            Command::BuildRoof { .. } => "build_roof",
            Command::SetRoofStyle { .. } => "set_roof_style",
            Command::RemoveRoof { .. } => "remove_roof",
            Command::AddRoofPlane { .. } => "add_roof_plane",
            Command::AddStair { .. } => "add_stair",
            Command::AddSchedule { .. } => "add_schedule",
            Command::AddElectrical { .. } => "add_electrical",
            Command::ConnectSwitch { .. } => "connect_switch",
            Command::SetRenovation { .. } => "set_renovation",
            Command::SetNorth { .. } => "set_north",
            Command::SetSun { .. } => "set_sun",
            Command::AddWallType { .. } => "add_wall_type",
            Command::AddLayer { .. } => "add_layer",
            Command::AddLayerSet { .. } => "add_layer_set",
            Command::SetLayerDisplay { .. } => "set_layer_display",
            Command::AddDefaultSet { .. } => "add_default_set",
            Command::SetActiveDefaultSet { .. } => "set_active_default_set",
            Command::AddTextStyle { .. } => "add_text_style",
            Command::AddRoomType { .. } => "add_room_type",
            Command::AddWall { .. } => "add_wall",
            Command::AddWalls { .. } => "add_walls",
            Command::MoveWall { .. } => "move_wall",
            Command::SetWallLength { .. } => "set_wall_length",
            Command::AddDoor { .. } => "add_door",
            Command::AddWindow { .. } => "add_window",
            Command::AddCad { .. } => "add_cad",
            Command::AddDimension { .. } => "add_dimension",
            Command::AddExteriorDimensions { .. } => "add_exterior_dimensions",
            Command::AddText { .. } => "add_text",
            Command::AddPlanView { .. } => "add_plan_view",
            Command::AddCamera { .. } => "add_camera",
            Command::Update { .. } => "update",
            Command::Transform { .. } => "transform",
            Command::Offset { .. } => "offset",
            Command::ExtendTo { .. } => "extend_to",
            Command::Fillet { .. } => "fillet",
            Command::Chamfer { .. } => "chamfer",
            Command::InsertObjects { .. } => "insert_objects",
            Command::Delete { .. } => "delete",
        }
    }

    /// Apply to `doc` in place. On error `doc` may be partially modified, so
    /// callers apply to a scratch copy (see [`crate::Session::apply`]).
    pub(crate) fn apply(&self, doc: &mut Document) -> Result<Outcome, ApiError> {
        let mut out = Outcome::default();
        let d = doc.active_defaults().clone();
        match self {
            Command::SetDocumentName { name } => doc.name = name.clone(),
            Command::SetDisplayUnit { unit } => {
                if let DisplayUnit::FeetInches { denominator } | DisplayUnit::Inches { denominator } =
                    unit
                    && *denominator == 0
                {
                    return invalid("denominator must be at least 1");
                }
                doc.settings.display_unit = *unit;
            }
            Command::AddFloor {
                name,
                elevation,
                ceiling_height,
                floor_structure,
                below,
            } => {
                let structure = floor_structure.unwrap_or_else(default_floor_structure);
                if !(structure.is_finite() && structure.mm() >= 0.0) {
                    return invalid("floor_structure must not be negative");
                }
                let floors: Vec<Floor> = doc.floors_by_elevation().into_iter().cloned().collect();
                let name = name
                    .clone()
                    .unwrap_or_else(|| format!("Floor {}", doc.count::<Floor>() + 1));
                let id = match below {
                    Some(b) => {
                        let anchor = expect::<Floor>(doc, *b)?.clone();
                        let ceiling_height = ceiling_height.unwrap_or(anchor.ceiling_height);
                        let elevation = elevation.unwrap_or(anchor.elevation);
                        // Everything from the anchor floor up moves up by the new storey.
                        let shift = ceiling_height + anchor.floor_structure;
                        for f in floors
                            .iter()
                            .filter(|f| f.elevation.mm() >= anchor.elevation.mm())
                        {
                            if let Some(fm) = doc.get_mut::<Floor>(f.id) {
                                fm.elevation += shift;
                                out.modified.push(f.id);
                            }
                        }
                        doc.insert(Floor {
                            id: Id(0),
                            name,
                            elevation,
                            ceiling_height,
                            floor_structure: structure,
                            foundation: None,
                            auto_roof: false,
                            platform: if floor_structure.is_none() {
                                housecraft_model::default_platform()
                            } else {
                                Vec::new()
                            },
                        })
                    }
                    None => {
                        let top = floors.last();
                        let ceiling_height = ceiling_height
                            .or(top.map(|f| f.ceiling_height))
                            .unwrap_or(Length::from_feet(8.0));
                        let elevation = elevation
                            .or(top.map(|f| f.elevation + f.ceiling_height + structure))
                            .unwrap_or(Length::ZERO);
                        doc.insert(Floor {
                            id: Id(0),
                            name,
                            elevation,
                            ceiling_height,
                            floor_structure: structure,
                            foundation: None,
                            auto_roof: false,
                            platform: if floor_structure.is_none() {
                                housecraft_model::default_platform()
                            } else {
                                Vec::new()
                            },
                        })
                    }
                };
                out.created.push(id);
            }
            Command::BuildFoundation {
                kind,
                wall_height,
                wall_thickness,
                footing_width,
                footing_depth,
                slab_thickness,
                auto_rebuild,
                pier_spacing,
                pier_diameter,
            } => {
                let inch = Length::from_inches;
                let spec = Foundation {
                    kind: *kind,
                    wall_height: wall_height.unwrap_or(match kind {
                        FoundationKind::Slab => inch(4.0),
                        FoundationKind::Crawlspace => inch(30.0),
                        FoundationKind::Basement => inch(96.0),
                        FoundationKind::Piers => inch(24.0),
                    }),
                    wall_thickness: wall_thickness.unwrap_or(inch(8.0)),
                    footing_width: footing_width.unwrap_or(match kind {
                        FoundationKind::Slab => inch(12.0),
                        _ => inch(16.0),
                    }),
                    footing_depth: footing_depth.unwrap_or(match kind {
                        FoundationKind::Slab => inch(12.0),
                        _ => inch(8.0),
                    }),
                    slab_thickness: slab_thickness.unwrap_or(inch(4.0)),
                    auto_rebuild: *auto_rebuild,
                    pier_spacing: pier_spacing.unwrap_or(Length::from_feet(6.0)),
                    pier_diameter: pier_diameter.unwrap_or(inch(12.0)),
                };
                if spec.pier_spacing.mm() < Length::from_feet(2.0).mm() {
                    return invalid("pier_spacing must be at least 2'");
                }
                crate::foundation::build(doc, spec, &mut out)?;
            }
            Command::SetRoofStyle {
                floor,
                style,
                pitch,
                lower_pitch,
                break_height,
            } => {
                crate::roofs::set_style(
                    doc,
                    *floor,
                    *style,
                    *pitch,
                    *lower_pitch,
                    *break_height,
                    &mut out,
                )?;
            }
            Command::BuildRoof {
                floor,
                auto_rebuild,
            } => {
                crate::roofs::build(doc, *floor, *auto_rebuild, &mut out)?;
            }
            Command::RemoveRoof { floor } => crate::roofs::remove(doc, *floor, &mut out)?,
            Command::AddRoofPlane {
                floor,
                points,
                eave_a,
                eave_b,
                eave_height,
                pitch,
            } => crate::roofs::add_plane(
                doc,
                *floor,
                points,
                *eave_a,
                *eave_b,
                *eave_height,
                *pitch,
                &mut out,
            )?,
            Command::AddStair {
                floor,
                start,
                direction,
                shape,
                width,
                rise,
                railing_left,
                railing_right,
                stairwell,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                if !direction.is_finite() {
                    return invalid("direction must be a number of degrees");
                }
                // Stairs starting outside the building use the exterior defaults.
                let inside = housecraft_geom::rooms::detect(doc, floor)
                    .footprints
                    .iter()
                    .any(|f| {
                        housecraft_geom::math::point_in_polygon(housecraft_geom::v2(*start), f)
                    });
                let sd = if inside { &d.stair } else { &d.exterior_stair };
                let mut stair = Stair {
                    id: Id(0),
                    floor,
                    start: *start,
                    direction: *direction,
                    width: width.unwrap_or(sd.width),
                    shape: *shape,
                    rise: *rise,
                    max_riser: sd.max_riser,
                    tread_depth: sd.tread_depth,
                    railing_left: railing_left.unwrap_or(sd.railing_left),
                    railing_right: railing_right.unwrap_or(sd.railing_right),
                    railing_height: sd.railing_height,
                    tread_overhang: sd.tread_overhang,
                    stringers: sd.stringers,
                    stairwell: *stairwell,
                    layer: d.layers.stairs.unwrap_or(d.layers.walls),
                };
                // Split L and U stairs evenly when asked to.
                let risers =
                    ((doc.stair_rise(&stair).mm() / stair.max_riser.mm()).ceil() as u32).max(1);
                let treads = housecraft_geom::stairs::tread_count(risers, &stair.shape);
                match &mut stair.shape {
                    StairShape::L { first_treads, .. } | StairShape::U { first_treads, .. }
                        if *first_treads == 0 =>
                    {
                        *first_treads = (treads / 2).max(1);
                    }
                    _ => {}
                }
                out.created.push(doc.insert(stair));
            }
            Command::AddElectrical {
                floor,
                electrical_kind,
                at,
                rotation,
                height,
                controls,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let mut position = *at;
                let mut angle = rotation.unwrap_or(0.0);
                if electrical_kind.on_wall()
                    && rotation.is_none()
                    && let Some((q, a)) = housecraft_geom::electrical::snap_to_wall(
                        doc,
                        floor,
                        housecraft_geom::v2(*at),
                        Length::from_feet(2.0).mm(),
                    )
                {
                    position = housecraft_geom::pt(q);
                    angle = a;
                }
                if !controls.is_empty() && !electrical_kind.is_switch() {
                    return invalid("only switches control fixtures");
                }
                out.created.push(doc.insert(Electrical {
                    id: Id(0),
                    floor,
                    electrical_kind: *electrical_kind,
                    position,
                    rotation: angle,
                    height: *height,
                    controls: controls.clone(),
                    renovation: Default::default(),
                    layer: d.layers.electrical.unwrap_or(d.layers.cad),
                }));
            }
            Command::ConnectSwitch {
                switch,
                fixtures,
                disconnect,
            } => {
                let sw = expect::<Electrical>(doc, *switch)?;
                if !sw.electrical_kind.is_switch() {
                    return invalid(format!(
                        "{switch} is a {}, not a switch",
                        sw.electrical_kind.name()
                    ));
                }
                for f in fixtures {
                    let fx = expect::<Electrical>(doc, *f)?;
                    if fx.electrical_kind.is_switch() {
                        return invalid(format!("{f} is a switch; connect switches to fixtures"));
                    }
                }
                let sw = doc.get_mut::<Electrical>(*switch).expect("checked");
                for f in fixtures {
                    if *disconnect {
                        sw.controls.retain(|c| c != f);
                    } else if !sw.controls.contains(f) {
                        sw.controls.push(*f);
                    }
                }
                out.modified.push(*switch);
            }
            Command::SetRenovation { ids, status } => {
                for id in ids {
                    let slot = match doc.object_mut(*id).ok_or(ApiError::NotFound(*id))? {
                        Object::Wall(w) => &mut w.renovation,
                        Object::Opening(o) => &mut o.renovation,
                        Object::Electrical(e) => &mut e.renovation,
                        other => {
                            return invalid(format!("a {} has no renovation status", other.kind()));
                        }
                    };
                    *slot = *status;
                    out.modified.push(*id);
                }
            }
            Command::SetNorth { angle, pointer } => {
                if !angle.is_finite() {
                    return invalid("north angle must be a number of degrees");
                }
                doc.settings.north = angle.rem_euclid(360.0);
                doc.settings.north_pointer = *pointer;
            }
            Command::SetSun {
                azimuth,
                altitude,
                latitude,
                day_of_year,
                hour,
                shadows,
            } => {
                let sun = &mut doc.settings.sun;
                match (latitude, day_of_year, hour) {
                    (Some(lat), Some(day), Some(h)) => {
                        if !(-90.0..=90.0).contains(lat)
                            || !(1..=366).contains(day)
                            || !(0.0..=24.0).contains(h)
                        {
                            return invalid("latitude -90..90, day_of_year 1..366, hour 0..24");
                        }
                        let (az, alt) = housecraft_geom::sun::position(*lat, *day, *h);
                        sun.azimuth = az;
                        sun.altitude = alt;
                    }
                    (None, None, None) => {}
                    _ => return invalid("give latitude, day_of_year and hour together"),
                }
                if let Some(a) = azimuth {
                    sun.azimuth = a.rem_euclid(360.0);
                }
                if let Some(a) = altitude {
                    if !(-90.0..=90.0).contains(a) {
                        return invalid("altitude must be between -90 and 90 degrees");
                    }
                    sun.altitude = *a;
                }
                if let Some(s) = shadows {
                    sun.shadows = *s;
                }
            }
            Command::AddSchedule {
                floor,
                schedule_kind,
                position,
                columns,
                all_floors,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let known = crate::schedule::available_columns(*schedule_kind);
                if let Some(bad) = columns
                    .iter()
                    .find(|c| !known.iter().any(|k| k.eq_ignore_ascii_case(c)))
                {
                    return invalid(format!(
                        "{} has no column {bad:?} (has: {})",
                        schedule_kind.name(),
                        known.join(", ")
                    ));
                }
                out.created.push(doc.insert(Schedule {
                    id: Id(0),
                    floor,
                    layer: d.layers.text,
                    position: *position,
                    schedule_kind: *schedule_kind,
                    columns: columns.clone(),
                    all_floors: *all_floors,
                    text_height: None,
                }));
            }
            Command::SwapFloors { a, b } => {
                expect::<Floor>(doc, *a)?;
                expect::<Floor>(doc, *b)?;
                ops::swap_floors(doc, *a, *b, &mut out);
            }
            Command::AddWallType { name, layers } => {
                out.created.push(doc.insert(WallType {
                    id: Id(0),
                    name: name.clone(),
                    layers: layers.clone(),
                    railing: None,
                }));
            }
            Command::AddLayer {
                name,
                color,
                line_weight,
                line_style,
            } => {
                out.created.push(doc.insert(Layer {
                    id: Id(0),
                    name: name.clone(),
                    color: *color,
                    line_weight: line_weight.unwrap_or(0.25),
                    line_style: *line_style,
                    text_style: None,
                    in_materials_list: true,
                }));
            }
            Command::AddLayerSet { name, copy_from } => {
                let layers = match copy_from {
                    Some(src) => expect::<LayerSet>(doc, *src)?.layers.clone(),
                    None => BTreeMap::new(),
                };
                out.created.push(doc.insert(LayerSet {
                    id: Id(0),
                    name: name.clone(),
                    layers,
                }));
            }
            Command::SetLayerDisplay {
                layer_set,
                layer,
                visible,
                color,
                line_weight,
                line_style,
            } => {
                expect::<Layer>(doc, *layer)?;
                expect::<LayerSet>(doc, *layer_set)?;
                let set = doc.get_mut::<LayerSet>(*layer_set).expect("checked");
                let entry = set.layers.entry(*layer).or_default();
                if let Some(v) = visible {
                    entry.visible = *v;
                }
                if color.is_some() {
                    entry.color = *color;
                }
                if line_weight.is_some() {
                    entry.line_weight = *line_weight;
                }
                if line_style.is_some() {
                    entry.line_style = *line_style;
                }
                out.modified.push(*layer_set);
            }
            Command::AddDefaultSet { name, copy_from } => {
                let mut set = match copy_from {
                    Some(src) => expect::<DefaultSet>(doc, *src)?.clone(),
                    None => d.clone(),
                };
                set.name = name.clone();
                out.created.push(doc.insert(set));
            }
            Command::SetActiveDefaultSet { id } => {
                expect::<DefaultSet>(doc, *id)?;
                doc.settings.active_default_set = *id;
            }
            Command::AddTextStyle {
                name,
                height,
                font,
                bold,
                italic,
                color,
            } => {
                out.created.push(doc.insert(TextStyle {
                    id: Id(0),
                    name: name.clone(),
                    height: *height,
                    font: font.clone().unwrap_or_else(|| "Sans".into()),
                    bold: *bold,
                    italic: *italic,
                    color: *color,
                }));
            }
            Command::AddRoomType {
                name,
                ceiling_height,
                floor_finish,
                living_area,
            } => {
                out.created.push(doc.insert(RoomType {
                    id: Id(0),
                    name: name.clone(),
                    ceiling_height: *ceiling_height,
                    floor_finish: floor_finish.clone(),
                    wall_finish: String::new(),
                    living_area: *living_area,
                }));
            }
            Command::AddWall {
                floor,
                start,
                end,
                wall_type,
                interior,
                height,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let wt = ops::resolve_wall_type(doc, &d, *wall_type, *interior)?;
                out.created.push(ops::add_wall(
                    doc,
                    floor,
                    *start,
                    *end,
                    wt,
                    *height,
                    d.layers.walls,
                )?);
            }
            Command::AddWalls {
                floor,
                points,
                closed,
                wall_type,
                interior,
                height,
            } => {
                if points.len() < 2 {
                    return invalid("add_walls needs at least two points");
                }
                if *closed && points.len() < 3 {
                    return invalid("a closed run needs at least three points");
                }
                let floor = ops::resolve_floor(doc, *floor)?;
                let wt = ops::resolve_wall_type(doc, &d, *wall_type, *interior)?;
                let mut segments: Vec<_> = points.windows(2).map(|w| (w[0], w[1])).collect();
                if *closed {
                    segments.push((*points.last().expect("len >= 3"), points[0]));
                }
                for (a, b) in segments {
                    out.created.push(ops::add_wall(
                        doc,
                        floor,
                        a,
                        b,
                        wt,
                        *height,
                        d.layers.walls,
                    )?);
                }
            }
            Command::MoveWall {
                id,
                start,
                end,
                detach,
            } => {
                let w = expect::<Wall>(doc, *id)?.clone();
                ops::move_wall_ends(
                    doc,
                    &w,
                    start.unwrap_or(w.start),
                    end.unwrap_or(w.end),
                    !*detach,
                    &mut out,
                );
            }
            Command::SetWallLength { id, length, anchor } => {
                positive(*length, "length")?;
                let w = expect::<Wall>(doc, *id)?.clone();
                let (s, e) = (housecraft_geom::v2(w.start), housecraft_geom::v2(w.end));
                let dir = (e - s).normalize_or_zero();
                let l = length.mm();
                let (ns, ne) = match anchor {
                    LengthAnchor::Start => (s, s + dir * l),
                    LengthAnchor::End => (e - dir * l, e),
                    LengthAnchor::Center => {
                        let c = (s + e) / 2.0;
                        (c - dir * l / 2.0, c + dir * l / 2.0)
                    }
                };
                ops::move_wall_ends(
                    doc,
                    &w,
                    housecraft_geom::pt(ns),
                    housecraft_geom::pt(ne),
                    true,
                    &mut out,
                );
            }
            Command::AddDoor {
                wall,
                position,
                at,
                style,
                width,
                height,
                hinge,
                swing_in,
            } => {
                let w = expect::<Wall>(doc, *wall)?.clone();
                let style = style.unwrap_or(d.door.style);
                let (dw, dh) = d.door.size_for(style);
                let width = width.unwrap_or(dw);
                let height = height.unwrap_or(dh);
                let position = ops::opening_position(&w, *position, *at, width)?;
                out.created.push(doc.insert(Opening {
                    id: Id(0),
                    wall: *wall,
                    opening_kind: OpeningKind::Door(style),
                    position,
                    width,
                    height,
                    sill: Length::ZERO,
                    hinge: hinge.unwrap_or(d.door.hinge),
                    swing_in: swing_in.unwrap_or(true),
                    open_percent: 0.0,
                    plan_swing: 90.0,
                    renovation: Default::default(),
                    layer: d.layers.doors,
                }));
            }
            Command::AddWindow {
                wall,
                position,
                at,
                style,
                width,
                height,
                sill,
            } => {
                let w = expect::<Wall>(doc, *wall)?.clone();
                let width = width.unwrap_or(d.window.width);
                let height = height.unwrap_or(d.window.height);
                let position = ops::opening_position(&w, *position, *at, width)?;
                let sill = sill.unwrap_or(d.window.head_height - height);
                out.created.push(doc.insert(Opening {
                    id: Id(0),
                    wall: *wall,
                    opening_kind: OpeningKind::Window(style.unwrap_or(d.window.style)),
                    position,
                    width,
                    height,
                    sill,
                    hinge: Hinge::Left,
                    swing_in: true,
                    open_percent: 0.0,
                    plan_swing: 90.0,
                    renovation: Default::default(),
                    layer: d.layers.windows,
                }));
            }
            Command::AddCad {
                floor,
                layer,
                shape,
                color,
                fill,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                out.created.push(doc.insert(Cad {
                    id: Id(0),
                    floor,
                    layer: layer.unwrap_or(d.layers.cad),
                    shape: shape.clone(),
                    color: *color,
                    fill: *fill,
                }));
            }
            Command::AddDimension {
                floor,
                layer,
                geometry,
                text_override,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                out.created.push(doc.insert(Dimension {
                    id: Id(0),
                    floor,
                    layer: layer.unwrap_or(d.layers.dimensions),
                    geometry: geometry.clone(),
                    text_override: text_override.clone(),
                }));
            }
            Command::AddExteriorDimensions { floor } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                ops::exterior_dimensions(doc, floor, &d, &mut out)?;
            }
            Command::AddText {
                floor,
                layer,
                position,
                text,
                style,
                rotation,
                justify,
                height,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                out.created.push(doc.insert(Text {
                    id: Id(0),
                    floor,
                    layer: layer.unwrap_or(d.layers.text),
                    position: *position,
                    text: text.clone(),
                    style: *style,
                    rotation: *rotation,
                    justify: *justify,
                    height: *height,
                    bold: None,
                    color: None,
                }));
            }
            Command::AddPlanView {
                name,
                floor,
                layer_set,
                default_set,
                zoom,
                reference_floor,
                references,
                phase,
            } => {
                let layer_set = first_layer_set(doc, *layer_set)?;
                out.created.push(doc.insert(PlanView {
                    id: Id(0),
                    name: name.clone(),
                    floor: *floor,
                    layer_set,
                    default_set: default_set.unwrap_or(d.id),
                    zoom: zoom.clone(),
                    reference_floor: *reference_floor,
                    references: references.clone(),
                    phase: *phase,
                }));
            }
            Command::AddCamera {
                name,
                projection,
                eye,
                target,
                layer_set,
            } => {
                let layer_set = first_layer_set(doc, *layer_set)?;
                out.created.push(doc.insert(Camera {
                    id: Id(0),
                    name: name.clone(),
                    projection: projection.clone(),
                    eye: *eye,
                    target: *target,
                    layer_set,
                }));
            }
            Command::Update { id, set } => ops::update(doc, *id, set, &mut out)?,
            Command::Transform {
                ids,
                transform,
                copies,
            } => ops::transform(doc, ids, transform, *copies, &mut out)?,
            Command::Offset { id, distance } => ops::offset(doc, *id, *distance, &mut out)?,
            Command::ExtendTo { id, boundary } => ops::extend_to(doc, *id, *boundary, &mut out)?,
            Command::Fillet { a, b, radius } => ops::fillet(doc, *a, *b, *radius, &mut out)?,
            Command::Chamfer { a, b, distance } => {
                positive(*distance, "distance")?;
                ops::chamfer(doc, *a, *b, *distance, &mut out)?
            }
            Command::InsertObjects {
                objects,
                floor,
                dx,
                dy,
            } => ops::insert_objects(doc, objects, *floor, *dx, *dy, &mut out)?,
            Command::Delete { ids } => ops::delete(doc, ids, &mut out)?,
        }
        Ok(out)
    }
}
