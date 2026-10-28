//! Decks and raised floor platforms: a surface of planks on joists, beams,
//! posts and footings, built from its outline.

use housecraft_units::{Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Color, Id};

/// A deck (or a raised platform inside a room).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Deck {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub outline: Vec<Point2>,
    /// Round the outline into a smooth curve.
    #[serde(default)]
    pub curved: bool,
    /// Top of the decking above the floor's elevation (negative: below).
    pub height: Length,
    /// A solid floor (a raised platform) instead of spaced planks.
    #[serde(default)]
    pub platform: bool,
    pub plank_width: Length,
    pub plank_gap: Length,
    pub plank_thickness: Length,
    /// Degrees counter-clockwise from east the planks run.
    #[serde(default)]
    pub plank_angle: f64,
    /// A picture-frame border board round the edge.
    #[serde(default = "yes")]
    pub border: bool,
    pub joist_spacing: Length,
    pub joist_depth: Length,
    /// Beams sit this far in from the edges they run along.
    pub beam_inset: Length,
    /// Largest joist span between beams (or a beam and the ledger).
    pub max_span: Length,
    pub post_spacing: Length,
    /// How far the post footings reach below the ground.
    #[serde(default = "footing_depth")]
    pub footing_depth: Length,
    /// Fasten to the house with a ledger board where it touches a wall
    /// (no beam or posts there).
    #[serde(default = "yes")]
    pub ledger: bool,
    /// Rows of blocking between the joists (at mid-span, and every 8' on long spans).
    #[serde(default)]
    pub blocking: bool,
    /// The ledger board's depth; the joists' when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ledger_depth: Option<Length>,
    /// Beams placed by hand: distances across the deck from the ledger
    /// side (or the near edge); automatic when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beam_offsets: Vec<Length>,
    /// A railing round the free edges.
    #[serde(default = "yes")]
    pub railing: bool,
    pub railing_height: Length,
    /// A roof over it on posts at the corners.
    #[serde(default)]
    pub covered: bool,
    /// The covered roof's pitch (rise in 12); flat when 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub roof_pitch: f64,
    /// The pitched roof has gable ends (instead of hips).
    #[serde(default)]
    pub roof_gables: bool,
    /// Under a pitched roof the ceiling follows the roof (cathedral)
    /// instead of a flat ceiling at the posts' tops.
    #[serde(default)]
    pub vaulted: bool,
    /// Post footings are drawn in plan on this layer (none when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footing_layer: Option<Id>,
    pub color: Color,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// Settings new decks start with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DeckDefaults {
    pub plank_width: Length,
    pub plank_gap: Length,
    pub plank_thickness: Length,
    pub footing_depth: Length,
    pub railing_height: Length,
}

impl Default for DeckDefaults {
    fn default() -> Self {
        let i = Length::from_inches;
        DeckDefaults { plank_width: i(5.5), plank_gap: i(0.25), plank_thickness: i(1.0), footing_depth: footing_depth(), railing_height: i(36.0) }
    }
}

fn footing_depth() -> Length {
    Length::from_inches(18.0)
}

fn yes() -> bool {
    true
}

impl Deck {
    /// A deck with typical sizes.
    pub fn new(floor: Id, layer: Id, outline: Vec<Point2>, height: Length) -> Self {
        let i = Length::from_inches;
        Deck {
            id: Id(0),
            floor,
            layer,
            outline,
            curved: false,
            height,
            platform: false,
            plank_width: i(5.5),
            plank_gap: i(0.25),
            plank_thickness: i(1.0),
            plank_angle: 0.0,
            border: true,
            joist_spacing: i(16.0),
            joist_depth: i(7.25),
            beam_inset: i(24.0),
            max_span: Length::from_feet(10.0),
            post_spacing: Length::from_feet(8.0),
            footing_depth: footing_depth(),
            ledger: true,
            blocking: false,
            ledger_depth: None,
            beam_offsets: Vec::new(),
            railing: true,
            railing_height: i(36.0),
            covered: false,
            roof_pitch: 0.0,
            roof_gables: false,
            vaulted: false,
            footing_layer: None,
            color: Color::rgb(160, 115, 75),
        }
    }

    /// A raised platform with a solid top (no railing).
    pub fn platform(floor: Id, layer: Id, outline: Vec<Point2>, height: Length) -> Self {
        Deck {
            platform: true,
            railing: false,
            border: false,
            ledger: false,
            blocking: false,
            ledger_depth: None,
            beam_offsets: Vec::new(),
            joist_depth: Length::from_inches(5.5),
            color: Color::rgb(200, 180, 150),
            ..Deck::new(floor, layer, outline, height)
        }
    }
}
