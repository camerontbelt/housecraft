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
    /// Extents: left and right along the line from its start, bottom and
    /// top as heights. Unset sides show everything (or the line's ends when
    /// clipping sides); values past the ends expand the view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_left: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_right: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_bottom: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_top: Option<Length>,
    /// Leave out whatever is above the top of the callout's floor.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignore_above: bool,
    /// Jogs in a cross section's cutting plane, in order along the line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<SectionStep>,
    /// Draw what is below grade as hidden (dashed) lines.
    #[serde(default = "yes")]
    pub hidden_below_grade: bool,
    /// Label doors and windows with their schedule numbers (D01, W03).
    #[serde(default = "yes")]
    pub schedule_labels: bool,
}

/// A jog in a stepped cutting plane: from `at` along the line onward, the
/// cut lies `offset` beyond the line (toward where the view looks).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SectionStep {
    pub at: Length,
    pub offset: Length,
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

/// A stop on a walkthrough path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Keyframe {
    pub eye: Point3,
    pub target: Point3,
    /// Seconds to travel here from the previous keyframe.
    #[serde(default = "two")]
    pub seconds: f64,
    /// Seconds to hold here.
    #[serde(default)]
    pub pause: f64,
}

fn two() -> f64 {
    2.0
}

fn fps() -> f64 {
    24.0
}

/// A recorded camera path through or around the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Walkthrough {
    pub id: Id,
    pub name: String,
    pub keyframes: Vec<Keyframe>,
    /// Frames per second when exported.
    #[serde(default = "fps")]
    pub fps: f64,
    /// How it is drawn (lighting for this path); the view's when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderSettings>,
}

impl Walkthrough {
    /// How long it plays, seconds.
    pub fn duration(&self) -> f64 {
        self.keyframes
            .iter()
            .enumerate()
            .map(|(i, k)| if i == 0 { k.pause } else { k.seconds.max(0.0) + k.pause })
            .sum()
    }

    /// Eye and target (mm) `t` seconds in, along a smooth curve through the
    /// keyframes.
    pub fn sample(&self, t: f64) -> Option<([f64; 3], [f64; 3])> {
        let k = &self.keyframes;
        let first = k.first()?;
        if k.len() == 1 {
            return Some((first.eye.mm(), first.target.mm()));
        }
        // Find the segment (i-1 → i) and how far along it.
        let mut clock = first.pause;
        if t <= clock {
            return Some((first.eye.mm(), first.target.mm()));
        }
        for i in 1..k.len() {
            let travel = k[i].seconds.max(1e-6);
            if t <= clock + travel {
                let f = (t - clock) / travel;
                let at = |j: usize| k[j.min(k.len() - 1)].clone();
                let (p0, p1, p2, p3) = (at(i.saturating_sub(2)), at(i - 1), at(i), at(i + 1));
                let spline = |a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]| {
                    let f2 = f * f;
                    let f3 = f2 * f;
                    [0, 1, 2].map(|n| {
                        0.5 * (2.0 * b[n]
                            + (-a[n] + c[n]) * f
                            + (2.0 * a[n] - 5.0 * b[n] + 4.0 * c[n] - d[n]) * f2
                            + (-a[n] + 3.0 * b[n] - 3.0 * c[n] + d[n]) * f3)
                    })
                };
                return Some((
                    spline(p0.eye.mm(), p1.eye.mm(), p2.eye.mm(), p3.eye.mm()),
                    spline(p0.target.mm(), p1.target.mm(), p2.target.mm(), p3.target.mm()),
                ));
            }
            clock += travel;
            if t <= clock + k[i].pause {
                return Some((k[i].eye.mm(), k[i].target.mm()));
            }
            clock += k[i].pause;
        }
        let last = k.last()?;
        Some((last.eye.mm(), last.target.mm()))
    }
}

#[cfg(test)]
mod walk_tests {
    use super::*;

    #[test]
    fn walkthroughs_pass_through_their_keyframes() {
        let kf = |x: f64, s: f64, pause: f64| Keyframe {
            eye: Point3::from_mm(x, 0.0, 1600.0),
            target: Point3::from_mm(x + 1000.0, 0.0, 1600.0),
            seconds: s,
            pause,
        };
        let w = Walkthrough {
            id: Id(1),
            name: "w".into(),
            keyframes: vec![kf(0.0, 0.0, 0.0), kf(1000.0, 2.0, 1.0), kf(3000.0, 2.0, 0.0)],
            fps: 24.0,
            render: None,
        };
        assert_eq!(w.duration(), 5.0);
        assert_eq!(w.sample(0.0).unwrap().0[0], 0.0);
        assert!((w.sample(2.0).unwrap().0[0] - 1000.0).abs() < 1e-6);
        assert!((w.sample(2.5).unwrap().0[0] - 1000.0).abs() < 1e-6, "paused");
        let mid = w.sample(4.0).unwrap().0[0];
        assert!(mid > 1500.0 && mid < 2500.0, "{mid}");
        assert!((w.sample(99.0).unwrap().0[0] - 3000.0).abs() < 1e-6);
    }
}

/// A note or dimension placed on a 3D view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Mark3d {
    /// Text pinned to a point.
    Note { at: Point3, text: String },
    /// The distance between two points.
    Dimension { a: Point3, b: Point3 },
}

/// A note or dimension in 3D, shown in every 3D view or only one camera's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Annotation3d {
    pub id: Id,
    /// Only in this camera's view; every 3D view when unset.
    #[serde(default)]
    pub camera: Option<Id>,
    pub mark: Mark3d,
}
