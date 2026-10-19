//! 2D drawings of HouseCraft documents.
//!
//! [`plan`] turns one floor into a [`Drawing`]: a flat list of primitives in
//! model millimetres, each tagged with the object it came from. The GUI paints
//! it on screen, [`svg`] and [`pdf`] write it out, so screen and paper match.

pub mod detail;
mod cabinets;
mod dims;
pub mod elevation;
pub mod layout;
pub mod macros;
pub mod pdf;
mod plan;
pub mod svg;

use housecraft_geom::DVec2;
use housecraft_model::{Color, Id, Justify};

pub use dims::format_dimension;
pub use plan::{PlanOptions, drafting, plan};

/// What colour to draw with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ink {
    /// The theme's foreground (black on paper, light on a dark screen).
    Default,
    /// The default ink, faded (reference floors).
    Faded,
    Color(Color),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub ink: Ink,
    /// Pen width on paper, mm.
    pub width: f64,
    /// Dash pattern on paper, mm; empty = solid.
    pub dash: &'static [f64],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill {
    Ink(Ink),
    /// Wall poché: a mid-grey that reads in both themes.
    Poche,
    /// Cast sun shadows: a light tint under everything else.
    Shadow,
    /// The page background, to mask what lies beneath.
    Paper,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Polyline {
        points: Vec<DVec2>,
        closed: bool,
        stroke: Stroke,
    },
    Polygon {
        points: Vec<DVec2>,
        fill: Fill,
    },
    Text {
        at: DVec2,
        text: String,
        /// Character height, model mm.
        height: f64,
        /// Degrees counter-clockwise.
        rotation: f64,
        justify: Justify,
        /// Vertically centre on `at` instead of sitting on it.
        middle: bool,
        bold: bool,
        ink: Ink,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// The object this was drawn for (for selection), if any.
    pub object: Option<Id>,
    pub prim: Prim,
}

#[derive(Debug, Clone, Default)]
pub struct Drawing {
    pub items: Vec<Item>,
}

impl Drawing {
    pub fn push(&mut self, object: Option<Id>, prim: Prim) {
        self.items.push(Item { object, prim });
    }

    /// Model-space bounds of everything drawn (text by its anchor).
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        housecraft_geom::math::bounds(self.items.iter().flat_map(|i| match &i.prim {
            Prim::Polyline { points, .. } | Prim::Polygon { points, .. } => points.clone(),
            Prim::Text { at, .. } => vec![*at],
        }))
    }
}

/// Rough text width for layout when no font metrics are available.
pub fn approx_text_width(text: &str, height: f64) -> f64 {
    text.chars().count() as f64 * height * 0.55
}
