//! Free-form 3D solids drawn with the CAD tools: an outline pushed up, or a
//! profile turned about a vertical axis.

use housecraft_units::{Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Color, Id};

/// How a solid's shape is made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SolidShape {
    /// A plan outline pushed straight up by `height`.
    Extrude { points: Vec<Point2>, height: Length },
    /// A profile of (radius, height) points turned about a vertical axis
    /// at `center`, through `sweep` degrees (360 for a full turn).
    Revolve {
        center: Point2,
        profile: Vec<[Length; 2]>,
        #[serde(default = "full_turn")]
        sweep: f64,
    },
}

fn full_turn() -> f64 {
    360.0
}

impl SolidShape {
    /// A cylinder of `radius` and `height` at `center`.
    pub fn cylinder(center: Point2, radius: Length, height: Length) -> Self {
        SolidShape::Revolve {
            center,
            profile: vec![
                [Length::ZERO, Length::ZERO],
                [radius, Length::ZERO],
                [radius, height],
                [Length::ZERO, height],
            ],
            sweep: 360.0,
        }
    }

    /// A cone of base `radius` and `height` at `center`.
    pub fn cone(center: Point2, radius: Length, height: Length) -> Self {
        SolidShape::Revolve {
            center,
            profile: vec![[Length::ZERO, Length::ZERO], [radius, Length::ZERO], [Length::ZERO, height]],
            sweep: 360.0,
        }
    }

    /// A sphere of `radius` resting on the floor at `center`.
    pub fn sphere(center: Point2, radius: Length) -> Self {
        let r = radius.mm();
        let profile = (0..=16)
            .map(|i| {
                let a = -std::f64::consts::FRAC_PI_2 + i as f64 / 16.0 * std::f64::consts::PI;
                [Length::from_mm(r * a.cos()), Length::from_mm(r + r * a.sin())]
            })
            .collect();
        SolidShape::Revolve {
            center,
            profile,
            sweep: 360.0,
        }
    }

    /// Height of its top above its bottom.
    pub fn height(&self) -> Length {
        match self {
            SolidShape::Extrude { height, .. } => *height,
            SolidShape::Revolve { profile, .. } => Length::from_mm(
                profile.iter().map(|p| p[1].mm()).fold(0.0, f64::max)
                    - profile.iter().map(|p| p[1].mm()).fold(f64::MAX, f64::min).min(0.0),
            ),
        }
    }
}

/// The ready-made shapes the 3D Solid tool draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SolidPreset {
    #[default]
    Box,
    Cylinder,
    Cone,
    Sphere,
}

impl SolidPreset {
    pub const ALL: [SolidPreset; 4] = [SolidPreset::Box, SolidPreset::Cylinder, SolidPreset::Cone, SolidPreset::Sphere];

    pub fn name(self) -> &'static str {
        match self {
            SolidPreset::Box => "Box",
            SolidPreset::Cylinder => "Cylinder",
            SolidPreset::Cone => "Cone",
            SolidPreset::Sphere => "Sphere",
        }
    }

    /// The shape filling the plan rectangle from `a` to `b`, `height` tall.
    pub fn shape(self, a: Point2, b: Point2, height: Length) -> SolidShape {
        let (x0, x1) = (a.x().mm().min(b.x().mm()), a.x().mm().max(b.x().mm()));
        let (y0, y1) = (a.y().mm().min(b.y().mm()), a.y().mm().max(b.y().mm()));
        let c = Point2(Length::from_mm((x0 + x1) / 2.0), Length::from_mm((y0 + y1) / 2.0));
        let r = Length::from_mm((x1 - x0).min(y1 - y0) / 2.0);
        let p = |x: f64, y: f64| Point2(Length::from_mm(x), Length::from_mm(y));
        match self {
            SolidPreset::Box => SolidShape::Extrude {
                points: vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)],
                height,
            },
            SolidPreset::Cylinder => SolidShape::cylinder(c, r, height),
            SolidPreset::Cone => SolidShape::cone(c, r, height),
            SolidPreset::Sphere => SolidShape::sphere(c, r),
        }
    }
}

/// Defaults for the 3D Solid tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SolidDefaults {
    pub preset: SolidPreset,
    pub height: Length,
    pub color: Color,
}

impl Default for SolidDefaults {
    fn default() -> Self {
        SolidDefaults {
            preset: SolidPreset::Box,
            height: Length::from_inches(36.0),
            color: Color::rgb(200, 200, 205),
        }
    }
}

/// A 3D solid on a floor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Solid {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub shape: SolidShape,
    /// Height of its bottom above the floor.
    #[serde(default)]
    pub elevation: Length,
    pub color: Color,
}
