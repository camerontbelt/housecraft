//! Imported content: pictures and 3D symbols.

use housecraft_units::{Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Color, Id, Renovation};

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
