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
    /// Show this floor as the reference instead of the one below.
    #[serde(default)]
    pub reference: Option<Id>,
    /// Which remodel phase the plan shows.
    #[serde(default)]
    pub phase: Phase,
}

/// Remodel phase a plan view shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Everything; demolished objects dashed.
    #[default]
    All,
    /// The building as it stands: existing and to-be-demolished objects.
    AsBuilt,
    /// Existing objects with demolition dashed; new work hidden.
    Demolition,
    /// The finished design: existing (lighter) and new; demolition hidden.
    Proposed,
}

impl Phase {
    pub const ALL: [Phase; 4] = [
        Phase::All,
        Phase::AsBuilt,
        Phase::Demolition,
        Phase::Proposed,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Phase::All => "All",
            Phase::AsBuilt => "As-Built",
            Phase::Demolition => "Demolition Plan",
            Phase::Proposed => "Proposed",
        }
    }

    /// Whether an object with this status appears in the phase.
    pub fn shows(self, r: crate::Renovation) -> bool {
        use crate::Renovation as R;
        match self {
            Phase::All => true,
            Phase::AsBuilt | Phase::Demolition => r != R::New,
            Phase::Proposed => r != R::Demolish,
        }
    }
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
