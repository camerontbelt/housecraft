//! Saved views: plan views and 3D cameras.

use housecraft_units::{Length, Point2, Point3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Id;

/// The visible region of a plan view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViewRect {
    pub center: Point2,
    /// Model millimetres per screen point.
    pub mm_per_point: f64,
}

/// A saved plan view: which floor, layer set and default set to use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanView {
    pub id: Id,
    pub name: String,
    /// `None` keeps whatever floor is current.
    pub floor: Option<Id>,
    pub layer_set: Id,
    pub default_set: Id,
    /// `None` keeps the current zoom.
    pub zoom: Option<ViewRect>,
    /// Show the floor below, faded, and snap to it.
    #[serde(default)]
    pub reference_floor: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Projection {
    Perspective {
        /// Vertical field of view in degrees.
        fov: f64,
    },
    Orthographic {
        /// Height of the visible region.
        height: Length,
    },
}

/// A saved 3D camera.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Camera {
    pub id: Id,
    pub name: String,
    pub projection: Projection,
    pub eye: Point3,
    pub target: Point3,
    pub layer_set: Id,
}
