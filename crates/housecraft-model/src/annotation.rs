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
