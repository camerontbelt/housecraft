//! The tool catalogue: parent families on the main toolbar, child tools in the
//! Tool Palette. Names and hotkeys follow the conventions residential CAD users
//! already know, so muscle memory carries over.

use housecraft_api::model::{DoorStyle, WindowStyle};

use crate::icons::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    ExteriorWall,
    InteriorWall,
    Door(DoorStyle),
    Window(WindowStyle),
    Dimension,
    AngularDimension,
    Text,
    Line,
    Arrow,
    Polyline,
    Box,
    Circle,
    Arc,
    Ellipse,
    Spline,
    Point,
    RoofPlane,
    Stair(StairKind),
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
            Family::Electrical => "Electrical Tools",
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
        !matches!(self, Family::Cabinets | Family::Electrical)
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
                Tool::Door(D::Sliding),
                Tool::Door(D::Pocket),
                Tool::Door(D::Bifold),
                Tool::Door(D::Barn),
                Tool::Door(D::Garage),
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
            ],
            Family::Dimensions => &[Tool::Dimension, Tool::AngularDimension],
            Family::Text => &[Tool::Text],
            Family::Cad => &[
                Tool::Line,
                Tool::Arrow,
                Tool::Polyline,
                Tool::Box,
                Tool::Circle,
                Tool::Arc,
                Tool::Ellipse,
                Tool::Spline,
                Tool::Point,
            ],
            Family::Roofs => &[Tool::RoofPlane],
            Family::Stairs => &[
                Tool::Stair(StairKind::Straight),
                Tool::Stair(StairKind::L),
                Tool::Stair(StairKind::U),
            ],
            Family::Cabinets | Family::Electrical | Family::Cameras => &[],
        }
    }

    /// The tool a parent button activates.
    pub fn default_tool(self) -> Option<Tool> {
        self.tools().first().copied()
    }
}

impl Tool {
    pub fn family(self) -> Family {
        match self {
            Tool::Select => Family::Select,
            Tool::ExteriorWall | Tool::InteriorWall => Family::Walls,
            Tool::Door(_) => Family::Doors,
            Tool::Window(_) => Family::Windows,
            Tool::Dimension | Tool::AngularDimension => Family::Dimensions,
            Tool::Text => Family::Text,
            Tool::RoofPlane => Family::Roofs,
            Tool::Stair(_) => Family::Stairs,
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
            Tool::Dimension => "Manual Dimension",
            Tool::AngularDimension => "Angular Dimension",
            Tool::Text => "Text",
            Tool::Line => "Draw Line",
            Tool::Arrow => "Line with Arrow",
            Tool::Polyline => "Polyline",
            Tool::Box => "Rectangular Polyline",
            Tool::Circle => "Circle",
            Tool::Arc => "Arc",
            Tool::Ellipse => "Ellipse",
            Tool::Spline => "Spline",
            Tool::Point => "Point",
            Tool::RoofPlane => "Roof Plane",
            Tool::Stair(StairKind::Straight) => "Straight Stair",
            Tool::Stair(StairKind::L) => "L-Shaped Stair",
            Tool::Stair(StairKind::U) => "U-Shaped Stair",
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
            Tool::Window(_) => Icon::Window,
            Tool::Dimension => Icon::Dimension,
            Tool::AngularDimension => Icon::AngularDimension,
            Tool::Text => Icon::Text,
            Tool::Line => Icon::Line,
            Tool::Arrow => Icon::Arrow,
            Tool::Polyline => Icon::Polyline,
            Tool::Box => Icon::Box,
            Tool::Circle => Icon::Circle,
            Tool::Arc => Icon::Arc,
            Tool::Ellipse => Icon::Ellipse,
            Tool::Spline => Icon::Spline,
            Tool::Point => Icon::Point,
            Tool::RoofPlane => Icon::Roof,
            Tool::Stair(_) => Icon::Stairs,
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
            Tool::Dimension => "Click two points, then click where the dimension line goes.",
            Tool::AngularDimension => "Click the vertex, a point on each side, then the radius.",
            Tool::Text => "Click where the text goes.",
            Tool::Line | Tool::Arrow => "Click the start and end points.",
            Tool::Polyline | Tool::Spline => {
                "Click points; double-click, Enter or click the first point to finish."
            }
            Tool::Box => "Click two opposite corners.",
            Tool::Circle => "Click the centre, then a point on the circle.",
            Tool::Arc => {
                "Click the centre, the start point, then the end point (counter-clockwise)."
            }
            Tool::Ellipse => "Click two opposite corners of the bounding box.",
            Tool::Point => "Click to place a point.",
            Tool::Stair(_) => {
                "Click the bottom of the stair, then click in the direction it climbs."
            }
            Tool::RoofPlane => {
                "Click the two ends of the eave, then the other corners; double-click or Enter to finish."
            }
        }
    }
}
