//! Saved views: plan views and 3D cameras.

use housecraft_units::{Length, Point2, Point3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Color, Id, Sun};

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
    /// Image shown behind the model when this camera is viewed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<String>,
    /// How the view is drawn (style, lines, fog, sky, lights).
    #[serde(default)]
    pub render: RenderSettings,
}

/// A 3D drawing style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RenderStyle {
    /// Materials, sun and lights.
    #[default]
    Standard,
    /// Everything one clay colour, lit and shadowed.
    Clay,
    /// Flat colours with outlines, no shadows (a technical illustration).
    Technical,
    /// Soft bands of colour on paper.
    Watercolor,
    /// Shades between two colours.
    Duotone,
    /// Walls and roofs see-through, the rest solid.
    GlassHouse,
}

impl RenderStyle {
    pub const ALL: [RenderStyle; 6] = [
        RenderStyle::Standard,
        RenderStyle::Clay,
        RenderStyle::Technical,
        RenderStyle::Watercolor,
        RenderStyle::Duotone,
        RenderStyle::GlassHouse,
    ];

    pub fn name(self) -> &'static str {
        match self {
            RenderStyle::Standard => "Standard",
            RenderStyle::Clay => "Clay",
            RenderStyle::Technical => "Technical Illustration",
            RenderStyle::Watercolor => "Watercolor",
            RenderStyle::Duotone => "Duotone",
            RenderStyle::GlassHouse => "Glass House",
        }
    }

    /// Its number in the shader.
    pub fn index(self) -> u32 {
        RenderStyle::ALL.iter().position(|s| *s == self).unwrap_or(0) as u32
    }
}

/// The sky behind a 3D view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Sky {
    pub zenith: Color,
    pub horizon: Color,
    /// Sunlight strength, 0–2 (1 is a clear day).
    pub sun_brightness: f64,
    /// Night: moonlight instead of sunlight, dark sky.
    #[serde(default)]
    pub night: bool,
}

impl Default for Sky {
    fn default() -> Self {
        Sky {
            zenith: Color::rgb(120, 160, 210),
            horizon: Color::rgb(205, 220, 235),
            sun_brightness: 1.0,
            night: false,
        }
    }
}

/// Depth of field: things away from the focus distance blur (in high
/// quality renders).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DepthOfField {
    pub focus: Length,
    /// Lens f-number: smaller blurs more (f/2 is shallow, f/16 deep).
    pub f_stop: f64,
}

/// How a 3D view is drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RenderSettings {
    pub style: RenderStyle,
    /// Draw dark lines along edges over the shading.
    #[serde(default)]
    pub outlines: bool,
    /// How dark shadows are, 0 (none) to 1 (full).
    #[serde(default = "one")]
    pub shadow_intensity: f64,
    /// Distance fog (depth cueing), 0 (none) to 1 (thick).
    #[serde(default)]
    pub fog: f64,
    /// Brightness multiplier.
    #[serde(default = "one")]
    pub exposure: f64,
    #[serde(default = "duo_dark")]
    pub duotone_dark: Color,
    #[serde(default = "duo_light")]
    pub duotone_light: Color,
    #[serde(default)]
    pub sky: Sky,
    /// Hide walls between the camera and what it looks at (for looking
    /// into small rooms).
    #[serde(default)]
    pub hide_near_walls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth_of_field: Option<DepthOfField>,
    /// The sun for this view (a sun study); the plan's sun when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sun: Option<Sun>,
    /// Light fixtures turned off in this view (a lighting set).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lights_off: Vec<Id>,
    /// A 360° (equirectangular) image around the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panorama: Option<Panorama>,
}

/// A 360° backdrop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Panorama {
    pub path: String,
    /// Turn, degrees.
    #[serde(default)]
    pub rotation: f64,
    /// Raise or lower the horizon, -1 to 1.
    #[serde(default)]
    pub horizon: f64,
    /// Light the model with the image's colours.
    #[serde(default)]
    pub lights_scene: bool,
}

fn one() -> f64 {
    1.0
}
fn duo_dark() -> Color {
    Color::rgb(30, 45, 80)
}
fn duo_light() -> Color {
    Color::rgb(245, 225, 190)
}

impl Default for RenderSettings {
    fn default() -> Self {
        RenderSettings {
            style: RenderStyle::Standard,
            outlines: false,
            shadow_intensity: 1.0,
            fog: 0.0,
            exposure: 1.0,
            duotone_dark: duo_dark(),
            duotone_light: duo_light(),
            sky: Sky::default(),
            hide_near_walls: false,
            depth_of_field: None,
            sun: None,
            lights_off: Vec::new(),
            panorama: None,
        }
    }
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
