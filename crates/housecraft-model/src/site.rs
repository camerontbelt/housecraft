//! The site: terrain and its elevation data.

use housecraft_units::{Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Color, Id};

/// The ground around the building: an outline whose surface is shaped by
/// the plan's elevation data. Heights are relative to the first floor's
/// elevation datum (0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Terrain {
    pub id: Id,
    /// The floor whose plan shows it (normally the lowest).
    pub floor: Id,
    pub layer: Id,
    /// The terrain's outline (the lot).
    pub perimeter: Vec<Point2>,
    /// Height where there is no elevation data.
    #[serde(default = "default_base")]
    pub base_elevation: Length,
    /// Smoothing passes over the surface (0 for none).
    #[serde(default = "two")]
    pub smoothing: u32,
    /// How far the sides drop below the surface in 3D.
    #[serde(default = "default_skirt")]
    pub skirt: Length,
    /// Contour line spacing in plan; zero for none.
    #[serde(default = "default_contour")]
    pub contour_interval: Length,
    /// Every this many contours is drawn heavier and labelled.
    #[serde(default = "five")]
    pub major_every: u32,
    /// Continue flat ground to the horizon in 3D.
    #[serde(default)]
    pub extend_to_horizon: bool,
    #[serde(default = "grass")]
    pub color: Color,
    /// The reference point: the ground here is at `base_elevation`, and the
    /// surface slopes from it to the elevation data (data right beside it win).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Point2>,
}

fn default_base() -> Length {
    Length::from_inches(-8.0)
}
fn two() -> u32 {
    2
}
fn five() -> u32 {
    5
}
fn default_skirt() -> Length {
    Length::from_feet(2.0)
}
fn default_contour() -> Length {
    Length::from_feet(1.0)
}
fn grass() -> Color {
    Color::rgb(140, 165, 110)
}

impl Terrain {
    /// A terrain with typical settings.
    pub fn new(floor: Id, layer: Id, perimeter: Vec<Point2>) -> Self {
        Terrain {
            id: Id(0),
            floor,
            layer,
            perimeter,
            base_elevation: default_base(),
            smoothing: two(),
            skirt: default_skirt(),
            contour_interval: default_contour(),
            major_every: five(),
            extend_to_horizon: false,
            color: grass(),
            reference: None,
        }
    }
}

/// A piece of elevation data shaping the terrain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ElevationShape {
    /// A spot elevation.
    Point { at: Point2, elevation: Length },
    /// A contour-like line at one height (smoothed when `smooth`).
    Line {
        points: Vec<Point2>,
        elevation: Length,
        #[serde(default)]
        smooth: bool,
    },
    /// A flat area at one height (a pad, a terrace).
    Region { points: Vec<Point2>, elevation: Length },
    /// A rounded hill added to the ground.
    Hill { center: Point2, radius: Length, height: Length },
    /// A rounded hollow cut into the ground.
    Valley { center: Point2, radius: Length, depth: Length },
    /// A terrain break: the ground follows this line, its height running
    /// straight between the heights at its points (a ditch, a curb line).
    BreakLine { points: Vec<Point2>, elevations: Vec<Length> },
    /// Where the ground meets the building: the terrain here is `below`
    /// under the lowest storey's subfloor, and follows the building up or down.
    Subfloor { at: Point2, below: Length },
}

impl ElevationShape {
    pub fn name(&self) -> &'static str {
        match self {
            ElevationShape::Point { .. } => "elevation point",
            ElevationShape::Line { .. } => "elevation line",
            ElevationShape::Region { .. } => "flat region",
            ElevationShape::Hill { .. } => "hill",
            ElevationShape::Valley { .. } => "valley",
            ElevationShape::BreakLine { .. } => "break line",
            ElevationShape::Subfloor { .. } => "subfloor marker",
        }
    }
}

/// Elevation data drawn on a floor plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ElevationData {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub shape: ElevationShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoadStyle {
    #[default]
    Road,
    Driveway,
    Sidewalk,
}

impl RoadStyle {
    pub fn name(self) -> &'static str {
        match self {
            RoadStyle::Road => "Road",
            RoadStyle::Driveway => "Driveway",
            RoadStyle::Sidewalk => "Sidewalk",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FenceStyle {
    #[default]
    Picket,
    Privacy,
    Rail,
}

impl FenceStyle {
    pub const ALL: [FenceStyle; 3] = [FenceStyle::Picket, FenceStyle::Privacy, FenceStyle::Rail];

    pub fn name(self) -> &'static str {
        match self {
            FenceStyle::Picket => "Picket",
            FenceStyle::Privacy => "Privacy",
            FenceStyle::Rail => "Split rail",
        }
    }
}

/// A gate in a fence, `at` along the fence from its start.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Gate {
    pub at: Length,
    pub width: Length,
    /// The gate's own height above the ground; `None` is as tall as the fence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<Length>,
    #[serde(default)]
    pub style: GateStyle,
    /// Gap between the ground and the gate's bottom.
    #[serde(default = "gate_clearance")]
    pub clearance: Length,
}

fn gate_clearance() -> Length {
    Length::from_inches(3.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateStyle {
    /// Solid boards.
    #[default]
    Panel,
    /// Spaced pickets between two rails.
    Picket,
    /// Solid boards with a top that arches up in the middle.
    Arched,
}

impl GateStyle {
    pub const ALL: [GateStyle; 3] = [GateStyle::Panel, GateStyle::Picket, GateStyle::Arched];

    pub fn name(self) -> &'static str {
        match self {
            GateStyle::Panel => "Panel",
            GateStyle::Picket => "Picket",
            GateStyle::Arched => "Arched",
        }
    }
}

impl Gate {
    pub fn new(at: Length, width: Length) -> Self {
        Gate { at, width, height: None, style: GateStyle::Panel, clearance: gate_clearance() }
    }
}

fn fence_post_size() -> Length {
    Length::from_inches(3.5)
}

fn fence_rail_size() -> Length {
    Length::from_inches(3.5)
}

/// What a site feature is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SiteKind {
    /// A retaining wall whose top is at `top`; it also breaks the terrain
    /// (the ground is held at `top` along it).
    RetainingWall { top: Length, thickness: Length },
    /// A road, driveway or sidewalk lying on the ground.
    Road {
        style: RoadStyle,
        width: Length,
        thickness: Length,
        #[serde(default)]
        curbs: bool,
        /// A raised median down the middle this wide (0: none).
        #[serde(default)]
        median: Length,
        /// A round turnaround of this radius at the road's end (0: none).
        #[serde(default)]
        cul_de_sac: Length,
    },
    /// A concrete curb along a path.
    Curb { height: Length, width: Length },
    /// A fence whose posts follow the ground.
    Fence {
        style: FenceStyle,
        height: Length,
        post_spacing: Length,
        #[serde(default)]
        gates: Vec<Gate>,
        /// Posts are this square.
        #[serde(default = "fence_post_size")]
        post_size: Length,
        /// Rails are this deep (1 1/2" thick).
        #[serde(default = "fence_rail_size")]
        rail_size: Length,
    },
    /// A garden bed (closed outline): raised by `height`, or a flat mulch
    /// bed on the ground when 0.
    GardenBed { height: Length },
    /// Stones along a path.
    SteppingStones { spacing: Length, size: Length },
    /// A stream or creek bed.
    Stream { width: Length },
    /// A strip of small lights along the path, `height` above the floor.
    RopeLight {
        height: Length,
        lumens_per_foot: f64,
        #[serde(default = "warm")]
        kelvin: f64,
    },
    /// Grass blades over an area (closed outline).
    Grass {
        length: Length,
        /// Blades per square metre.
        density: f64,
        color: Color,
    },
}

fn warm() -> f64 {
    2700.0
}

impl SiteKind {
    pub fn name(&self) -> &'static str {
        match self {
            SiteKind::RetainingWall { .. } => "Retaining Wall",
            SiteKind::Road { style, .. } => style.name(),
            SiteKind::Fence { .. } => "Fence",
            SiteKind::GardenBed { .. } => "Garden Bed",
            SiteKind::SteppingStones { .. } => "Stepping Stones",
            SiteKind::Stream { .. } => "Stream",
            SiteKind::RopeLight { .. } => "Rope Light",
            SiteKind::Grass { .. } => "Grass",
            SiteKind::Curb { .. } => "Curb",
        }
    }

    /// Whether its points make a closed outline.
    pub fn closed(&self) -> bool {
        matches!(self, SiteKind::GardenBed { .. } | SiteKind::Grass { .. })
    }

    /// Typical settings.
    pub fn retaining_wall() -> Self {
        SiteKind::RetainingWall { top: Length::from_feet(2.0), thickness: Length::from_inches(8.0) }
    }
    pub fn road(style: RoadStyle) -> Self {
        let (w, t, curbs) = match style {
            RoadStyle::Road => (24.0, 6.0, true),
            RoadStyle::Driveway => (12.0, 4.0, false),
            RoadStyle::Sidewalk => (4.0, 4.0, false),
        };
        SiteKind::Road { style, width: Length::from_feet(w), thickness: Length::from_inches(t), curbs, median: Length::ZERO, cul_de_sac: Length::ZERO }
    }
    pub fn fence() -> Self {
        SiteKind::Fence {
            style: FenceStyle::Picket,
            height: Length::from_feet(4.0),
            post_spacing: Length::from_feet(8.0),
            gates: Vec::new(),
            post_size: fence_post_size(),
            rail_size: fence_rail_size(),
        }
    }
    pub fn garden_bed() -> Self {
        SiteKind::GardenBed { height: Length::from_inches(12.0) }
    }
    pub fn stepping_stones() -> Self {
        SiteKind::SteppingStones { spacing: Length::from_inches(24.0), size: Length::from_inches(16.0) }
    }
    pub fn stream() -> Self {
        SiteKind::Stream { width: Length::from_feet(3.0) }
    }
    pub fn rope_light() -> Self {
        SiteKind::RopeLight { height: Length::from_inches(84.0), lumens_per_foot: 60.0, kelvin: 2700.0 }
    }
    pub fn curb() -> Self {
        SiteKind::Curb { height: Length::from_inches(6.0), width: Length::from_inches(6.0) }
    }
    pub fn grass() -> Self {
        SiteKind::Grass { length: Length::from_inches(4.0), density: 400.0, color: Color::rgb(95, 145, 60) }
    }
}

/// A retaining wall, road, fence, garden bed, path or stream drawn along
/// points on a floor plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SiteFeature {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub points: Vec<Point2>,
    /// What it is. (Not `kind`: that tags every object.)
    pub feature: SiteKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlantForm {
    #[default]
    Tree,
    Conifer,
    Shrub,
    Grass,
}

impl PlantForm {
    pub const ALL: [PlantForm; 4] = [PlantForm::Tree, PlantForm::Conifer, PlantForm::Shrub, PlantForm::Grass];

    pub fn name(self) -> &'static str {
        match self {
            PlantForm::Tree => "Tree",
            PlantForm::Conifer => "Conifer",
            PlantForm::Shrub => "Shrub",
            PlantForm::Grass => "Grass / perennial",
        }
    }

    /// Typical mature height and spread.
    pub fn typical(self) -> (Length, Length) {
        let f = Length::from_feet;
        match self {
            PlantForm::Tree => (f(25.0), f(20.0)),
            PlantForm::Conifer => (f(30.0), f(12.0)),
            PlantForm::Shrub => (f(4.0), f(4.0)),
            PlantForm::Grass => (f(2.0), f(2.0)),
        }
    }
}

/// A tree, shrub or other plant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Plant {
    pub id: Id,
    pub floor: Id,
    pub layer: Id,
    pub at: Point2,
    /// Common or botanical name.
    pub name: String,
    pub form: PlantForm,
    /// Mature size.
    pub height: Length,
    pub spread: Length,
    /// Years to reach mature size (for growth views); 0 when unknown.
    #[serde(default)]
    pub years_to_mature: f64,
    /// Show it at this age in years (0 = mature).
    #[serde(default)]
    pub age: f64,
    #[serde(default = "leaf")]
    pub color: Color,
    /// A picture of the plant seen from above, shown in plan in place of
    /// the drawn symbol (absolute, or relative to the plan file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

fn leaf() -> Color {
    Color::rgb(80, 130, 60)
}

impl Plant {
    /// Height and spread at its shown age.
    pub fn size_now(&self) -> (Length, Length) {
        let k = if self.age > 0.0 && self.years_to_mature > 0.0 {
            (self.age / self.years_to_mature).clamp(0.1, 1.0)
        } else {
            1.0
        };
        (self.height * k, self.spread * k)
    }
}
