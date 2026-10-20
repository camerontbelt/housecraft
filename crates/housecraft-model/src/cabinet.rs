//! Cabinets, countertops, appliances and fixtures.
//!
//! A cabinet is placed by the centre of its back edge (`position`) and the
//! direction its front faces (`facing`, degrees). Width runs along the
//! back, depth from back to front.

use housecraft_units::{Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Id, Renovation};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CabinetKind {
    /// On the floor, under a countertop.
    #[default]
    Base,
    /// Hung on the wall above the counter.
    Wall,
    /// Floor to the top of the wall cabinets (pantry, oven tower).
    Full,
}

impl CabinetKind {
    pub fn name(self) -> &'static str {
        match self {
            CabinetKind::Base => "Base Cabinet",
            CabinetKind::Wall => "Wall Cabinet",
            CabinetKind::Full => "Full Height Cabinet",
        }
    }
}

/// Face frame around the openings, or frameless (European) full-overlay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CabinetConstruction {
    #[default]
    Framed,
    Frameless,
}

/// The cabinet's footprint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CabinetShape {
    /// A rectangle.
    #[default]
    Standard,
    /// Front corners cut back at 45° by these amounts (an angled end or a
    /// diagonal corner cabinet).
    Angled { left: Length, right: Length },
    /// The front bows out by `depth` at the middle.
    Bow { depth: Length },
    /// The whole cabinet curves: its back follows an arc of `radius`
    /// (positive: the front is convex).
    Curved { radius: Length },
}

/// What fills one cell of the cabinet front.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FaceItem {
    /// A door hinged on the left (seen from the front).
    DoorLeft,
    /// A door hinged on the right.
    #[default]
    DoorRight,
    /// A pair of doors.
    DoubleDoor,
    Drawer,
    /// Open shelves.
    Open,
    /// A fixed panel (false drawer front under a sink).
    Panel,
    /// A glass door hinged on the left (wall cabinets).
    GlassDoor,
    /// A built-in appliance face (oven, microwave, dishwasher panel).
    Appliance,
}

/// A horizontal band of the cabinet front, split into equal cells.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FaceRow {
    /// Band height; bands without one share what is left.
    #[serde(default)]
    pub height: Option<Length>,
    pub items: Vec<FaceItem>,
    /// Shelves behind this band's doors; the cabinet's count when `None`.
    #[serde(default)]
    pub shelves: Option<u32>,
}

/// Door and drawer front style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DoorStyleCab {
    /// Flat slab.
    Slab,
    /// Frame-and-panel with a recessed centre.
    #[default]
    Shaker,
    /// Frame-and-panel with a raised centre.
    Raised,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HardwareStyle {
    None,
    Knob,
    /// Bar pull; vertical on doors, horizontal on drawers.
    #[default]
    Pull,
}

/// A countertop edge profile (drawn in 3D as a chamfer or round-over).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EdgeProfile {
    Square,
    /// Slightly softened.
    #[default]
    Eased,
    Bevel,
    Bullnose,
    Ogee,
}

impl EdgeProfile {
    pub const ALL: [EdgeProfile; 5] = [
        EdgeProfile::Square,
        EdgeProfile::Eased,
        EdgeProfile::Bevel,
        EdgeProfile::Bullnose,
        EdgeProfile::Ogee,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EdgeProfile::Square => "Square",
            EdgeProfile::Eased => "Eased",
            EdgeProfile::Bevel => "Bevel",
            EdgeProfile::Bullnose => "Bullnose",
            EdgeProfile::Ogee => "Ogee",
        }
    }
}

/// A cut-out in a countertop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HoleKind {
    /// Undermount sink bowl.
    Sink,
    /// Drop-in cooktop.
    Cooktop,
    /// Round hole (faucet, soap dispenser, bar sink).
    Round,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CounterHole {
    pub hole_kind: HoleKind,
    /// Centre of the hole in plan.
    pub center: Point2,
    /// Size across (diameter for round holes).
    pub width: Length,
    /// Size front to back (ignored for round holes).
    pub depth: Length,
    /// Degrees; the hole's width runs along this direction.
    #[serde(default)]
    pub rotation: f64,
}

/// A base or full cabinet's own countertop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CabinetTop {
    pub thickness: Length,
    /// Overhang past the front (and exposed ends).
    pub overhang: Length,
    #[serde(default)]
    pub edge: EdgeProfile,
    /// Backsplash height along the back; none when zero.
    #[serde(default)]
    pub backsplash: Length,
    /// Rounded or chamfered front corners.
    #[serde(default)]
    pub corner: CornerTreatment,
    #[serde(default)]
    pub corner_left: bool,
    #[serde(default)]
    pub corner_right: bool,
    /// The countertop turns down the exposed end to the floor.
    #[serde(default)]
    pub waterfall_left: bool,
    #[serde(default)]
    pub waterfall_right: bool,
}

/// How a corner is finished.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CornerTreatment {
    #[default]
    Square,
    Round { radius: Length },
    Chamfer { size: Length },
}

/// Which parts the cabinet box is built from and how thick they are; how
/// doors sit on the box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CabinetBuild {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
    pub back: bool,
    pub side_thickness: Length,
    pub back_thickness: Length,
    /// Framed: how far doors overlap the face frame at the left, right,
    /// top and bottom.
    pub overlay: [Length; 4],
    /// Frameless: the gap around and between doors.
    pub reveal: Length,
    /// Face-frame stiles run down to the floor past the toe kick.
    #[serde(default)]
    pub extended_stiles: bool,
}

impl Default for CabinetBuild {
    fn default() -> Self {
        let i = Length::from_inches;
        CabinetBuild {
            top: true,
            bottom: true,
            left: true,
            right: true,
            back: true,
            side_thickness: i(0.75),
            back_thickness: i(0.25),
            overlay: [i(0.5); 4],
            reveal: i(0.125),
            extended_stiles: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    #[default]
    Left,
    Right,
}

/// Part of the front hidden by a cabinet butting in at a corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlindCorner {
    pub side: Side,
    pub width: Length,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MoldingPlace {
    /// Crown at the top.
    #[default]
    Top,
    /// Light rail under a wall cabinet.
    Bottom,
}

/// A cabinet molding profile shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MoldingShape {
    /// Sloped crown.
    #[default]
    Crown,
    /// Square-edged board.
    Flat,
    /// Quarter-round cove.
    Cove,
    /// Bead along the bottom edge.
    Bead,
}

/// A molding along the front and exposed ends; several at one place stack
/// outward from the box (auto-stacking).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CabinetMolding {
    pub place: MoldingPlace,
    pub shape: MoldingShape,
    pub height: Length,
    /// How far it stands out from the box.
    pub projection: Length,
    /// Set into the box face instead of above/below it.
    #[serde(default)]
    pub flush: bool,
}

/// Furniture-style ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EndPanels {
    pub left: bool,
    pub right: bool,
    /// Legs at the corners instead of a toe kick.
    #[serde(default)]
    pub legs: bool,
    #[serde(default)]
    pub leg_style: LegStyle,
    /// Pilasters (decorative columns) at the front corners.
    #[serde(default)]
    pub pilasters: bool,
}

/// The shape of furniture legs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LegStyle {
    #[default]
    Square,
    /// Round, turned legs.
    Round,
    /// Low bun feet.
    Bun,
}

impl LegStyle {
    pub const ALL: [LegStyle; 3] = [LegStyle::Square, LegStyle::Round, LegStyle::Bun];

    pub fn name(self) -> &'static str {
        match self {
            LegStyle::Square => "Square",
            LegStyle::Round => "Turned",
            LegStyle::Bun => "Bun feet",
        }
    }
}

/// Fittings inside a cabinet's door opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CabinetAccessory {
    /// Two waste bins on a pull-out.
    TrashPullout,
    /// Two turntable shelves.
    LazySusan,
    /// Shelves on slides, with a rim.
    RollOuts,
    /// Upright dividers for trays and sheet pans.
    TrayDividers,
}

impl CabinetAccessory {
    pub const ALL: [CabinetAccessory; 4] = [CabinetAccessory::TrashPullout, CabinetAccessory::LazySusan, CabinetAccessory::RollOuts, CabinetAccessory::TrayDividers];

    pub fn name(self) -> &'static str {
        match self {
            CabinetAccessory::TrashPullout => "Trash pull-out",
            CabinetAccessory::LazySusan => "Lazy susan",
            CabinetAccessory::RollOuts => "Roll-out shelves",
            CabinetAccessory::TrayDividers => "Tray dividers",
        }
    }
}

/// Rounded or chamfered front corners of a cabinet box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BoxCorner {
    pub round: bool,
    pub size: Length,
    #[serde(default = "yes_corner")]
    pub left: bool,
    #[serde(default = "yes_corner")]
    pub right: bool,
}

fn yes_corner() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cabinet {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub cabinet_kind: CabinetKind,
    /// Centre of the back edge.
    pub position: Point2,
    /// Direction the front faces, degrees counter-clockwise from east.
    pub facing: f64,
    pub width: Length,
    pub depth: Length,
    /// Box height (not counting the countertop).
    pub height: Length,
    /// Height of the box bottom above the floor (0 for base cabinets).
    #[serde(default)]
    pub elevation: Length,
    #[serde(default)]
    pub construction: CabinetConstruction,
    #[serde(default)]
    pub shape: CabinetShape,
    /// Bands of doors and drawers, top to bottom.
    pub face: Vec<FaceRow>,
    #[serde(default)]
    pub door_style: DoorStyleCab,
    #[serde(default)]
    pub hardware: HardwareStyle,
    /// Shelves behind the doors.
    #[serde(default)]
    pub shelves: u32,
    /// Recess at the floor (base and full cabinets): height and depth.
    #[serde(default)]
    pub toe_kick: Option<(Length, Length)>,
    /// The cabinet's own countertop, if any.
    #[serde(default)]
    pub countertop: Option<CabinetTop>,
    /// A sink in this cabinet's countertop.
    #[serde(default)]
    pub sink: bool,
    /// Filler strips added beside the box to close gaps.
    #[serde(default)]
    pub filler_left: Length,
    #[serde(default)]
    pub filler_right: Length,
    /// Leave alone when filling gaps automatically.
    #[serde(default)]
    pub no_auto_filler: bool,
    #[serde(default)]
    pub renovation: Renovation,
    #[serde(default)]
    pub build: CabinetBuild,
    /// Doors swung and drawers pulled open this much, 0–1 (2D and 3D).
    #[serde(default)]
    pub open: f64,
    #[serde(default)]
    pub blind: Option<BlindCorner>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moldings: Vec<CabinetMolding>,
    #[serde(default)]
    pub ends: EndPanels,
    /// Fittings inside the doors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accessories: Vec<CabinetAccessory>,
    /// Rounded or chamfered front corners.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner: Option<BoxCorner>,
}

impl Cabinet {
    /// The NKBA-style label: B24, SB36, DB18, W3030, T2484.
    pub fn code(&self) -> String {
        let w = self.width.inches().round() as i64;
        let h = self.height.inches().round() as i64;
        let all_drawers = self
            .face
            .iter()
            .all(|r| r.items.iter().all(|i| *i == FaceItem::Drawer));
        match self.cabinet_kind {
            CabinetKind::Base if self.sink => format!("SB{w}"),
            CabinetKind::Base if all_drawers => format!("DB{w}"),
            CabinetKind::Base if self.blind.is_some() => format!("BBC{w}"),
            CabinetKind::Base => format!("B{w}"),
            CabinetKind::Wall => format!("W{w}{h}"),
            CabinetKind::Full => format!("T{w}{h}"),
        }
    }
}

/// A countertop drawn as its own shape, across any cabinets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Countertop {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    /// Outline, counter-clockwise.
    pub points: Vec<Point2>,
    /// Height of the top surface above the floor.
    pub height: Length,
    pub thickness: Length,
    #[serde(default)]
    pub edge: EdgeProfile,
    #[serde(default)]
    pub holes: Vec<CounterHole>,
}

/// Appliances and plumbing fixtures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplianceKind {
    Refrigerator,
    Range,
    Cooktop,
    WallOven,
    Dishwasher,
    Microwave,
    RangeHood,
    Washer,
    Dryer,
    Toilet,
    Bathtub,
    Shower,
    VanitySink,
    Pedestal,
    WaterHeater,
    QueenBed,
    EndTable,
    Dresser,
    Sofa,
    ArmChair,
    CoffeeTable,
    DiningTable,
    DiningChair,
    Vase,
    PictureFrame,
}

impl ApplianceKind {
    pub const ALL: [ApplianceKind; 25] = [
        ApplianceKind::Refrigerator,
        ApplianceKind::Range,
        ApplianceKind::Cooktop,
        ApplianceKind::WallOven,
        ApplianceKind::Dishwasher,
        ApplianceKind::Microwave,
        ApplianceKind::RangeHood,
        ApplianceKind::Washer,
        ApplianceKind::Dryer,
        ApplianceKind::Toilet,
        ApplianceKind::Bathtub,
        ApplianceKind::Shower,
        ApplianceKind::VanitySink,
        ApplianceKind::Pedestal,
        ApplianceKind::WaterHeater,
        ApplianceKind::QueenBed,
        ApplianceKind::EndTable,
        ApplianceKind::Dresser,
        ApplianceKind::Sofa,
        ApplianceKind::ArmChair,
        ApplianceKind::CoffeeTable,
        ApplianceKind::DiningTable,
        ApplianceKind::DiningChair,
        ApplianceKind::Vase,
        ApplianceKind::PictureFrame,
    ];

    /// Furniture and accessories (rather than appliances and fixtures).
    pub fn is_furniture(self) -> bool {
        matches!(self, ApplianceKind::QueenBed | ApplianceKind::EndTable | ApplianceKind::Dresser | ApplianceKind::Sofa | ApplianceKind::ArmChair | ApplianceKind::CoffeeTable | ApplianceKind::DiningTable | ApplianceKind::DiningChair | ApplianceKind::Vase | ApplianceKind::PictureFrame)
    }

    pub fn name(self) -> &'static str {
        match self {
            ApplianceKind::Refrigerator => "Refrigerator",
            ApplianceKind::Range => "Range",
            ApplianceKind::Cooktop => "Cooktop",
            ApplianceKind::WallOven => "Wall Oven",
            ApplianceKind::Dishwasher => "Dishwasher",
            ApplianceKind::Microwave => "Microwave",
            ApplianceKind::RangeHood => "Range Hood",
            ApplianceKind::Washer => "Washer",
            ApplianceKind::Dryer => "Dryer",
            ApplianceKind::Toilet => "Toilet",
            ApplianceKind::Bathtub => "Bathtub",
            ApplianceKind::Shower => "Shower Base",
            ApplianceKind::VanitySink => "Vanity Sink",
            ApplianceKind::Pedestal => "Pedestal Sink",
            ApplianceKind::WaterHeater => "Water Heater",
            ApplianceKind::QueenBed => "Queen Bed",
            ApplianceKind::EndTable => "End Table",
            ApplianceKind::Dresser => "Dresser",
            ApplianceKind::Sofa => "Sofa",
            ApplianceKind::ArmChair => "Arm Chair",
            ApplianceKind::CoffeeTable => "Coffee Table",
            ApplianceKind::DiningTable => "Dining Table",
            ApplianceKind::DiningChair => "Dining Chair",
            ApplianceKind::Vase => "Vase",
            ApplianceKind::PictureFrame => "Picture Frame",
        }
    }

    /// Plan label.
    pub fn short(self) -> &'static str {
        match self {
            ApplianceKind::Refrigerator => "REF",
            ApplianceKind::Range => "RANGE",
            ApplianceKind::Cooktop => "CT",
            ApplianceKind::WallOven => "OVEN",
            ApplianceKind::Dishwasher => "DW",
            ApplianceKind::Microwave => "MW",
            ApplianceKind::RangeHood => "HOOD",
            ApplianceKind::Washer => "W",
            ApplianceKind::Dryer => "D",
            ApplianceKind::Toilet => "",
            ApplianceKind::Bathtub => "",
            ApplianceKind::Shower => "",
            ApplianceKind::VanitySink => "",
            ApplianceKind::Pedestal => "",
            ApplianceKind::WaterHeater => "WH",
            _ => "",
        }
    }

    /// Typical width, depth, height and height above the floor (inches).
    pub fn typical(self) -> (f64, f64, f64, f64) {
        match self {
            ApplianceKind::Refrigerator => (36.0, 30.0, 70.0, 0.0),
            ApplianceKind::Range => (30.0, 26.0, 36.0, 0.0),
            ApplianceKind::Cooktop => (30.0, 21.0, 2.0, 36.0),
            ApplianceKind::WallOven => (30.0, 24.0, 52.0, 18.0),
            ApplianceKind::Dishwasher => (24.0, 24.0, 34.5, 0.0),
            ApplianceKind::Microwave => (30.0, 15.0, 17.0, 54.0),
            ApplianceKind::RangeHood => (30.0, 20.0, 6.0, 60.0),
            ApplianceKind::Washer => (27.0, 30.0, 39.0, 0.0),
            ApplianceKind::Dryer => (27.0, 30.0, 39.0, 0.0),
            ApplianceKind::Toilet => (19.0, 28.0, 30.0, 0.0),
            ApplianceKind::Bathtub => (60.0, 30.0, 20.0, 0.0),
            ApplianceKind::Shower => (36.0, 36.0, 4.0, 0.0),
            ApplianceKind::VanitySink => (20.0, 16.0, 6.0, 30.0),
            ApplianceKind::Pedestal => (22.0, 18.0, 34.0, 0.0),
            ApplianceKind::WaterHeater => (22.0, 22.0, 60.0, 0.0),
            ApplianceKind::QueenBed => (60.0, 80.0, 24.0, 0.0),
            ApplianceKind::EndTable => (20.0, 18.0, 26.0, 0.0),
            ApplianceKind::Dresser => (60.0, 20.0, 34.0, 0.0),
            ApplianceKind::Sofa => (84.0, 36.0, 34.0, 0.0),
            ApplianceKind::ArmChair => (34.0, 34.0, 34.0, 0.0),
            ApplianceKind::CoffeeTable => (48.0, 24.0, 18.0, 0.0),
            ApplianceKind::DiningTable => (72.0, 40.0, 30.0, 0.0),
            ApplianceKind::DiningChair => (18.0, 20.0, 38.0, 0.0),
            ApplianceKind::Vase => (6.0, 6.0, 18.0, 34.0),
            ApplianceKind::PictureFrame => (24.0, 1.5, 30.0, 50.0),
        }
    }

    /// Stands against a wall with its back to it.
    pub fn against_wall(self) -> bool {
        !matches!(self, ApplianceKind::Cooktop | ApplianceKind::CoffeeTable | ApplianceKind::DiningTable | ApplianceKind::DiningChair | ApplianceKind::Vase )
    }
}

/// An appliance or plumbing fixture, placed like a cabinet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Appliance {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub appliance_kind: ApplianceKind,
    /// Centre of the back edge.
    pub position: Point2,
    /// Direction the front faces, degrees.
    pub facing: f64,
    pub width: Length,
    pub depth: Length,
    pub height: Length,
    #[serde(default)]
    pub elevation: Length,
    #[serde(default)]
    pub renovation: Renovation,
}

/// Cabinet sizes and options used when placing new cabinets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CabinetDefaults {
    pub base_width: Length,
    pub base_depth: Length,
    pub base_height: Length,
    pub wall_width: Length,
    pub wall_depth: Length,
    pub wall_height: Length,
    /// Bottom of wall cabinets above the floor.
    pub wall_elevation: Length,
    pub full_depth: Length,
    pub full_height: Length,
    pub toe_kick_height: Length,
    pub toe_kick_depth: Length,
    pub construction: CabinetConstruction,
    pub door_style: DoorStyleCab,
    pub hardware: HardwareStyle,
    pub countertop_thickness: Length,
    pub countertop_overhang: Length,
    pub edge: EdgeProfile,
    pub backsplash: Length,
    /// Gaps up to this wide are closed by automatic fillers.
    pub max_filler: Length,
    /// Label cabinets in plan with their NKBA codes (B24, W3030…).
    #[serde(default)]
    pub show_codes: bool,
}

impl Default for CabinetDefaults {
    fn default() -> Self {
        let i = Length::from_inches;
        CabinetDefaults {
            base_width: i(24.0),
            base_depth: i(24.0),
            base_height: i(34.5),
            wall_width: i(24.0),
            wall_depth: i(12.0),
            wall_height: i(30.0),
            wall_elevation: i(54.0),
            full_depth: i(24.0),
            full_height: i(84.0),
            toe_kick_height: i(4.0),
            toe_kick_depth: i(3.0),
            construction: CabinetConstruction::Framed,
            door_style: DoorStyleCab::Shaker,
            hardware: HardwareStyle::Pull,
            countertop_thickness: i(1.5),
            countertop_overhang: i(1.0),
            edge: EdgeProfile::Eased,
            backsplash: i(4.0),
            max_filler: i(6.0),
            show_codes: false,
        }
    }
}

impl CabinetDefaults {
    /// A new cabinet's front: base cabinets get a drawer over doors, wall
    /// cabinets doors, full-height cabinets two rows of doors. Cabinets 24"
    /// or wider get pairs of doors; a sink base gets a false front.
    pub fn face(&self, kind: CabinetKind, width: Length, sink: bool) -> Vec<FaceRow> {
        let door = if width.inches() >= 24.0 {
            FaceItem::DoubleDoor
        } else {
            FaceItem::DoorRight
        };
        let row = |height: Option<Length>, items: Vec<FaceItem>| FaceRow {
            height,
            items,
            shelves: None,
        };
        match kind {
            CabinetKind::Base => vec![
                row(
                    Some(Length::from_inches(6.0)),
                    vec![if sink { FaceItem::Panel } else { FaceItem::Drawer }],
                ),
                row(None, vec![door]),
            ],
            CabinetKind::Wall => vec![row(None, vec![door])],
            CabinetKind::Full => vec![
                row(Some(Length::from_inches(36.0)), vec![door]),
                row(None, vec![door]),
            ],
        }
    }
}

/// A backsplash drawn along any path (wrapping corners and ends), standing
/// on the counter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Backsplash {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    /// Along the wall face; the backsplash stands to the right of the path's
    /// direction.
    pub points: Vec<Point2>,
    /// Height of its bottom above the floor.
    pub bottom: Length,
    pub height: Length,
    pub thickness: Length,
}
