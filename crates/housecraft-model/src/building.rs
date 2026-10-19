//! Architectural objects: floors, walls, openings, rooms.

use housecraft_units::{Length, Point2, Point3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Id;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Floor {
    pub id: Id,
    pub name: String,
    /// Elevation of the finished floor surface above the project datum.
    pub elevation: Length,
    /// Finished floor to finished ceiling.
    pub ceiling_height: Length,
    /// Depth of the floor platform (subfloor + joists) under this floor's
    /// finished surface; the floor sits on the ceiling of the floor below.
    #[serde(default = "default_floor_structure")]
    pub floor_structure: Length,
    /// Layers of the floor platform, top (finish) first. When present,
    /// `floor_structure` is kept equal to their total.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platform: Vec<WallLayer>,
    /// Set when this floor is the foundation under the floor above it.
    #[serde(default)]
    pub foundation: Option<Foundation>,
    /// Regenerate this floor's roof from its walls whenever they change.
    #[serde(default)]
    pub auto_roof: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FoundationKind {
    /// Slab on grade with a thickened (footed) edge; floor 1 sits on the slab.
    Slab,
    /// Stem walls on footings around an unexcavated crawlspace.
    Crawlspace,
    /// Full-height walls on footings around a slab; a habitable level.
    Basement,
    /// Round piers on pad footings carrying beams under the floor.
    Piers,
}

/// How a foundation floor was built, so it can be rebuilt when the floor above changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Foundation {
    pub kind: FoundationKind,
    /// Foundation wall height (slab thickness for slabs).
    pub wall_height: Length,
    pub wall_thickness: Length,
    pub footing_width: Length,
    pub footing_depth: Length,
    pub slab_thickness: Length,
    /// Regenerate the foundation walls whenever the floor above changes outline.
    pub auto_rebuild: bool,
    /// Pier foundations: centre-to-centre spacing along the beams.
    #[serde(default = "pier_spacing")]
    pub pier_spacing: Length,
    #[serde(default = "pier_diameter")]
    pub pier_diameter: Length,
}

fn pier_spacing() -> Length {
    Length::from_feet(6.0)
}

fn pier_diameter() -> Length {
    Length::from_inches(12.0)
}

/// A concrete footing centred under a wall.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Footing {
    pub width: Length,
    pub depth: Length,
}

/// A typical framed floor: finish, subfloor and 2x12 joists (12 5/8" total).
pub fn default_platform() -> Vec<WallLayer> {
    let layer = |name: &str, inches: f64, material: &str| WallLayer {
        name: name.into(),
        thickness: Length::from_inches(inches),
        material: material.into(),
    };
    vec![
        layer("Finish floor", 0.75, "Hardwood"),
        layer("Subfloor", 0.75, "Plywood"),
        layer("Joists", 11.125, "2x12 joist"),
    ]
}

/// 3/4" subfloor on 11 7/8" I-joists.
pub fn default_floor_structure() -> Length {
    Length::from_inches(12.625)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallType {
    pub id: Id,
    pub name: String,
    /// Layers listed exterior to interior.
    pub layers: Vec<WallLayer>,
    /// Makes walls of this type railings (rail and balusters) instead of solid walls.
    #[serde(default)]
    pub railing: Option<Railing>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Railing {
    /// Top of the handrail above the floor (or the stair nosing line).
    pub height: Length,
    /// Centre-to-centre baluster spacing.
    pub baluster_spacing: Length,
}

impl Default for Railing {
    fn default() -> Self {
        Railing {
            height: Length::from_inches(36.0),
            baluster_spacing: Length::from_inches(4.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Turn {
    Left,
    Right,
}

/// Plan shape of a stair.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StairShape {
    Straight,
    /// Turns 90° at a square landing after `first_treads` treads.
    L {
        turn: Turn,
        first_treads: u32,
    },
    /// Turns back 180° at a landing after `first_treads` treads; `gap` separates the flights.
    U {
        turn: Turn,
        first_treads: u32,
        #[serde(default)]
        gap: Length,
    },
}

/// Remodel status: what is there now, what is being added, what comes out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Renovation {
    Existing,
    #[default]
    New,
    Demolish,
}

impl Renovation {
    pub const ALL: [Renovation; 3] = [Renovation::Existing, Renovation::New, Renovation::Demolish];

    pub fn name(self) -> &'static str {
        match self {
            Renovation::Existing => "Existing",
            Renovation::New => "New",
            Renovation::Demolish => "Demolish",
        }
    }
}
/// Electrical devices and fixtures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ElectricalKind {
    Outlet,
    GfciOutlet,
    /// 240 V appliance receptacle.
    Outlet240,
    Switch,
    Dimmer,
    CeilingLight,
    RecessedLight,
    WallLight,
    CeilingFan,
    SmokeDetector,
    JunctionBox,
    // Plumbing.
    HoseBibb,
    FloorDrain,
    GasOutlet,
    // HVAC.
    SupplyRegister,
    ReturnGrille,
    Thermostat,
    ExhaustFan,
}

impl ElectricalKind {
    pub const ALL: [ElectricalKind; 18] = [
        ElectricalKind::Outlet,
        ElectricalKind::GfciOutlet,
        ElectricalKind::Outlet240,
        ElectricalKind::Switch,
        ElectricalKind::Dimmer,
        ElectricalKind::CeilingLight,
        ElectricalKind::RecessedLight,
        ElectricalKind::WallLight,
        ElectricalKind::CeilingFan,
        ElectricalKind::SmokeDetector,
        ElectricalKind::JunctionBox,
        ElectricalKind::HoseBibb,
        ElectricalKind::FloorDrain,
        ElectricalKind::GasOutlet,
        ElectricalKind::SupplyRegister,
        ElectricalKind::ReturnGrille,
        ElectricalKind::Thermostat,
        ElectricalKind::ExhaustFan,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ElectricalKind::Outlet => "Duplex Outlet",
            ElectricalKind::GfciOutlet => "GFCI Outlet",
            ElectricalKind::Outlet240 => "240V Outlet",
            ElectricalKind::Switch => "Switch",
            ElectricalKind::Dimmer => "Dimmer Switch",
            ElectricalKind::CeilingLight => "Ceiling Light",
            ElectricalKind::RecessedLight => "Recessed Light",
            ElectricalKind::WallLight => "Wall Light",
            ElectricalKind::CeilingFan => "Ceiling Fan",
            ElectricalKind::SmokeDetector => "Smoke Detector",
            ElectricalKind::JunctionBox => "Junction Box",
            ElectricalKind::HoseBibb => "Hose Bibb",
            ElectricalKind::FloorDrain => "Floor Drain",
            ElectricalKind::GasOutlet => "Gas Outlet",
            ElectricalKind::SupplyRegister => "Supply Register",
            ElectricalKind::ReturnGrille => "Return Air Grille",
            ElectricalKind::Thermostat => "Thermostat",
            ElectricalKind::ExhaustFan => "Exhaust Fan",
        }
    }

    /// Trade the symbol belongs to, for grouping in menus and lists.
    pub fn trade(self) -> &'static str {
        match self {
            ElectricalKind::HoseBibb | ElectricalKind::FloorDrain | ElectricalKind::GasOutlet => {
                "Plumbing"
            }
            ElectricalKind::SupplyRegister
            | ElectricalKind::ReturnGrille
            | ElectricalKind::Thermostat
            | ElectricalKind::ExhaustFan => "HVAC",
            _ => "Electrical",
        }
    }

    /// Light fixtures (ceiling fans have a light kit).
    pub fn emits_light(self) -> bool {
        matches!(
            self,
            ElectricalKind::CeilingLight
                | ElectricalKind::RecessedLight
                | ElectricalKind::WallLight
                | ElectricalKind::CeilingFan
        )
    }

    /// Typical light output, lumens.
    pub fn typical_lumens(self) -> f64 {
        match self {
            ElectricalKind::RecessedLight => 650.0,
            ElectricalKind::WallLight => 450.0,
            _ => 1100.0,
        }
    }

    pub fn is_switch(self) -> bool {
        matches!(self, ElectricalKind::Switch | ElectricalKind::Dimmer)
    }

    /// Mounted on a wall face (snaps to walls when placed).
    pub fn on_wall(self) -> bool {
        matches!(
            self,
            ElectricalKind::Outlet
                | ElectricalKind::GfciOutlet
                | ElectricalKind::Outlet240
                | ElectricalKind::Switch
                | ElectricalKind::Dimmer
                | ElectricalKind::WallLight
                | ElectricalKind::HoseBibb
                | ElectricalKind::GasOutlet
                | ElectricalKind::ReturnGrille
                | ElectricalKind::Thermostat
        )
    }

    /// Typical mounting height to the centre of the device (0 = in the
    /// floor); `None` for ceiling-mounted fixtures, which sit at the ceiling.
    pub fn default_height(self) -> Option<Length> {
        Some(Length::from_inches(match self {
            ElectricalKind::Outlet | ElectricalKind::GfciOutlet => 12.0,
            ElectricalKind::Outlet240 => 18.0,
            ElectricalKind::Switch | ElectricalKind::Dimmer => 48.0,
            ElectricalKind::WallLight => 72.0,
            ElectricalKind::HoseBibb => 18.0,
            ElectricalKind::GasOutlet => 12.0,
            ElectricalKind::ReturnGrille => 12.0,
            ElectricalKind::Thermostat => 60.0,
            ElectricalKind::FloorDrain => 0.0,
            _ => return None,
        }))
    }
}

/// An electrical device or fixture. Switches list the fixtures they control;
/// two or more switches sharing a fixture become 3-way and 4-way switches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Electrical {
    pub id: Id,
    pub floor: Id,
    pub electrical_kind: ElectricalKind,
    /// Plan position (on the wall face for wall-mounted devices).
    pub position: Point2,
    /// Degrees; wall devices face this direction (into the room).
    #[serde(default)]
    pub rotation: f64,
    /// Height above the floor; `None` uses the kind's default (ceiling for fixtures).
    #[serde(default)]
    pub height: Option<Length>,
    /// Fixtures this switch controls (switches only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<Id>,
    #[serde(default)]
    pub renovation: Renovation,
    pub layer: Id,
    /// How a light fixture lights the 3D view; `None` uses the kind's typical
    /// output, on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<LightSettings>,
}

/// A light fixture's output.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LightSettings {
    pub on: bool,
    /// Full output, lumens.
    pub lumens: f64,
    /// Dimmed to this fraction of full output, 0–1.
    #[serde(default = "full_dimmer")]
    pub dimmer: f64,
    /// Colour temperature, kelvin (2700 warm white – 6500 daylight).
    #[serde(default = "warm_white")]
    pub kelvin: f64,
    /// Casts shadows in renders.
    #[serde(default = "yes_shadows")]
    pub shadows: bool,
}

fn full_dimmer() -> f64 {
    1.0
}

fn warm_white() -> f64 {
    2700.0
}

fn yes_shadows() -> bool {
    true
}

impl LightSettings {
    pub fn typical(kind: ElectricalKind) -> Self {
        LightSettings {
            on: true,
            lumens: kind.typical_lumens(),
            dimmer: 1.0,
            kelvin: 2700.0,
            shadows: true,
        }
    }

    /// The light's colour (linear RGB, 0–1) from its colour temperature.
    pub fn rgb(&self) -> [f64; 3] {
        // Tanner Helland's blackbody fit, normalised to the brightest channel.
        let t = (self.kelvin.clamp(1000.0, 40000.0)) / 100.0;
        let r = if t <= 66.0 { 255.0 } else { 329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2) };
        let g = if t <= 66.0 {
            99.470_802_586_1 * t.ln() - 161.119_568_166_1
        } else {
            288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2)
        };
        let b = if t >= 66.0 {
            255.0
        } else if t <= 19.0 {
            0.0
        } else {
            138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
        };
        let c = |v: f64| (v / 255.0).clamp(0.0, 1.0);
        [c(r), c(g), c(b)]
    }
}
/// A stair from its floor up to the floor above.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stair {
    pub id: Id,
    /// The floor the stair starts on.
    pub floor: Id,
    /// Centre of the first riser (the bottom of the stair).
    pub start: Point2,
    /// Direction of travel going up, degrees counter-clockwise from east.
    pub direction: f64,
    pub width: Length,
    pub shape: StairShape,
    /// Total rise; `None` reaches the finished floor above.
    #[serde(default)]
    pub rise: Option<Length>,
    pub max_riser: Length,
    pub tread_depth: Length,
    pub railing_left: bool,
    pub railing_right: bool,
    pub railing_height: Length,
    /// How far each tread's nosing projects past the riser below it.
    #[serde(default = "one_inch")]
    pub tread_overhang: Length,
    /// Side stringers under the treads.
    #[serde(default = "yes")]
    pub stringers: bool,
    /// Cut the floor platform above (and the ceiling below) over the stair.
    pub stairwell: bool,
    pub layer: Id,
}

impl WallType {
    pub fn thickness(&self) -> Length {
        self.layers.iter().map(|l| l.thickness).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallLayer {
    pub name: String,
    pub thickness: Length,
    #[serde(default)]
    pub material: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wall {
    pub id: Id,
    pub floor: Id,
    /// Wall centreline start, in plan coordinates.
    pub start: Point2,
    /// Wall centreline end. With layers listed exterior→interior, the exterior
    /// face is on the left when looking from start to end.
    pub end: Point2,
    pub wall_type: Id,
    /// Overrides the floor's ceiling height when set.
    pub height: Option<Length>,
    pub layer: Id,
    #[serde(default)]
    pub footing: Option<Footing>,
    /// How the roof meets this wall when roofs are built automatically.
    #[serde(default)]
    pub roof: RoofEdge,
    #[serde(default)]
    pub renovation: Renovation,
}

/// Per-wall roof settings; `None` values come from the default set.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct RoofEdge {
    /// A full gable: the roof does not slope up from this wall; the wall rises to the ridge.
    #[serde(default)]
    pub gable: bool,
    /// Roof pitch in degrees above horizontal.
    #[serde(default)]
    pub pitch: Option<f64>,
    /// Eave overhang beyond the wall's outer face (rake overhang for gables).
    #[serde(default)]
    pub overhang: Option<Length>,
    /// A second pitch above a break (mansard, gambrel, Dutch gable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper: Option<UpperRoof>,
}

/// The upper part of a roof that changes pitch partway up.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpperRoof {
    /// Height above the wall plate where the pitch changes.
    pub break_height: Length,
    /// Pitch above the break, degrees.
    pub pitch: f64,
    /// Above the break this edge becomes a gable (Dutch gable).
    #[serde(default)]
    pub gable: bool,
}

/// Whole-roof styles that set every exterior wall's roof edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoofStyle {
    Hip,
    Gable,
    Shed,
    Mansard,
    Gambrel,
    DutchGable,
}

impl RoofStyle {
    pub const ALL: [RoofStyle; 6] = [
        RoofStyle::Hip,
        RoofStyle::Gable,
        RoofStyle::Shed,
        RoofStyle::Mansard,
        RoofStyle::Gambrel,
        RoofStyle::DutchGable,
    ];

    pub fn name(self) -> &'static str {
        match self {
            RoofStyle::Hip => "Hip",
            RoofStyle::Gable => "Gable",
            RoofStyle::Shed => "Shed",
            RoofStyle::Mansard => "Mansard",
            RoofStyle::Gambrel => "Gambrel",
            RoofStyle::DutchGable => "Dutch Gable",
        }
    }
}

/// One plane of a roof, or the vertical gable infill above a gable wall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoofPlane {
    pub id: Id,
    /// The floor whose walls the roof sits on.
    pub floor: Id,
    /// Top surface outline, absolute coordinates.
    pub outline: Vec<Point3>,
    /// Degrees above horizontal (90 for gable infill).
    pub pitch: f64,
    pub thickness: Length,
    /// Vertical wall infill above a gable wall rather than a sloped plane.
    #[serde(default)]
    pub gable: bool,
    /// Generated by `build_roof`; regenerated while the floor's `auto_roof` is on.
    #[serde(default)]
    pub auto: bool,
    pub layer: Id,
}

impl Wall {
    pub fn length(&self) -> Length {
        self.start.distance(self.end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DoorStyle {
    Hinged,
    Double,
    Sliding,
    Pocket,
    Bifold,
    /// A cased opening with no door leaf.
    Doorway,
    /// Sectional overhead garage door.
    Garage,
    /// A surface-mounted sliding barn door, hung on the swing side.
    Barn,
    /// A pair with one operable leaf beside a fixed one.
    Atrium,
    /// A frameless glass hinged door.
    Shower,
    /// A fixed glass panel in a door frame.
    Fixed,
}

impl DoorStyle {
    pub const ALL: [DoorStyle; 11] = [
        DoorStyle::Hinged,
        DoorStyle::Double,
        DoorStyle::Atrium,
        DoorStyle::Sliding,
        DoorStyle::Pocket,
        DoorStyle::Bifold,
        DoorStyle::Barn,
        DoorStyle::Garage,
        DoorStyle::Shower,
        DoorStyle::Fixed,
        DoorStyle::Doorway,
    ];

    pub fn name(self) -> &'static str {
        match self {
            DoorStyle::Hinged => "Hinged Door",
            DoorStyle::Double => "Double Door",
            DoorStyle::Sliding => "Sliding Door",
            DoorStyle::Pocket => "Pocket Door",
            DoorStyle::Bifold => "Bifold Door",
            DoorStyle::Doorway => "Doorway",
            DoorStyle::Garage => "Garage Door",
            DoorStyle::Barn => "Barn Door",
            DoorStyle::Atrium => "Atrium Door",
            DoorStyle::Shower => "Shower Door",
            DoorStyle::Fixed => "Fixed Glass Door",
        }
    }

    /// Typical width and height for styles that differ from a single door;
    /// `None` uses the default set's door size.
    pub fn typical_size(self) -> Option<(Length, Length)> {
        match self {
            DoorStyle::Double => Some((Length::from_inches(60.0), Length::from_inches(80.0))),
            DoorStyle::Garage => Some((Length::from_feet(16.0), Length::from_feet(7.0))),
            DoorStyle::Atrium => Some((Length::from_inches(60.0), Length::from_inches(80.0))),
            DoorStyle::Shower => Some((Length::from_inches(24.0), Length::from_inches(72.0))),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WindowStyle {
    Fixed,
    Casement,
    SingleHung,
    DoubleHung,
    Sliding,
    /// Hinged at the top, opening outward.
    Awning,
    /// Hinged at the bottom, opening inward.
    Hopper,
}

impl WindowStyle {
    pub const ALL: [WindowStyle; 7] = [
        WindowStyle::DoubleHung,
        WindowStyle::SingleHung,
        WindowStyle::Casement,
        WindowStyle::Sliding,
        WindowStyle::Awning,
        WindowStyle::Hopper,
        WindowStyle::Fixed,
    ];

    pub fn name(self) -> &'static str {
        match self {
            WindowStyle::Fixed => "Fixed Glass Window",
            WindowStyle::Casement => "Casement Window",
            WindowStyle::SingleHung => "Single Hung Window",
            WindowStyle::DoubleHung => "Double Hung Window",
            WindowStyle::Sliding => "Sliding Window",
            WindowStyle::Awning => "Awning Window",
            WindowStyle::Hopper => "Hopper Window",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type", content = "style")]
pub enum OpeningKind {
    Door(DoorStyle),
    Window(WindowStyle),
}

impl OpeningKind {
    pub fn is_door(self) -> bool {
        matches!(self, OpeningKind::Door(_))
    }
}

fn one_inch() -> Length {
    Length::from_inches(1.0)
}

fn yes() -> bool {
    true
}

fn ninety() -> f64 {
    90.0
}

/// Which jamb a door is hinged on, seen from the side it swings toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Hinge {
    Left,
    Right,
}

/// A door or window placed in a wall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Opening {
    pub id: Id,
    pub wall: Id,
    /// Door or window, and its style. (Not `kind`: that tags every object.)
    pub opening_kind: OpeningKind,
    /// Distance along the wall from its start to the centre of the opening.
    pub position: Length,
    pub width: Length,
    pub height: Length,
    /// Height of the bottom of the opening above the floor (0 for doors).
    pub sill: Length,
    pub hinge: Hinge,
    /// Door swings toward the wall's right (interior) side.
    pub swing_in: bool,
    /// How far the door is open in 3D, 0 (closed) to 100 (fully open).
    #[serde(default)]
    pub open_percent: f64,
    /// Angle the leaf is drawn open in plan, degrees (90 = square to the wall).
    #[serde(default = "ninety")]
    pub plan_swing: f64,
    #[serde(default)]
    pub renovation: Renovation,
    pub layer: Id,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoomType {
    pub id: Id,
    pub name: String,
    /// Default ceiling height for rooms of this type; `None` uses the floor's.
    pub ceiling_height: Option<Length>,
    #[serde(default)]
    pub floor_finish: String,
    #[serde(default)]
    pub wall_finish: String,
    /// Counted in the living-area total.
    pub living_area: bool,
}

/// A room is an enclosed region of walls. Rooms are created and removed
/// automatically as walls change; their properties are user-editable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Room {
    pub id: Id,
    pub floor: Id,
    /// A point inside the room; identifies which wall-enclosed region it is.
    pub anchor: Point2,
    /// Overrides the room type's name as the label.
    pub name: Option<String>,
    pub room_type: Id,
    pub ceiling_height: Option<Length>,
    pub floor_finish: Option<String>,
    pub layer: Id,
    #[serde(default)]
    pub ceiling: CeilingStyle,
}

/// The shape of a room's ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "style", rename_all = "snake_case")]
pub enum CeilingStyle {
    /// Flat at the room's ceiling height.
    #[default]
    Flat,
    /// Follows the underside of the roof above (flat until a roof exists).
    Vaulted,
    /// A raised centre: the flat ceiling steps up `rise` at `inset` from the walls.
    Tray { inset: Length, rise: Length },
    /// Open to the floor above (no ceiling or platform drawn).
    None,
    /// A flat ceiling with a grid of beams `depth` deep every `spacing`.
    Coffered {
        spacing: Length,
        beam_width: Length,
        depth: Length,
    },
    /// A half-round vault along the room's long axis, rising `rise` at the middle.
    Barrel { rise: Length },
}
