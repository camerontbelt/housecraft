//! How objects look: layers, layer sets, text styles, and default sets.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use housecraft_units::Length;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{DoorStyle, Hinge, Id, WindowStyle};

/// An sRGB colour, serialized as `"#rrggbb"` or `"#rrggbbaa"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color(pub [u8; 4]);

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color([r, g, b, 255])
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b, a] = self.0;
        if a == 255 {
            write!(f, "#{r:02x}{g:02x}{b:02x}")
        } else {
            write!(f, "#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }
}

impl FromStr for Color {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let hex = s.strip_prefix('#').ok_or("colour must start with #")?;
        let byte = |i: usize| {
            u8::from_str_radix(hex.get(i..i + 2).ok_or("short colour")?, 16)
                .map_err(|e| e.to_string())
        };
        match hex.len() {
            6 => Ok(Color([byte(0)?, byte(2)?, byte(4)?, 255])),
            8 => Ok(Color([byte(0)?, byte(2)?, byte(4)?, byte(6)?])),
            _ => Err(format!("invalid colour {s:?}")),
        }
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Color {
    fn schema_name() -> Cow<'static, str> {
        "Color".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "sRGB colour, \"#rrggbb\" or \"#rrggbbaa\"",
            "pattern": "^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$"
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    DashDot,
}

impl LineStyle {
    /// Dash pattern in paper millimetres (on, off, ...); empty = solid.
    pub fn pattern(self) -> &'static [f64] {
        match self {
            LineStyle::Solid => &[],
            LineStyle::Dashed => &[3.0, 1.5],
            LineStyle::Dotted => &[0.4, 1.0],
            LineStyle::DashDot => &[4.0, 1.0, 0.4, 1.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Layer {
    pub id: Id,
    pub name: String,
    /// `None` draws in the default ink (black on paper, light on dark screens).
    pub color: Option<Color>,
    /// Pen width on paper, in millimetres.
    pub line_weight: f64,
    pub line_style: LineStyle,
    /// Default text style for text on this layer.
    pub text_style: Option<Id>,
    /// Objects on this layer are counted in the materials list.
    pub in_materials_list: bool,
}

/// Per-layer overrides inside a [`LayerSet`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerDisplay {
    pub visible: bool,
    pub color: Option<Color>,
    pub line_weight: Option<f64>,
    pub line_style: Option<LineStyle>,
}

impl Default for LayerDisplay {
    fn default() -> Self {
        LayerDisplay {
            visible: true,
            color: None,
            line_weight: None,
            line_style: None,
        }
    }
}

/// A named configuration of layer visibility and appearance, e.g. "Electrical Plan".
/// Layers missing from `layers` are visible with their own appearance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerSet {
    pub id: Id,
    pub name: String,
    #[serde(with = "id_keys")]
    #[schemars(with = "BTreeMap<String, LayerDisplay>")]
    pub layers: BTreeMap<Id, LayerDisplay>,
}

/// Maps keyed by [`Id`] inside tagged enums: serde buffers those, which turns
/// integer keys into strings, so accept string keys when reading.
mod id_keys {
    use super::*;

    pub fn serialize<S: Serializer, V: Serialize>(
        map: &BTreeMap<Id, V>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.collect_map(map.iter().map(|(k, v)| (k.0.to_string(), v)))
    }

    pub fn deserialize<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
        d: D,
    ) -> Result<BTreeMap<Id, V>, D::Error> {
        BTreeMap::<String, V>::deserialize(d)?
            .into_iter()
            .map(|(k, v)| {
                k.parse()
                    .map(|n| (Id(n), v))
                    .map_err(|_| serde::de::Error::custom(format!("invalid id key {k:?}")))
            })
            .collect()
    }
}

impl LayerSet {
    pub fn display(&self, layer: Id) -> LayerDisplay {
        self.layers.get(&layer).cloned().unwrap_or_default()
    }
    pub fn is_visible(&self, layer: Id) -> bool {
        self.layers.get(&layer).is_none_or(|d| d.visible)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Justify {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextStyle {
    pub id: Id,
    pub name: String,
    /// Character height in model units.
    pub height: Length,
    pub font: String,
    pub bold: bool,
    pub italic: bool,
    /// `None` uses the layer colour.
    pub color: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArrowStyle {
    #[default]
    Tick,
    Arrow,
    Dot,
    None,
}

/// Which layer each kind of new object goes on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KindLayers {
    pub walls: Id,
    pub doors: Id,
    pub windows: Id,
    pub rooms: Id,
    pub cad: Id,
    pub dimensions: Id,
    pub text: Id,
    /// Roof planes; falls back to the walls layer when unset (older files).
    #[serde(default)]
    pub roofs: Option<Id>,
    /// Stairs; falls back to the walls layer when unset (older files).
    #[serde(default)]
    pub stairs: Option<Id>,
    /// Electrical; falls back to the CAD layer when unset (older files).
    #[serde(default)]
    pub electrical: Option<Id>,
    /// Cabinets, countertops and appliances; CAD layer when `None`.
    #[serde(default)]
    pub cabinets: Option<Id>,
    /// Framing members; made (hidden in plan) the first time framing is built.
    #[serde(default)]
    pub framing: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DimensionDefaults {
    pub text_style: Id,
    pub arrow: ArrowStyle,
    /// Gap between the measured point and the start of the extension line.
    pub extension_gap: Length,
    /// How far extension lines run past the dimension line.
    pub extension_overshoot: Length,
    /// Distance of automatic exterior dimensions from the building.
    pub exterior_offset: Length,
    /// Values shorter than this are shown in inches only (e.g. 18" instead of 1'-6").
    pub inches_below: Length,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DoorDefaults {
    pub style: DoorStyle,
    pub width: Length,
    pub height: Length,
    pub hinge: Hinge,
    /// Sizes for particular door types, overriding the typical ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sizes: Vec<DoorSize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DoorSize {
    pub style: DoorStyle,
    pub width: Length,
    pub height: Length,
}

impl DoorDefaults {
    /// Width and height for a new door of `style`: a per-type size if set,
    /// the default size for the default style, else the style's typical size.
    pub fn size_for(&self, style: DoorStyle) -> (Length, Length) {
        if let Some(s) = self.sizes.iter().find(|s| s.style == style) {
            return (s.width, s.height);
        }
        if style == self.style {
            return (self.width, self.height);
        }
        style.typical_size().unwrap_or((self.width, self.height))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WindowDefaults {
    pub style: WindowStyle,
    pub width: Length,
    pub height: Length,
    /// Height of the top of the window above the floor.
    pub head_height: Length,
}

/// A named bundle of defaults for new objects. Plan views pick one, so a
/// kitchen plan and a framing plan can create objects differently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DefaultSet {
    pub id: Id,
    pub name: String,
    pub exterior_wall_type: Id,
    pub interior_wall_type: Id,
    pub room_type: Id,
    pub text_style: Id,
    pub layers: KindLayers,
    pub dimensions: DimensionDefaults,
    pub door: DoorDefaults,
    pub window: WindowDefaults,
    #[serde(default)]
    pub roof: RoofDefaults,
    #[serde(default)]
    pub stair: StairDefaults,
    /// Used for stairs that start outside the building.
    #[serde(default = "StairDefaults::exterior")]
    pub exterior_stair: StairDefaults,
    #[serde(default)]
    pub cabinets: crate::CabinetDefaults,
    #[serde(default)]
    pub solids: crate::SolidDefaults,
    #[serde(default)]
    pub framing: crate::FramingDefaults,
    /// Default material for each surface role (empty: built-in colours).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub materials: crate::SurfaceMaterials,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StairDefaults {
    pub width: Length,
    pub max_riser: Length,
    pub tread_depth: Length,
    pub railing_height: Length,
    pub railing_left: bool,
    pub railing_right: bool,
    #[serde(default = "stair_overhang")]
    pub tread_overhang: Length,
    #[serde(default = "stair_stringers")]
    pub stringers: bool,
}

fn stair_overhang() -> Length {
    Length::from_inches(1.0)
}

fn stair_stringers() -> bool {
    true
}

impl StairDefaults {
    /// Typical exterior stairs (decks, porches): wider, deeper treads,
    /// rails both sides, no nosing.
    pub fn exterior() -> Self {
        StairDefaults {
            width: Length::from_inches(42.0),
            max_riser: Length::from_inches(7.5),
            tread_depth: Length::from_inches(11.0),
            railing_height: Length::from_inches(36.0),
            railing_left: true,
            railing_right: true,
            tread_overhang: Length::ZERO,
            stringers: true,
        }
    }
}

impl Default for StairDefaults {
    fn default() -> Self {
        StairDefaults {
            width: Length::from_inches(36.0),
            max_riser: Length::from_inches(7.75),
            tread_depth: Length::from_inches(10.0),
            railing_height: Length::from_inches(36.0),
            railing_left: false,
            railing_right: true,
            tread_overhang: stair_overhang(),
            stringers: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoofDefaults {
    /// Degrees above horizontal (6 in 12 = 26.57°).
    pub pitch: f64,
    /// Eave overhang beyond the walls' outer faces.
    pub overhang: Length,
    /// Overhang past gable walls.
    pub rake_overhang: Length,
    /// Rafter + sheathing depth, drawn as the roof's thickness.
    pub thickness: Length,
    /// Raised plate (energy heel) above the ceiling.
    pub heel: Length,
    /// Depth of the fascia board along eaves (0 for none).
    #[serde(default = "fascia_depth")]
    pub fascia: Length,
    /// A shadow board on the fascia's face (0 for none).
    #[serde(default)]
    pub shadow_board: Length,
}

fn fascia_depth() -> Length {
    Length::from_inches(7.25)
}

impl Default for RoofDefaults {
    fn default() -> Self {
        RoofDefaults {
            pitch: (6.0f64 / 12.0).atan().to_degrees(),
            overhang: Length::from_inches(16.0),
            rake_overhang: Length::from_inches(12.0),
            thickness: Length::from_inches(8.0),
            heel: Length::ZERO,
            fascia: fascia_depth(),
            shadow_board: Length::ZERO,
        }
    }
}
