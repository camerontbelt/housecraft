//! The tool catalogue: parent families on the main toolbar, child tools in the
//! Tool Palette. Names and hotkeys follow the conventions residential CAD users
//! already know, so muscle memory carries over.

use housecraft_api::model::{
    ApplianceKind, CabinetKind, DoorStyle, ElectricalKind, Id, PlantForm, RoadStyle, SectionKind, SiteKind,
    WindowStyle,
};

use crate::icons::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// A dormer on the roof.
    Dormer(housecraft_api::model::DormerStyle),
    /// A skylight in a roof plane.
    Skylight,
    /// A bay, box or bow window built out from a wall.
    BayWindow(housecraft_api::BayStyle),
    /// Two windows meeting at a wall corner.
    CornerWindow,
    Select,
    ExteriorWall,
    InteriorWall,
    Door(DoorStyle),
    Window(WindowStyle),
    Dimension,
    AngularDimension,
    Text,
    TextArrow,
    Line,
    Arrow,
    Polyline,
    Box,
    Circle,
    Arc,
    ArcThreePoint,
    RegularPolygon,
    Ellipse,
    Spline,
    Point,
    RoofPlane,
    CrossBox,
    BlockingBox,
    Insulation,
    Callout,
    Marker,
    RevisionCloud,
    /// Draw an elevation or cross-section camera: click where you stand, then the direction to look.
    SectionLine(SectionKind),
    Stair(StairKind),
    Electrical(ElectricalKind),
    /// Wire switches to the fixtures they control.
    ConnectSwitch,
    /// Place copies of a CAD block.
    PlaceBlock(Id),
    Cabinet(CabinetKind),
    /// A base cabinet with a sink.
    SinkCabinet,
    /// Draw a custom countertop outline.
    Countertop,
    Appliance(ApplianceKind),
    /// Place the cabinet or appliance chosen in the Library Browser.
    PlaceTemplate,
    /// Place copies of an imported 3D symbol.
    PlaceSymbol(Id),
    /// Draw a backsplash along the wall.
    Backsplash,
    /// Place a group of objects from the Library Browser.
    PlaceGroup,
    /// Draw a molding along a path.
    MoldingPolyline,
    /// Draw a floor material region (inlay).
    FloorRegion,
    /// Mark a material region on a wall face.
    WallRegion,
    /// Copy chosen properties from one object to others.
    ObjectPainter,
    /// Draw a 3D solid (box, cylinder, cone or sphere) in a rectangle.
    Solid,
    /// Draw the terrain's outline.
    TerrainPerimeter,
    /// Place a spot elevation.
    ElevationPoint,
    /// Draw a line of one elevation.
    ElevationLine,
    /// Draw a flat region.
    FlatRegion,
    /// Raise a rounded hill.
    Hill,
    /// Dig a rounded valley.
    Valley,
    RetainingWall,
    Road(RoadStyle),
    Fence,
    GardenBed,
    SteppingStones,
    Stream,
    Plant(PlantForm),
    /// Click a room, then the direction its joists run.
    JoistDirection,
    /// A post from the floor to the ceiling.
    Post,
    /// A beam at the ceiling between two clicks.
    Beam,
    Deck,
    RaisedPlatform,
    RopeLight,
    Grass,
}

impl Tool {
    /// The terrain tools, in menu order.
    pub const TERRAIN: [Tool; 6] = [
        Tool::TerrainPerimeter,
        Tool::ElevationPoint,
        Tool::ElevationLine,
        Tool::FlatRegion,
        Tool::Hill,
        Tool::Valley,
    ];

    /// The site tools, in menu order.
    pub const SITE: [Tool; 16] = [
        Tool::Deck,
        Tool::RaisedPlatform,
        Tool::RetainingWall,
        Tool::Road(RoadStyle::Road),
        Tool::Road(RoadStyle::Driveway),
        Tool::Road(RoadStyle::Sidewalk),
        Tool::Fence,
        Tool::GardenBed,
        Tool::SteppingStones,
        Tool::Stream,
        Tool::RopeLight,
        Tool::Grass,
        Tool::Plant(PlantForm::Tree),
        Tool::Plant(PlantForm::Conifer),
        Tool::Plant(PlantForm::Shrub),
        Tool::Plant(PlantForm::Grass),
    ];

    /// The site feature a multi-click site tool draws.
    pub fn site_kind(self) -> Option<SiteKind> {
        Some(match self {
            Tool::RetainingWall => SiteKind::retaining_wall(),
            Tool::Road(s) => SiteKind::road(s),
            Tool::Fence => SiteKind::fence(),
            Tool::GardenBed => SiteKind::garden_bed(),
            Tool::SteppingStones => SiteKind::stepping_stones(),
            Tool::Stream => SiteKind::stream(),
            Tool::RopeLight => SiteKind::rope_light(),
            Tool::Grass => SiteKind::grass(),
            _ => return None,
        })
    }
}

/// The shapes offered by the stair tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StairKind {
    Straight,
    L,
    U,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Select,
    Walls,
    Doors,
    Windows,
    Cabinets,
    Stairs,
    Roofs,
    Electrical,
    Dimensions,
    Text,
    Cad,
    Cameras,
}

/// Actions in the Cameras family (they open 3D views rather than draw).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraAction {
    FullCamera,
    PerspectiveOverview,
    OrthographicOverview,
}

impl CameraAction {
    pub const ALL: &[CameraAction] = &[
        CameraAction::FullCamera,
        CameraAction::PerspectiveOverview,
        CameraAction::OrthographicOverview,
    ];
    pub fn name(self) -> &'static str {
        match self {
            CameraAction::FullCamera => "Full Camera",
            CameraAction::PerspectiveOverview => "Perspective Full Overview",
            CameraAction::OrthographicOverview => "Orthographic Full Overview",
        }
    }
    pub fn hotkey(self) -> &'static str {
        match self {
            CameraAction::FullCamera => "Shift+J",
            CameraAction::PerspectiveOverview => "Shift+K",
            CameraAction::OrthographicOverview => "H",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            CameraAction::FullCamera => Icon::Camera,
            CameraAction::PerspectiveOverview => Icon::Overview,
            CameraAction::OrthographicOverview => Icon::OrthoOverview,
        }
    }
}

impl Family {
    /// Main build toolbar order.
    pub const TOOLBAR: &[Family] = &[
        Family::Select,
        Family::Walls,
        Family::Cabinets,
        Family::Doors,
        Family::Windows,
        Family::Stairs,
        Family::Roofs,
        Family::Electrical,
        Family::Dimensions,
        Family::Text,
        Family::Cad,
        Family::Cameras,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Family::Select => "Select Objects",
            Family::Walls => "Wall Tools",
            Family::Doors => "Door Tools",
            Family::Windows => "Window Tools",
            Family::Cabinets => "Cabinet Tools",
            Family::Stairs => "Stair Tools",
            Family::Roofs => "Roof Tools",
            Family::Electrical => "Electrical, Plumbing and HVAC",
            Family::Dimensions => "Dimension Tools",
            Family::Text => "Text Tools",
            Family::Cad => "CAD Tools",
            Family::Cameras => "Camera Tools",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Family::Select => Icon::Select,
            Family::Walls => Icon::ExteriorWall,
            Family::Doors => Icon::Door,
            Family::Windows => Icon::Window,
            Family::Cabinets => Icon::Cabinet,
            Family::Stairs => Icon::Stairs,
            Family::Roofs => Icon::Roof,
            Family::Electrical => Icon::Electrical,
            Family::Dimensions => Icon::Dimension,
            Family::Text => Icon::Text,
            Family::Cad => Icon::Line,
            Family::Cameras => Icon::Camera,
        }
    }

    /// Families whose tools are not built yet are shown disabled.
    pub fn available(self) -> bool {
        true
    }

    pub fn tools(self) -> &'static [Tool] {
        use DoorStyle as D;
        use WindowStyle as W;
        match self {
            Family::Select => &[Tool::Select],
            Family::Walls => &[Tool::ExteriorWall, Tool::InteriorWall],
            Family::Doors => &[
                Tool::Door(D::Hinged),
                Tool::Door(D::Double),
                Tool::Door(D::Atrium),
                Tool::Door(D::Sliding),
                Tool::Door(D::Pocket),
                Tool::Door(D::Bifold),
                Tool::Door(D::Barn),
                Tool::Door(D::Garage),
                Tool::Door(D::Shower),
                Tool::Door(D::Fixed),
                Tool::Door(D::Doorway),
            ],
            Family::Windows => &[
                Tool::Window(W::DoubleHung),
                Tool::Window(W::SingleHung),
                Tool::Window(W::Casement),
                Tool::Window(W::Sliding),
                Tool::Window(W::Awning),
                Tool::Window(W::Hopper),
                Tool::Window(W::Fixed),
                Tool::CornerWindow,
                Tool::BayWindow(housecraft_api::BayStyle::Bay),
                Tool::BayWindow(housecraft_api::BayStyle::Box),
                Tool::BayWindow(housecraft_api::BayStyle::Bow),
            ],
            Family::Dimensions => &[Tool::Dimension, Tool::AngularDimension],
            Family::Text => &[Tool::Text, Tool::TextArrow, Tool::Callout, Tool::Marker, Tool::RevisionCloud],
            Family::Cad => &[
                Tool::Line,
                Tool::Arrow,
                Tool::Polyline,
                Tool::Box,
                Tool::Circle,
                Tool::Arc,
                Tool::ArcThreePoint,
                Tool::RegularPolygon,
                Tool::Ellipse,
                Tool::Spline,
                Tool::Point,
                Tool::MoldingPolyline,
                Tool::FloorRegion,
                Tool::WallRegion,
                Tool::ObjectPainter,
                Tool::Solid,
                Tool::CrossBox,
                Tool::BlockingBox,
                Tool::Insulation,
            ],
            Family::Roofs => &[
                Tool::RoofPlane,
                Tool::Dormer(housecraft_api::model::DormerStyle::Gable),
                Tool::Dormer(housecraft_api::model::DormerStyle::Shed),
                Tool::Skylight,
            ],
            Family::Stairs => &[
                Tool::Stair(StairKind::Straight),
                Tool::Stair(StairKind::L),
                Tool::Stair(StairKind::U),
            ],
            Family::Electrical => &[
                Tool::Electrical(ElectricalKind::Outlet),
                Tool::Electrical(ElectricalKind::GfciOutlet),
                Tool::Electrical(ElectricalKind::Outlet240),
                Tool::Electrical(ElectricalKind::Switch),
                Tool::Electrical(ElectricalKind::Dimmer),
                Tool::Electrical(ElectricalKind::CeilingLight),
                Tool::Electrical(ElectricalKind::RecessedLight),
                Tool::Electrical(ElectricalKind::WallLight),
                Tool::Electrical(ElectricalKind::CeilingFan),
                Tool::Electrical(ElectricalKind::SmokeDetector),
                Tool::Electrical(ElectricalKind::JunctionBox),
                Tool::Electrical(ElectricalKind::HoseBibb),
                Tool::Electrical(ElectricalKind::FloorDrain),
                Tool::Electrical(ElectricalKind::GasOutlet),
                Tool::Electrical(ElectricalKind::SupplyRegister),
                Tool::Electrical(ElectricalKind::ReturnGrille),
                Tool::Electrical(ElectricalKind::Thermostat),
                Tool::Electrical(ElectricalKind::ExhaustFan),
                Tool::ConnectSwitch,
            ],
            Family::Cabinets => &[
                Tool::Cabinet(CabinetKind::Base),
                Tool::Cabinet(CabinetKind::Wall),
                Tool::Cabinet(CabinetKind::Full),
                Tool::SinkCabinet,
                Tool::Countertop,
                Tool::Backsplash,
                Tool::Appliance(ApplianceKind::Refrigerator),
                Tool::Appliance(ApplianceKind::Range),
                Tool::Appliance(ApplianceKind::Cooktop),
                Tool::Appliance(ApplianceKind::WallOven),
                Tool::Appliance(ApplianceKind::Dishwasher),
                Tool::Appliance(ApplianceKind::Microwave),
                Tool::Appliance(ApplianceKind::RangeHood),
                Tool::Appliance(ApplianceKind::Washer),
                Tool::Appliance(ApplianceKind::Dryer),
                Tool::Appliance(ApplianceKind::Toilet),
                Tool::Appliance(ApplianceKind::Bathtub),
                Tool::Appliance(ApplianceKind::Shower),
                Tool::Appliance(ApplianceKind::VanitySink),
                Tool::Appliance(ApplianceKind::Pedestal),
                Tool::Appliance(ApplianceKind::WaterHeater),
            ],
            Family::Cameras => &[],
        }
    }

    /// The tool a parent button activates.
    pub fn default_tool(self) -> Option<Tool> {
        self.tools().first().copied()
    }
}

impl Tool {
    /// Tools that draft (lines, dimensions, text): the ones that work in
    /// elevation, section and CAD detail views as well as on plans.
    pub fn is_drafting(self) -> bool {
        matches!(
            self.family(),
            Family::Select | Family::Dimensions | Family::Text | Family::Cad
        )
    }

    pub fn family(self) -> Family {
        match self {
            Tool::Select => Family::Select,
            Tool::ExteriorWall | Tool::InteriorWall => Family::Walls,
            Tool::Door(_) => Family::Doors,
            Tool::Window(_) | Tool::BayWindow(_) | Tool::CornerWindow => Family::Windows,
            Tool::Dimension | Tool::AngularDimension => Family::Dimensions,
            Tool::Text | Tool::TextArrow | Tool::Callout | Tool::Marker | Tool::RevisionCloud => Family::Text,
            Tool::RoofPlane | Tool::Dormer(_) | Tool::Skylight => Family::Roofs,
            Tool::SectionLine(_) => Family::Cameras,
            Tool::Stair(_) => Family::Stairs,
            Tool::Electrical(_) | Tool::ConnectSwitch => Family::Electrical,
            Tool::Cabinet(_)
            | Tool::SinkCabinet
            | Tool::Countertop
            | Tool::Appliance(_)
            | Tool::PlaceTemplate
            | Tool::PlaceSymbol(_)
            | Tool::Backsplash
            | Tool::PlaceGroup => Family::Cabinets,
            _ => Family::Cad,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "Select Objects",
            Tool::ExteriorWall => "Straight Exterior Wall",
            Tool::InteriorWall => "Straight Interior Wall",
            Tool::Door(d) => d.name(),
            Tool::Window(w) => w.name(),
            Tool::BayWindow(housecraft_api::BayStyle::Bay) => "Bay Window",
            Tool::BayWindow(housecraft_api::BayStyle::Box) => "Box Window",
            Tool::BayWindow(housecraft_api::BayStyle::Bow) => "Bow Window",
            Tool::CornerWindow => "Corner Window",
            Tool::Dimension => "Manual Dimension",
            Tool::AngularDimension => "Angular Dimension",
            Tool::Text => "Text",
            Tool::TextArrow => "Text with Arrow",
            Tool::Line => "Draw Line",
            Tool::Arrow => "Line with Arrow",
            Tool::Polyline => "Polyline",
            Tool::Box => "Rectangular Polyline",
            Tool::Circle => "Circle",
            Tool::Arc => "Arc",
            Tool::ArcThreePoint => "Arc Through Three Points",
            Tool::RegularPolygon => "Regular Polygon",
            Tool::Ellipse => "Ellipse",
            Tool::Spline => "Spline",
            Tool::Point => "Point",
            Tool::CrossBox => "Cross Box",
            Tool::BlockingBox => "Blocking Box",
            Tool::Insulation => "Batt Insulation",
            Tool::Callout => "Callout",
            Tool::Marker => "Marker",
            Tool::RevisionCloud => "Revision Cloud",
            Tool::RoofPlane => "Roof Plane",
            Tool::Dormer(housecraft_api::model::DormerStyle::Gable) => "Gable Dormer",
            Tool::Dormer(housecraft_api::model::DormerStyle::Shed) => "Shed Dormer",
            Tool::Skylight => "Skylight",
            Tool::SectionLine(SectionKind::Section) => "Cross Section / Elevation",
            Tool::SectionLine(SectionKind::Elevation) => "Elevation",
            Tool::Stair(StairKind::Straight) => "Straight Stair",
            Tool::Stair(StairKind::L) => "L-Shaped Stair",
            Tool::Stair(StairKind::U) => "U-Shaped Stair",
            Tool::Electrical(k) => k.name(),
            Tool::ConnectSwitch => "Connect Electrical",
            Tool::PlaceBlock(_) => "Place Block",
            Tool::Cabinet(k) => k.name(),
            Tool::SinkCabinet => "Sink Base Cabinet",
            Tool::Countertop => "Custom Countertop",
            Tool::Appliance(k) => k.name(),
            Tool::PlaceTemplate => "Place from Library",
            Tool::PlaceSymbol(_) => "Place 3D Symbol",
            Tool::Backsplash => "Custom Backsplash",
            Tool::MoldingPolyline => "Molding Polyline",
            Tool::FloorRegion => "Floor Material Region",
            Tool::WallRegion => "Wall Material Region",
            Tool::ObjectPainter => "Object Painter",
            Tool::Solid => "3D Solid",
            Tool::TerrainPerimeter => "Terrain Perimeter",
            Tool::ElevationPoint => "Elevation Point",
            Tool::ElevationLine => "Elevation Line",
            Tool::FlatRegion => "Flat Region",
            Tool::Hill => "Hill",
            Tool::Valley => "Valley",
            Tool::RetainingWall => "Retaining Wall",
            Tool::Road(RoadStyle::Road) => "Road",
            Tool::Road(RoadStyle::Driveway) => "Driveway",
            Tool::Road(RoadStyle::Sidewalk) => "Sidewalk",
            Tool::Fence => "Fence",
            Tool::GardenBed => "Garden Bed",
            Tool::SteppingStones => "Stepping Stones",
            Tool::Stream => "Stream",
            Tool::Plant(PlantForm::Tree) => "Tree",
            Tool::Plant(PlantForm::Conifer) => "Conifer",
            Tool::Plant(PlantForm::Shrub) => "Shrub",
            Tool::Plant(PlantForm::Grass) => "Grass / Perennial",
            Tool::JoistDirection => "Joist Direction",
            Tool::Post => "Post",
            Tool::Beam => "Beam",
            Tool::Deck => "Deck",
            Tool::RopeLight => "Rope Light",
            Tool::Grass => "Grass",
            Tool::RaisedPlatform => "Raised Platform",
            Tool::PlaceGroup => "Place Group",
        }
    }

    pub fn icon(self) -> Icon {
        use DoorStyle as D;
        match self {
            Tool::Select => Icon::Select,
            Tool::ExteriorWall => Icon::ExteriorWall,
            Tool::InteriorWall => Icon::InteriorWall,
            Tool::Door(D::Hinged) => Icon::Door,
            Tool::Door(D::Double) => Icon::DoubleDoor,
            Tool::Door(D::Sliding) => Icon::SlidingDoor,
            Tool::Door(D::Pocket) => Icon::PocketDoor,
            Tool::Door(D::Bifold) => Icon::BifoldDoor,
            Tool::Door(D::Doorway) => Icon::Doorway,
            Tool::Door(D::Garage | D::Barn) => Icon::SlidingDoor,
            Tool::Door(D::Atrium) => Icon::DoubleDoor,
            Tool::Door(D::Shower | D::Fixed) => Icon::Door,
            Tool::Window(_) | Tool::BayWindow(_) | Tool::CornerWindow => Icon::Window,
            Tool::Dimension => Icon::Dimension,
            Tool::AngularDimension => Icon::AngularDimension,
            Tool::Text => Icon::Text,
            Tool::TextArrow => Icon::Arrow,
            Tool::Line => Icon::Line,
            Tool::Arrow => Icon::Arrow,
            Tool::Polyline => Icon::Polyline,
            Tool::Box => Icon::Box,
            Tool::Circle => Icon::Circle,
            Tool::Arc => Icon::Arc,
            Tool::ArcThreePoint => Icon::Arc,
            Tool::RegularPolygon => Icon::Polygon,
            Tool::Ellipse => Icon::Ellipse,
            Tool::Spline => Icon::Spline,
            Tool::Point => Icon::Point,
            Tool::MoldingPolyline | Tool::FloorRegion => Icon::Polyline,
            Tool::WallRegion | Tool::Solid => Icon::Box,
            Tool::TerrainPerimeter | Tool::ElevationLine | Tool::FlatRegion => Icon::Polyline,
            Tool::ElevationPoint => Icon::Point,
            Tool::Hill | Tool::Valley | Tool::Plant(_) | Tool::SteppingStones => Icon::Circle,
            Tool::JoistDirection => Icon::Arrow,
            Tool::Post => Icon::Box,
            Tool::Beam => Icon::Line,
            Tool::Deck | Tool::RaisedPlatform | Tool::RopeLight | Tool::Grass => Icon::Polyline,
            Tool::RetainingWall | Tool::Road(_) | Tool::Fence | Tool::GardenBed | Tool::Stream => Icon::Polyline,
            Tool::ObjectPainter => Icon::Select,
            Tool::CrossBox | Tool::BlockingBox | Tool::Insulation => Icon::Box,
            Tool::Callout | Tool::Marker | Tool::RevisionCloud => Icon::Text,
            Tool::RoofPlane | Tool::Dormer(_) => Icon::Roof,
            Tool::Skylight => Icon::Window,
            Tool::SectionLine(_) => Icon::Camera,
            Tool::Stair(_) => Icon::Stairs,
            Tool::Electrical(_) | Tool::ConnectSwitch => Icon::Electrical,
            Tool::PlaceBlock(_) => Icon::Box,
            Tool::Cabinet(_)
            | Tool::SinkCabinet
            | Tool::Countertop
            | Tool::Appliance(_)
            | Tool::PlaceTemplate
            | Tool::PlaceSymbol(_)
            | Tool::Backsplash
            | Tool::PlaceGroup => Icon::Cabinet,
        }
    }

    pub fn hotkey(self) -> Option<&'static str> {
        Some(match self {
            Tool::Select => "Space",
            Tool::ExteriorWall => "Shift+Q",
            Tool::Door(DoorStyle::Hinged) => "Shift+E",
            Tool::Window(WindowStyle::DoubleHung) => "Shift+W",
            Tool::Text => "Y",
            Tool::Line => "W",
            Tool::Arc => "E",
            Tool::Circle => "K",
            Tool::Box => "Shift+P",
            _ => return None,
        })
    }

    pub fn tooltip(self) -> String {
        match self.hotkey() {
            Some(k) => format!("{} ({k})", self.name()),
            None => self.name().to_string(),
        }
    }

    /// Status-bar instructions.
    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => {
                "Click to select, Ctrl+click to add, drag to move or box-select. Double-click to open."
            }
            Tool::ExteriorWall | Tool::InteriorWall => {
                "Click to start, click for each corner; type a length and Enter for exact walls. Esc or double-click ends."
            }
            Tool::Door(_) | Tool::Window(_) => "Click on a wall to place.",
            Tool::BayWindow(_) => "Click on an exterior wall where the bay goes; edit its walls and windows afterwards.",
            Tool::CornerWindow => "Click near the end of a wall at a corner.",
            Tool::Dimension => "Click two points, then click where the dimension line goes.",
            Tool::AngularDimension => "Click the vertex, a point on each side, then the radius.",
            Tool::Text => "Click where the text goes. Markup: **bold**, __underline__, [label](link), {#ff0000}colour{/}.",
            Tool::TextArrow => "Click where the arrow points, then where the text goes.",
            Tool::Line | Tool::Arrow => "Click the start and end points.",
            Tool::Polyline | Tool::Spline => {
                "Click points; double-click, Enter or click the first point to finish."
            }
            Tool::Box => "Click two opposite corners.",
            Tool::Circle => "Click the centre, then a point on the circle.",
            Tool::Arc => {
                "Click the centre, the start point, then the end point (counter-clockwise)."
            }
            Tool::RegularPolygon => {
                "Click the centre, then a corner. Set the number of sides in the Tool Palette."
            }
            Tool::ArcThreePoint => "Click the start, a point the arc passes through, then the end.",
            Tool::Ellipse => "Click two opposite corners of the bounding box.",
            Tool::Point => "Click to place a point.",
            Tool::CrossBox | Tool::BlockingBox | Tool::Insulation => {
                "Click the two ends along the member; set its depth in its specification."
            }
            Tool::Callout => "Click what the arrow points at, then where the note goes.",
            Tool::Marker => "Click to place a numbered marker.",
            Tool::RevisionCloud => {
                "Click around the changed area; double-click or Enter to close the cloud."
            }
            Tool::Stair(_) => {
                "Click the bottom of the stair, then click in the direction it climbs."
            }
            Tool::Electrical(k) if k.on_wall() => "Click near a wall to place it on the wall face.",
            Tool::Electrical(ElectricalKind::FloorDrain) => "Click to place it in the floor.",
            Tool::Electrical(_) => "Click to place it on the ceiling.",
            Tool::ConnectSwitch => {
                "Click a switch, then click fixtures to wire or unwire them. Shared fixtures make 3-way and 4-way switches."
            }
            Tool::SectionLine(_) => {
                "Click where the view stands, then click in the direction to look."
            }
            Tool::Dormer(_) => "Click on a roof plane where the dormer's front goes; it faces down the roof.",
            Tool::Skylight => "Click on a roof plane to place a skylight.",
            Tool::RoofPlane => {
                "Click the two ends of the eave, then the other corners; double-click or Enter to finish."
            }
            Tool::PlaceBlock(_) => "Click to place a copy of the block; Esc when done.",
            Tool::Cabinet(_) | Tool::SinkCabinet => {
                "Click near a wall to back the cabinet onto it, or anywhere for an island."
            }
            Tool::Countertop => {
                "Click the corners of the countertop; double-click or Enter to finish."
            }
            Tool::Appliance(_) | Tool::PlaceTemplate | Tool::PlaceSymbol(_) => {
                "Click near a wall to back it onto the wall."
            }
            Tool::Backsplash => {
                "Click along the wall with the room on your right; double-click or Enter to finish."
            }
            Tool::PlaceGroup => "Click to place the group's bottom-left corner.",
            Tool::MoldingPolyline => {
                "Click along the path with the molding on your right; double-click or Enter to finish."
            }
            Tool::FloorRegion => "Click the corners of the region; double-click or Enter to finish.",
            Tool::Solid => {
                "Click two opposite corners; the shape and height come from Default Settings > 3D Solids."
            }
            Tool::TerrainPerimeter => "Click the corners of the lot; double-click or Enter to finish.",
            Tool::ElevationPoint => "Click to place a spot elevation (set its height in its specification).",
            Tool::ElevationLine => "Click along the line; double-click or Enter to finish.",
            Tool::FlatRegion => "Click the corners of the flat area; double-click or Enter to finish.",
            Tool::Hill | Tool::Valley => "Click the centre, then a point on its edge.",
            Tool::RetainingWall
            | Tool::Road(_)
            | Tool::Fence
            | Tool::SteppingStones
            | Tool::Stream => "Click along its path; double-click or Enter to finish.",
            Tool::GardenBed => "Click the corners of the bed; double-click or Enter to finish.",
            Tool::Plant(_) => "Click to plant it (set its name and size in its specification).",
            Tool::JoistDirection => "Click in a room, then click the direction its floor joists run.",
            Tool::Post => "Click to place a 6x6 post from the floor to the ceiling.",
            Tool::Beam => "Click the two ends of a 2-ply beam under the ceiling.",
            Tool::Deck => "Click the deck's corners (against the house for a ledger); double-click or Enter to finish.",
            Tool::RaisedPlatform => "Click the platform's corners; double-click or Enter to finish.",
            Tool::RopeLight => "Click along the rope light's path; double-click or Enter to finish.",
            Tool::Grass => "Click the corners of the lawn; double-click or Enter to finish.",
            Tool::WallRegion => "Click the two ends of the region on one side of a wall.",
            Tool::ObjectPainter => {
                "Alt+click an object to copy from, tick what to copy, then click objects to paint them."
            }
        }
    }
}
