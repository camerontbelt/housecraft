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
    /// Floors shown as references; empty = the floor below.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<Id>,
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

/// Elevation (everything in view) or cross section (cut at the line).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    #[default]
    Elevation,
    Section,
}

/// A 2D elevation or cross-section view. It looks across the line `a`→`b`
/// toward its left side; the line shows in plan as a callout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SectionView {
    pub id: Id,
    pub name: String,
    /// Floor whose plan shows the callout.
    pub floor: Id,
    pub layer: Id,
    pub a: Point2,
    pub b: Point2,
    #[serde(default)]
    pub section_kind: SectionKind,
    /// How far beyond the line to draw; everything when `None`.
    #[serde(default)]
    pub depth: Option<Length>,
    /// Only draw what lies between the line's ends (room elevations).
    #[serde(default)]
    pub clip_sides: bool,
}

/// A layout: sheets of paper holding scaled, live views of the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Layout {
    pub id: Id,
    pub name: String,
    /// Sheet size (paper millimetres as lengths).
    pub paper_width: Length,
    pub paper_height: Length,
    /// Page names, in order.
    pub pages: Vec<String>,
    /// Draw a border and title block on every page.
    #[serde(default = "yes")]
    pub title_block: bool,
}

fn yes() -> bool {
    true
}

/// What a layout box shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayoutSource {
    /// A floor plan as a layer set shows it.
    Plan {
        floor: Id,
        layer_set: Option<Id>,
        #[serde(default)]
        phase: Phase,
    },
    /// An elevation or cross section.
    View {
        view: Id,
    },
    /// A CAD detail.
    Detail {
        detail: Id,
    },
    /// A schedule table.
    Schedule {
        schedule_kind: crate::ScheduleKind,
        #[serde(default)]
        floor: Option<Id>,
    },
    MaterialsList,
    /// Free text (notes); `{macros}` are filled in.
    Text {
        text: String,
    },
}

/// A view placed on a layout page. Paper positions are lengths in paper
/// millimetres from the sheet's bottom-left corner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayoutBox {
    pub id: Id,
    pub layout: Id,
    /// Page index (0-based).
    pub page: u32,
    pub source: LayoutSource,
    pub x: Length,
    pub y: Length,
    pub width: Length,
    pub height: Length,
    /// Model millimetres per paper millimetre (48 = 1/4" = 1'-0"); 0 fits a standard scale.
    #[serde(default)]
    pub scale: f64,
    /// Model point at the box centre; the drawing's centre when `None`.
    #[serde(default)]
    pub center: Option<Point2>,
    /// Caption under the box; the source's name when `None`.
    #[serde(default)]
    pub label: Option<String>,
}
