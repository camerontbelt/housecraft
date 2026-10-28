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
    /// A patterned fill for closed shapes (over the layer's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_style: Option<crate::FillStyle>,
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
    /// Made by Auto Exterior Dimensions: rebuilt when the walls change.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto: bool,
    /// Named dimension style (`settings.dimension_styles`); `None` uses the
    /// default set's dimension defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
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
    /// Arrow tips: a leader runs from the text's nearer side to each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leaders: Vec<Point2>,
    /// Draw a box around the text.
    #[serde(default)]
    pub border: bool,
    /// Space between the text and its border; a third of the letter height
    /// when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_margin: Option<Length>,
    /// Wrap lines wider than this.
    #[serde(default)]
    pub wrap: Option<Length>,
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
    /// Cabinets with their door and drawer sizes.
    Cabinets,
    /// Trees, shrubs and other plants (a landscaping plan's planting list).
    Plants,
    /// Framing members by role, size and cut length (a cut list).
    Framing,
    /// Closed CAD shapes (polylines, rectangles, circles): area and perimeter.
    Shapes,
    /// Numbered notes and their text.
    Notes,
    /// Roads, driveways and sidewalks: width, length, area.
    Hardscape,
}

impl ScheduleKind {
    pub const ALL: [ScheduleKind; 13] = [
        ScheduleKind::Doors,
        ScheduleKind::Windows,
        ScheduleKind::Rooms,
        ScheduleKind::Walls,
        ScheduleKind::Fixtures,
        ScheduleKind::Stairs,
        ScheduleKind::RoofPlanes,
        ScheduleKind::Cabinets,
        ScheduleKind::Plants,
        ScheduleKind::Framing,
        ScheduleKind::Shapes,
        ScheduleKind::Notes,
        ScheduleKind::Hardscape,
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
            ScheduleKind::Cabinets => "Cabinet Schedule",
            ScheduleKind::Plants => "Plant Schedule",
            ScheduleKind::Framing => "Framing Cut List",
            ScheduleKind::Shapes => "Shape Schedule",
            ScheduleKind::Notes => "Note Schedule",
            ScheduleKind::Hardscape => "Hardscape Schedule",
        }
    }
}

/// A schedule table placed in plan; its rows come from the live model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Schedule {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    /// The table's title; the kind's name when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Only objects inside this room (Create Schedule from Room).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<Id>,
    /// Only rows with this text in a cell (any case), e.g. "light".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub filter: String,
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
    /// Other kinds listed in the same table (a mixed schedule).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<ScheduleKind>,
    /// Only existing, new or demolished objects (renovation plans).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renovation: Option<crate::Renovation>,
    /// Number labels (D01…) from this instead of 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_number: Option<u32>,
    /// A small plan picture of each row's object.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub thumbnails: bool,
    /// Rows put in order by hand (Move Up / Move Down): their objects, first
    /// to last; other rows follow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<Id>,
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
    /// The view, detail, plan view or layout this callout opens (and whose
    /// layout page it can name with `{link}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Id>,
    /// A numbered note: the marker shows its number and the text is listed
    /// in the Note Schedule.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub note: bool,
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

/// An architectural block: objects on a floor grouped so they select, move,
/// copy and delete as one. Exploding it leaves the objects as they are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Group {
    pub id: Id,
    pub floor: Id,
    pub name: String,
    pub members: Vec<Id>,
    /// A ganged electrical block: outlets and switches in one box, drawn
    /// with one plate round them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ganged: bool,
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
