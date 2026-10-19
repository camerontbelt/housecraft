//! Imported content: pictures and 3D symbols.

use housecraft_units::{Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Color, Id, LightSettings, Renovation};

/// An image placed on a floor plan, view or CAD detail (a traced survey, a
/// logo, a photo).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Picture {
    pub id: Id,
    /// A floor, elevation/section view or CAD detail.
    pub floor: Id,
    pub layer: Id,
    /// Image file (absolute, or relative to the plan file).
    pub path: String,
    /// Bottom-left corner.
    pub position: Point2,
    pub width: Length,
    pub height: Length,
    /// Degrees counter-clockwise about the bottom-left corner.
    #[serde(default)]
    pub rotation: f64,
    /// Pixels of this colour are see-through.
    #[serde(default)]
    pub transparent: Option<Color>,
    /// 1 = solid, 0 = invisible.
    #[serde(default = "one")]
    pub opacity: f64,
}

fn one() -> f64 {
    1.0
}

/// A coloured part of an imported 3D symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SymbolGroup {
    /// First triangle (index into `triangles`) and how many.
    pub first: u32,
    pub count: u32,
    pub color: Color,
    /// Material name in the source file, if any.
    #[serde(default)]
    pub name: String,
}

/// What a symbol does besides being drawn (a "smart" symbol).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SymbolBehavior {
    #[default]
    Plain,
    /// Cuts its outline out of countertops it sits in.
    Sink,
    /// Hangs on a wall: placed against the nearest wall with its bottom
    /// `height` above the floor.
    WallHung { height: Length },
    /// A light fixture that lights the 3D view.
    Light { light: LightSettings },
}

impl SymbolBehavior {
    pub fn name(&self) -> &'static str {
        match self {
            SymbolBehavior::Plain => "Plain",
            SymbolBehavior::Sink => "Sink (cuts countertops)",
            SymbolBehavior::WallHung { .. } => "Wall-hung",
            SymbolBehavior::Light { .. } => "Light fixture",
        }
    }

    /// A light fixture with typical settings.
    pub fn light() -> Self {
        SymbolBehavior::Light {
            light: LightSettings {
                on: true,
                lumens: 800.0,
                dimmer: 1.0,
                kelvin: 2700.0,
                shadows: false,
            },
        }
    }
}

/// An imported 3D model, stored in the plan so the file is self-contained.
/// Coordinates are millimetres: x across, y front to back (the front faces
/// −y), z up, with the origin at the centre of the bottom of the back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SymbolDef {
    pub id: Id,
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    #[serde(default)]
    pub groups: Vec<SymbolGroup>,
    #[serde(default)]
    pub behavior: SymbolBehavior,
}

impl SymbolDef {
    /// Bounding box (min, max) in millimetres.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in &self.positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if self.positions.is_empty() {
            ([0.0; 3], [0.0; 3])
        } else {
            (lo, hi)
        }
    }

    /// Width, depth and height (mm).
    pub fn size(&self) -> [f64; 3] {
        let (lo, hi) = self.bounds();
        [0, 1, 2].map(|k| (hi[k] - lo[k]) as f64)
    }
}

/// A placed copy of a 3D symbol, placed like a cabinet: the centre of its
/// back at `position`, facing `facing`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Symbol {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub def: Id,
    pub position: Point2,
    /// Degrees counter-clockwise from east.
    pub facing: f64,
    /// Height of its bottom above the floor.
    #[serde(default)]
    pub elevation: Length,
    /// Size; the symbol is stretched to fit.
    pub width: Length,
    pub depth: Length,
    pub height: Length,
    #[serde(default)]
    pub renovation: Renovation,
}
