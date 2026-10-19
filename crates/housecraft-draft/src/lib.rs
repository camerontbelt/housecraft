//! 2D drawings of HouseCraft documents.
//!
//! [`plan`] turns one floor into a [`Drawing`]: a flat list of primitives in
//! model millimetres, each tagged with the object it came from. The GUI paints
//! it on screen, [`svg`] and [`pdf`] write it out, so screen and paper match.

pub mod detail;
pub mod dxf;
pub mod fills;
mod cabinets;
mod dims;
pub mod elevation;
pub mod layout;
pub mod macros;
pub mod pdf;
mod plan;
pub mod rich;
mod site;
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
    /// An image file stretched over a quadrilateral: corners bottom-left,
    /// bottom-right, top-right, top-left.
    Image {
        corners: [DVec2; 4],
        path: String,
        opacity: f64,
        transparent: Option<housecraft_model::Color>,
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
            Prim::Image { corners, .. } => corners.to_vec(),
        }))
    }
}

/// Rough text width for layout when no font metrics are available.
pub fn approx_text_width(text: &str, height: f64) -> f64 {
    text.chars().count() as f64 * height * 0.55
}

/// Standard base64 (for embedding images in SVG).
pub(crate) fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod base64_tests {
    #[test]
    fn encodes() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
    }
}
