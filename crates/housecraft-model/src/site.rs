//! The site: terrain and its elevation data.

use housecraft_units::{Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Color, Id};

/// The ground around the building: an outline whose surface is shaped by
/// the plan's elevation data. Heights are relative to the first floor's
/// elevation datum (0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Terrain {
    pub id: Id,
    /// The floor whose plan shows it (normally the lowest).
    pub floor: Id,
    pub layer: Id,
    /// The terrain's outline (the lot).
    pub perimeter: Vec<Point2>,
    /// Height where there is no elevation data.
    #[serde(default = "default_base")]
    pub base_elevation: Length,
    /// Smoothing passes over the surface (0 for none).
    #[serde(default = "two")]
    pub smoothing: u32,
    /// How far the sides drop below the surface in 3D.
    #[serde(default = "default_skirt")]
    pub skirt: Length,
    /// Contour line spacing in plan; zero for none.
    #[serde(default = "default_contour")]
    pub contour_interval: Length,
    /// Every this many contours is drawn heavier and labelled.
    #[serde(default = "five")]
    pub major_every: u32,
    /// Continue flat ground to the horizon in 3D.
    #[serde(default)]
    pub extend_to_horizon: bool,
    #[serde(default = "grass")]
    pub color: Color,
}

fn default_base() -> Length {
    Length::from_inches(-8.0)
}
fn two() -> u32 {
    2
}
fn five() -> u32 {
    5
}
fn default_skirt() -> Length {
    Length::from_feet(2.0)
}
fn default_contour() -> Length {
    Length::from_feet(1.0)
}
fn grass() -> Color {
    Color::rgb(140, 165, 110)
}

impl Terrain {
    /// A terrain with typical settings.
    pub fn new(floor: Id, layer: Id, perimeter: Vec<Point2>) -> Self {
        Terrain {
            id: Id(0),
            floor,
            layer,
            perimeter,
            base_elevation: default_base(),
            smoothing: two(),
            skirt: default_skirt(),
            contour_interval: default_contour(),
            major_every: five(),
            extend_to_horizon: false,
            color: grass(),
        }
    }
}

/// A piece of elevation data shaping the terrain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ElevationShape {
    /// A spot elevation.
    Point { at: Point2, elevation: Length },
    /// A contour-like line at one height (smoothed when `smooth`).
    Line {
        points: Vec<Point2>,
        elevation: Length,
        #[serde(default)]
        smooth: bool,
    },
    /// A flat area at one height (a pad, a terrace).
    Region { points: Vec<Point2>, elevation: Length },
    /// A rounded hill added to the ground.
    Hill { center: Point2, radius: Length, height: Length },
    /// A rounded hollow cut into the ground.
    Valley { center: Point2, radius: Length, depth: Length },
}

impl ElevationShape {
    pub fn name(&self) -> &'static str {
        match self {
            ElevationShape::Point { .. } => "elevation point",
            ElevationShape::Line { .. } => "elevation line",
            ElevationShape::Region { .. } => "flat region",
            ElevationShape::Hill { .. } => "hill",
            ElevationShape::Valley { .. } => "valley",
        }
    }
}

/// Elevation data drawn on a floor plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ElevationData {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub shape: ElevationShape,
}
