//! Materials: what surfaces are made of and how they look in 3D.
//!
//! Every surface the 3D model draws has a [`Surface`] role (wall, floor,
//! roof, glass…). Its material comes from, first to last:
//! a [`MaterialAssignment`] on the object for that surface, one on the
//! object for all its surfaces, one on the object's floor for that surface,
//! then the default set's material for the surface.

use std::collections::BTreeMap;

use housecraft_units::Length;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Color, Id};

/// The role a surface plays, which picks its default material.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// Wall faces (siding outside, paint inside).
    Wall,
    Floor,
    Ceiling,
    Roof,
    /// Eave and rake undersides, fascia.
    Soffit,
    /// Door slabs.
    Door,
    Glass,
    /// Door and window frames, casing.
    Frame,
    /// Foundations, piers, slabs.
    Concrete,
    /// Floor platform edges and sub-floor.
    Platform,
    /// Treads and risers.
    Stair,
    Stringer,
    Railing,
    /// Outlets, switches, registers.
    Device,
    /// Light fixture lenses.
    Lamp,
    /// Cabinet boxes (sides, back, toe kick).
    CabinetBox,
    /// Cabinet doors and drawer fronts.
    CabinetDoor,
    Countertop,
    Backsplash,
    /// Knobs and pulls.
    Hardware,
}

impl Surface {
    pub const ALL: [Surface; 20] = [
        Surface::Wall,
        Surface::Floor,
        Surface::Ceiling,
        Surface::Roof,
        Surface::Soffit,
        Surface::Door,
        Surface::Glass,
        Surface::Frame,
        Surface::Concrete,
        Surface::Platform,
        Surface::Stair,
        Surface::Stringer,
        Surface::Railing,
        Surface::Device,
        Surface::Lamp,
        Surface::CabinetBox,
        Surface::CabinetDoor,
        Surface::Countertop,
        Surface::Backsplash,
        Surface::Hardware,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Surface::Wall => "Walls",
            Surface::Floor => "Floors",
            Surface::Ceiling => "Ceilings",
            Surface::Roof => "Roofing",
            Surface::Soffit => "Soffit & Fascia",
            Surface::Door => "Doors",
            Surface::Glass => "Glass",
            Surface::Frame => "Frames & Casing",
            Surface::Concrete => "Concrete",
            Surface::Platform => "Floor Platform",
            Surface::Stair => "Stair Treads",
            Surface::Stringer => "Stringers",
            Surface::Railing => "Railings",
            Surface::Device => "Electrical Devices",
            Surface::Lamp => "Light Fixtures",
            Surface::CabinetBox => "Cabinet Boxes",
            Surface::CabinetDoor => "Cabinet Doors & Drawers",
            Surface::Countertop => "Countertops",
            Surface::Backsplash => "Backsplashes",
            Surface::Hardware => "Cabinet Hardware",
        }
    }
}

/// How a material reflects and transmits light.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MaterialClass {
    /// Plain diffuse surface (paint, wood, stone…).
    #[default]
    Generic,
    /// Smooth, slightly shiny (laminate, enamel).
    Plastic,
    Metal,
    Mirror,
    /// Clear, see-through.
    Glass,
    /// Lets light through but blurs it (frosted glass, fabric shades).
    Translucent,
    /// Gives off its own light (lamp lenses, screens).
    Emissive,
}

impl MaterialClass {
    pub const ALL: [MaterialClass; 7] = [
        MaterialClass::Generic,
        MaterialClass::Plastic,
        MaterialClass::Metal,
        MaterialClass::Mirror,
        MaterialClass::Glass,
        MaterialClass::Translucent,
        MaterialClass::Emissive,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MaterialClass::Generic => "General",
            MaterialClass::Plastic => "Plastic",
            MaterialClass::Metal => "Metal",
            MaterialClass::Mirror => "Mirror",
            MaterialClass::Glass => "Glass",
            MaterialClass::Translucent => "Translucent",
            MaterialClass::Emissive => "Emissive",
        }
    }
}

/// An image map: a file and how big one tile of it is in the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextureMap {
    /// Image file (absolute, or relative to the plan file).
    pub path: String,
    /// How strongly the map applies, 0–1.
    #[serde(default = "full")]
    pub strength: f64,
}

fn full() -> f64 {
    1.0
}

/// Physically based image maps; any may be absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaterialMaps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<TextureMap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal: Option<TextureMap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<TextureMap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness: Option<TextureMap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metallic: Option<TextureMap>,
    /// Ambient occlusion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ao: Option<TextureMap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emissive: Option<TextureMap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<TextureMap>,
}

/// A material: colour plus physically based properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Material {
    pub id: Id,
    pub name: String,
    /// Library category, e.g. "Paint", "Wood", "Stone".
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub class: MaterialClass,
    pub color: Color,
    /// 0 = mirror-smooth, 1 = fully matte.
    #[serde(default = "matte")]
    pub roughness: f64,
    /// 0 = dielectric, 1 = metal.
    #[serde(default)]
    pub metallic: f64,
    /// 1 = opaque, 0 = invisible.
    #[serde(default = "full")]
    pub opacity: f64,
    /// Light given off, 0 = none, 1 = as bright as its colour.
    #[serde(default)]
    pub emissive: f64,
    /// Stretch of highlights along the grain (brushed metal), -1–1.
    #[serde(default)]
    pub anisotropy: f64,
    /// The roughness map holds gloss (white = shiny) and is inverted.
    #[serde(default)]
    pub gloss_map: bool,
    #[serde(default)]
    pub maps: MaterialMaps,
    /// Real-world size of one tile of the image maps.
    #[serde(default)]
    pub texture_width: Option<Length>,
    #[serde(default)]
    pub texture_height: Option<Length>,
}

fn matte() -> f64 {
    0.8
}

impl Material {
    /// A plain material of the given colour and class with sensible
    /// properties for the class.
    pub fn new(name: &str, category: &str, class: MaterialClass, color: Color) -> Self {
        let (roughness, metallic, opacity, emissive) = match class {
            MaterialClass::Generic => (0.8, 0.0, 1.0, 0.0),
            MaterialClass::Plastic => (0.35, 0.0, 1.0, 0.0),
            MaterialClass::Metal => (0.3, 1.0, 1.0, 0.0),
            MaterialClass::Mirror => (0.02, 1.0, 1.0, 0.0),
            MaterialClass::Glass => (0.05, 0.0, 0.25, 0.0),
            MaterialClass::Translucent => (0.6, 0.0, 0.6, 0.0),
            MaterialClass::Emissive => (0.5, 0.0, 1.0, 1.0),
        };
        Material {
            id: Id(0),
            name: name.into(),
            category: category.into(),
            class,
            color,
            roughness,
            metallic,
            opacity,
            emissive,
            anisotropy: 0.0,
            gloss_map: false,
            maps: MaterialMaps::default(),
            texture_width: None,
            texture_height: None,
        }
    }
}

/// A painted material: `material` on `object`'s `surface` (all its surfaces
/// when `None`). An assignment on a floor applies to everything on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaterialAssignment {
    pub id: Id,
    pub object: Id,
    #[serde(default)]
    pub surface: Option<Surface>,
    pub material: Id,
}

/// The default set's material for each surface role.
pub type SurfaceMaterials = BTreeMap<Surface, Id>;

/// Looks up the material for an object's surface (see the module docs).
pub struct MaterialIndex<'a> {
    doc: &'a crate::Document,
    painted: std::collections::HashMap<(Id, Option<Surface>), Id>,
    defaults: &'a SurfaceMaterials,
}

impl<'a> MaterialIndex<'a> {
    pub fn new(doc: &'a crate::Document) -> Self {
        let painted = doc
            .iter::<MaterialAssignment>()
            .map(|a| ((a.object, a.surface), a.material))
            .collect();
        MaterialIndex {
            doc,
            painted,
            defaults: &doc.active_defaults().materials,
        }
    }

    /// The material `object`'s `surface` is drawn with, if any is set.
    pub fn resolve(&self, object: Id, surface: Surface) -> Option<&'a Material> {
        let floor = self.doc.floor_of(object);
        [Some(object), floor]
            .into_iter()
            .flatten()
            .flat_map(|o| [(o, Some(surface)), (o, None)])
            .find_map(|k| self.painted.get(&k))
            .or_else(|| self.defaults.get(&surface))
            .and_then(|m| self.doc.get::<Material>(*m))
    }
}
