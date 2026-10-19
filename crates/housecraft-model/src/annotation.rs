//! 2D drafting objects: CAD shapes, dimensions, text.

use housecraft_units::{Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Color, Id, Justify};

/// A 2D CAD shape. Angles are degrees, counter-clockwise from east.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Shape {
    Point {
        at: Point2,
    },
    Line {
        a: Point2,
        b: Point2,
        #[serde(default)]
        arrow_start: bool,
        #[serde(default)]
        arrow_end: bool,
    },
    /// Open or closed polyline; a closed four-point polyline is a box.
    Polyline {
        points: Vec<Point2>,
        #[serde(default)]
        closed: bool,
    },
    Arc {
        center: Point2,
        radius: Length,
        start_angle: f64,
        /// Positive is counter-clockwise.
        sweep: f64,
    },
    Circle {
        center: Point2,
        radius: Length,
    },
    Ellipse {
        center: Point2,
        rx: Length,
        ry: Length,
        #[serde(default)]
        rotation: f64,
    },
    /// Smooth curve through `points`.
    Spline {
        points: Vec<Point2>,
        #[serde(default)]
        closed: bool,
    },
    /// Detailing: a box along `a`→`b`, `width` across, with both diagonals
    /// (a continuous framing member in section).
    CrossBox {
        a: Point2,
        b: Point2,
        width: Length,
    },
    /// Detailing: a box with one diagonal (blocking / a discontinuous member).
    BlockingBox {
        a: Point2,
        b: Point2,
        width: Length,
    },
    /// Detailing: batt insulation symbol along `a`→`b`, `width` deep.
    Insulation {
        a: Point2,
        b: Point2,
        width: Length,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cad {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub shape: Shape,
    /// Overrides the layer colour.
    #[serde(default)]
    pub color: Option<Color>,
    /// Fill closed shapes with this colour.
    #[serde(default)]
    pub fill: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DimOrientation {
    /// Measured along the line from the first to the last point.
    #[default]
    Aligned,
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DimensionGeometry {
    /// A chain of linear dimensions between consecutive points (point-to-point,
    /// running and end-to-end dimensions are all chains).
    Linear {
        points: Vec<Point2>,
        #[serde(default)]
        orientation: DimOrientation,
        /// Where the dimension line sits. Aligned: distance from the line
        /// first→last, positive to its left. Horizontal: `points[0].y + offset`.
        /// Vertical: `points[0].x + offset`.
        offset: Length,
    },
    /// The angle at `center` from `start` to `end`, drawn at `radius`.
    Angular {
        center: Point2,
        start: Point2,
        end: Point2,
        radius: Length,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Dimension {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub geometry: DimensionGeometry,
    /// Replaces the measured value; `{}` inside is replaced with the value.
    #[serde(default)]
    pub text_override: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Text {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub position: Point2,
    pub text: String,
    /// `None` uses the layer's text style, then the default set's.
    #[serde(default)]
    pub style: Option<Id>,
    /// Degrees counter-clockwise.
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub justify: Justify,
    #[serde(default)]
    pub height: Option<Length>,
    #[serde(default)]
    pub bold: Option<bool>,
    #[serde(default)]
    pub color: Option<Color>,
}

/// What a schedule lists.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleKind {
    Doors,
    Windows,
    Rooms,
    Walls,
    /// Electrical, plumbing and HVAC symbols.
    Fixtures,
    Stairs,
    RoofPlanes,
}

impl ScheduleKind {
    pub const ALL: [ScheduleKind; 7] = [
        ScheduleKind::Doors,
        ScheduleKind::Windows,
        ScheduleKind::Rooms,
        ScheduleKind::Walls,
        ScheduleKind::Fixtures,
        ScheduleKind::Stairs,
        ScheduleKind::RoofPlanes,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ScheduleKind::Doors => "Door Schedule",
            ScheduleKind::Windows => "Window Schedule",
            ScheduleKind::Rooms => "Room Finish Schedule",
            ScheduleKind::Walls => "Wall Schedule",
            ScheduleKind::Fixtures => "Electrical, Plumbing and HVAC Schedule",
            ScheduleKind::Stairs => "Stair Schedule",
            ScheduleKind::RoofPlanes => "Roof Plane Schedule",
        }
    }
}

/// A schedule table placed in plan; its rows come from the live model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Schedule {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    /// Top-left corner of the table.
    pub position: Point2,
    pub schedule_kind: ScheduleKind,
    /// Columns to show, by name, in order; empty for all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    /// List objects on every floor, not just this one.
    #[serde(default)]
    pub all_floors: bool,
    /// Text height; `None` uses the layer's or default text style.
    #[serde(default)]
    pub text_height: Option<Length>,
}

/// Shape drawn around a marker's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkerShape {
    Circle,
    Hexagon,
    Square,
    Diamond,
}

/// A note with an optional leader arrow to what it points at, or a marker
/// (text inside a shape, e.g. a keyed note or detail number).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Callout {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub text: String,
    /// Where the text sits.
    pub at: Point2,
    /// Where the leader arrow points; no leader when `None`.
    #[serde(default)]
    pub target: Option<Point2>,
    #[serde(default)]
    pub marker: Option<MarkerShape>,
    #[serde(default)]
    pub height: Option<Length>,
}

/// A revision cloud: a scalloped outline around changed work, tagged with
/// the revision number in a triangle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RevisionCloud {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    /// Closed outline.
    pub points: Vec<Point2>,
    /// Length of each scallop.
    pub arc: Length,
    #[serde(default)]
    pub label: Option<String>,
}

/// A CAD shape with its own colours, outside the document (a block in a
/// library file, or linework for a new CAD detail).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DetailShape {
    pub shape: Shape,
    #[serde(default)]
    pub color: Option<Color>,
    #[serde(default)]
    pub fill: Option<Color>,
}

/// A CAD detail: a blank drafting sheet, not tied to any floor, for
/// construction details and other 2D drawings. Drafting objects whose `floor`
/// is the detail's id are drawn on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CadDetail {
    pub id: Id,
    pub name: String,
}

/// A CAD block definition: a named group of CAD shapes. Its shapes are `Cad`
/// objects whose `floor` is the block's id, drawn relative to the insertion
/// point at the origin. Blocks are placed with [`BlockRef`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Block {
    pub id: Id,
    pub name: String,
}

/// A placed copy of a block on a floor plan, view or CAD detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlockRef {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub block: Id,
    /// Where the block's insertion point sits.
    pub at: Point2,
    /// Degrees counter-clockwise.
    #[serde(default)]
    pub rotation: f64,
    #[serde(default = "one")]
    pub scale: f64,
    /// Drawn mirrored left to right (before rotation).
    #[serde(default)]
    pub mirrored: bool,
}

fn one() -> f64 {
    1.0
}
