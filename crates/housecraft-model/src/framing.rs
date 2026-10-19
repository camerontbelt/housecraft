//! Structural framing: studs, plates, headers, joists, rafters and beams.

use housecraft_units::{Length, Point2, Point3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Id;

/// What a framing member does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FramingRole {
    #[default]
    Stud,
    KingStud,
    /// A jack (trimmer) stud under a header.
    JackStud,
    Cripple,
    Header,
    TopPlate,
    BottomPlate,
    /// The rough sill under a window.
    Sill,
    Blocking,
    FloorJoist,
    RimJoist,
    CeilingJoist,
    Rafter,
    Ridge,
    Beam,
    Post,
    /// A truss's top or bottom chord.
    Chord,
    /// A truss web.
    Web,
}

impl FramingRole {
    pub const ALL: [FramingRole; 18] = [
        FramingRole::Stud,
        FramingRole::KingStud,
        FramingRole::JackStud,
        FramingRole::Cripple,
        FramingRole::Header,
        FramingRole::TopPlate,
        FramingRole::BottomPlate,
        FramingRole::Sill,
        FramingRole::Blocking,
        FramingRole::FloorJoist,
        FramingRole::RimJoist,
        FramingRole::CeilingJoist,
        FramingRole::Rafter,
        FramingRole::Ridge,
        FramingRole::Beam,
        FramingRole::Post,
        FramingRole::Chord,
        FramingRole::Web,
    ];

    pub fn name(self) -> &'static str {
        match self {
            FramingRole::Stud => "Stud",
            FramingRole::KingStud => "King Stud",
            FramingRole::JackStud => "Jack Stud",
            FramingRole::Cripple => "Cripple",
            FramingRole::Header => "Header",
            FramingRole::TopPlate => "Top Plate",
            FramingRole::BottomPlate => "Bottom Plate",
            FramingRole::Sill => "Rough Sill",
            FramingRole::Blocking => "Blocking",
            FramingRole::FloorJoist => "Floor Joist",
            FramingRole::RimJoist => "Rim Joist",
            FramingRole::CeilingJoist => "Ceiling Joist",
            FramingRole::Rafter => "Rafter",
            FramingRole::Ridge => "Ridge",
            FramingRole::Beam => "Beam",
            FramingRole::Post => "Post",
            FramingRole::Chord => "Truss Chord",
            FramingRole::Web => "Truss Web",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FramingMaterial {
    #[default]
    Lumber,
    /// Laminated veneer lumber.
    Lvl,
    /// Parallel strand lumber.
    Psl,
    /// Versa-Lam style veneer lumber.
    Vsl,
    /// Light-gauge steel (C or U channel).
    Steel,
}

impl FramingMaterial {
    pub const ALL: [FramingMaterial; 5] = [
        FramingMaterial::Lumber,
        FramingMaterial::Lvl,
        FramingMaterial::Psl,
        FramingMaterial::Vsl,
        FramingMaterial::Steel,
    ];

    pub fn name(self) -> &'static str {
        match self {
            FramingMaterial::Lumber => "Lumber",
            FramingMaterial::Lvl => "LVL",
            FramingMaterial::Psl => "PSL",
            FramingMaterial::Vsl => "VSL",
            FramingMaterial::Steel => "Steel",
        }
    }
}

/// One framing member: a box from `a` to `b` (its centre line), `width`
/// by `depth` in section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FramingMember {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub role: FramingRole,
    pub a: Point3,
    pub b: Point3,
    /// The thin side (1 1/2" for dimension lumber).
    pub width: Length,
    /// The deep side.
    pub depth: Length,
    /// Vertical members: the plan direction (degrees) the depth runs in.
    /// Others: degrees the section is turned about the member from depth
    /// upright (90 lays it flat, like a plate).
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub material: FramingMaterial,
    /// The wall, room, floor or roof plane it was built for.
    #[serde(default)]
    pub host: Option<Id>,
    /// Placed or edited by hand: kept when framing is rebuilt.
    #[serde(default)]
    pub manual: bool,
    /// Pieces nailed side by side (a 2-ply beam); `width` is each piece's.
    #[serde(default = "one_ply")]
    pub plies: u32,
}

fn one_ply() -> u32 {
    1
}

impl FramingMember {
    pub fn length(&self) -> Length {
        let d = [0, 1, 2].map(|k| self.b.mm()[k] - self.a.mm()[k]);
        Length::from_mm((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
    }

    /// The trade name of its section, e.g. "2x6" or "3 1/2x11 7/8 LVL".
    pub fn size_name(&self) -> String {
        let nominal = |l: Length| -> String {
            let i = l.inches();
            // Dimension lumber is sold by its nominal size.
            for (actual, nom) in [(0.75, 1), (1.5, 2), (2.5, 3), (3.5, 4), (5.5, 6), (7.25, 8), (9.25, 10), (11.25, 12), (13.25, 14)] {
                if (i - actual).abs() < 0.06 {
                    return nom.to_string();
                }
            }
            let whole = i.floor();
            let frac = ((i - whole) * 8.0).round() as i64;
            match frac {
                0 => format!("{whole}"),
                8 => format!("{}", whole + 1.0),
                f => format!("{whole} {}/8", f),
            }
        };
        let s = format!("{}x{}", nominal(self.width), nominal(self.depth));
        let s = if self.plies > 1 { format!("{}-ply {s}", self.plies) } else { s };
        match self.material {
            FramingMaterial::Lumber => s,
            m => format!("{s} {}", m.name()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CornerFraming {
    /// Three studs at each corner.
    #[default]
    Standard,
    /// Two studs with drywall clips.
    TwoStud,
    /// Blocking across the end stud every 24".
    Ladder,
}

impl CornerFraming {
    pub const ALL: [CornerFraming; 3] = [CornerFraming::Standard, CornerFraming::TwoStud, CornerFraming::Ladder];

    pub fn name(self) -> &'static str {
        match self {
            CornerFraming::Standard => "Three-stud",
            CornerFraming::TwoStud => "Two-stud",
            CornerFraming::Ladder => "Ladder",
        }
    }
}

/// How framing is built.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FramingDefaults {
    pub stud_spacing: Length,
    pub joist_spacing: Length,
    pub ceiling_joist_spacing: Length,
    pub rafter_spacing: Length,
    /// The thin side of studs, joists and rafters.
    pub lumber_width: Length,
    pub floor_joist_depth: Length,
    pub ceiling_joist_depth: Length,
    pub rafter_depth: Length,
    pub top_plates: u32,
    pub corners: CornerFraming,
    /// Rows of blocking (bridging) at mid-height in walls.
    #[serde(default)]
    pub wall_blocking: bool,
    /// Frame roofs with trusses (chords and webs) instead of rafters and
    /// ceiling joists.
    #[serde(default)]
    pub trusses: bool,
    #[serde(default = "truss_spacing")]
    pub truss_spacing: Length,
}

fn truss_spacing() -> Length {
    Length::from_inches(24.0)
}

impl Default for FramingDefaults {
    fn default() -> Self {
        let i = Length::from_inches;
        FramingDefaults {
            stud_spacing: i(16.0),
            joist_spacing: i(16.0),
            ceiling_joist_spacing: i(16.0),
            rafter_spacing: i(24.0),
            lumber_width: i(1.5),
            floor_joist_depth: i(9.25),
            ceiling_joist_depth: i(5.5),
            rafter_depth: i(7.25),
            top_plates: 2,
            corners: CornerFraming::Standard,
            wall_blocking: false,
            trusses: false,
            truss_spacing: truss_spacing(),
        }
    }
}

/// Sets which way floor or ceiling joists run in the room it is drawn in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JoistDirection {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    /// A point in the room; the line is drawn through it.
    pub at: Point2,
    /// Degrees counter-clockwise from east that the joists run.
    pub angle: f64,
    /// Ceiling joists rather than floor joists.
    #[serde(default)]
    pub ceiling: bool,
}
