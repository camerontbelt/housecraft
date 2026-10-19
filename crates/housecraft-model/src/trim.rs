//! Trim: room moldings, molding polylines, custom profiles, siding
//! cladding and corner trim.

use housecraft_units::{Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Id, MoldingShape};

/// A custom molding cross-section: (out, up) points in mm, drawn with the
/// back (out = 0) against the surface and the bottom at up = 0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MoldingProfile {
    pub id: Id,
    pub name: String,
    pub points: Vec<[f64; 2]>,
}

/// Which profile a molding uses.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProfileRef {
    Shape { shape: MoldingShape },
    Custom { profile: Id },
}

impl Default for ProfileRef {
    fn default() -> Self {
        ProfileRef::Shape {
            shape: MoldingShape::Flat,
        }
    }
}

/// One molding: its profile scaled to `height` × `depth`, optionally
/// flipped upside down, turned 90°, or centred on its path.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MoldingSpec {
    #[serde(default)]
    pub profile: ProfileRef,
    pub height: Length,
    pub depth: Length,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub rotate: bool,
    #[serde(default)]
    pub center: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoomMoldingPlace {
    /// Baseboard at the floor.
    #[default]
    Base,
    /// Chair rail at `offset` above the floor.
    Chair,
    /// Crown at the ceiling.
    Crown,
}

/// A molding run around a room's walls (broken at doors for base and
/// chair rails).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoomMolding {
    pub place: RoomMoldingPlace,
    pub spec: MoldingSpec,
    /// Chair rails: height of the bottom above the floor.
    #[serde(default)]
    pub offset: Length,
}

impl RoomMolding {
    pub fn base() -> Self {
        RoomMolding {
            place: RoomMoldingPlace::Base,
            spec: MoldingSpec {
                profile: ProfileRef::Shape { shape: MoldingShape::Flat },
                height: Length::from_inches(3.5),
                depth: Length::from_inches(0.5),
                flip: false,
                rotate: false,
                center: false,
            },
            offset: Length::ZERO,
        }
    }

    pub fn crown() -> Self {
        RoomMolding {
            place: RoomMoldingPlace::Crown,
            spec: MoldingSpec {
                profile: ProfileRef::Shape { shape: MoldingShape::Crown },
                height: Length::from_inches(3.5),
                depth: Length::from_inches(2.5),
                flip: false,
                rotate: false,
                center: false,
            },
            offset: Length::ZERO,
        }
    }
}

/// A molding drawn along any path (wainscot cap, picture rail, exterior
/// band board), with profiles stacked from the bottom up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MoldingPolyline {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    /// The molding stands to the right of the path's direction.
    pub points: Vec<Point2>,
    #[serde(default)]
    pub closed: bool,
    /// Height of the bottom above the floor.
    pub elevation: Length,
    pub stack: Vec<MoldingSpec>,
}

/// 3D siding on a wall type's exterior face.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Cladding {
    /// Horizontal lap boards showing `exposure` each.
    Lap { exposure: Length },
    /// Vertical boards with battens `batten` wide every `spacing`.
    BoardAndBatten { spacing: Length, batten: Length },
    /// Metal panels with raised seams every `spacing`.
    StandingSeam { spacing: Length },
    /// Shingle courses `exposure` high with staggered joints.
    Shingle { exposure: Length },
}

/// Trim at the outside corners of walls.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CornerTrim {
    Board { width: Length },
    /// Alternating long and short stone blocks.
    Quoins { width: Length, height: Length },
}
