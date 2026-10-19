//! The HouseCraft document model.
//!
//! A [`Document`] is plain data: every object lives in one id-keyed map as an
//! [`Object`] variant. All edits from the GUI, CLI, MCP server and scripts go
//! through `housecraft-api` commands, which keep it consistent; the fields are
//! public so readers and file-format crates can build it directly.

mod annotation;
mod building;
mod cabinet;
mod display;
mod deck;
mod framing;
mod imported;
mod site;
mod solid;
mod material;
mod template;
mod trim;
mod validate;
mod views;

use std::collections::BTreeMap;
use std::fmt;

use housecraft_units::{DisplayUnit, Length};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use annotation::*;
pub use building::*;
pub use cabinet::*;
pub use display::*;
pub use deck::*;
pub use framing::*;
pub use imported::*;
pub use site::*;
pub use solid::*;
pub use material::*;
pub use trim::*;
pub use validate::Issue;
pub use views::*;

/// Document-scoped object identifier. Never reused within a document.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct Id(pub u64);

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Implemented by every object type stored in a [`Document`].
pub trait ObjectData: Sized + Into<Object> + 'static {
    const KIND: ObjectKind;
    fn from_ref(object: &Object) -> Option<&Self>;
    fn from_mut(object: &mut Object) -> Option<&mut Self>;
    fn id(&self) -> Id;
}

macro_rules! objects {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// Any object in a document, tagged by `kind`.
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        // Default sets are the big variant; there are few, so boxing buys nothing.
        #[allow(clippy::large_enum_variant)]
        pub enum Object {
            $($variant($variant)),*
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum ObjectKind {
            $($variant),*
        }

        impl ObjectKind {
            pub const ALL: &[ObjectKind] = &[$(ObjectKind::$variant),*];
            pub fn name(self) -> &'static str {
                match self {
                    $(ObjectKind::$variant => $name),*
                }
            }
        }

        impl Object {
            pub fn kind(&self) -> ObjectKind {
                match self {
                    $(Object::$variant(_) => ObjectKind::$variant),*
                }
            }
            pub fn id(&self) -> Id {
                match self {
                    $(Object::$variant(o) => o.id),*
                }
            }
            pub fn set_id(&mut self, id: Id) {
                match self {
                    $(Object::$variant(o) => o.id = id),*
                }
            }
        }

        $(
            impl From<$variant> for Object {
                fn from(o: $variant) -> Object {
                    Object::$variant(o)
                }
            }
            impl ObjectData for $variant {
                const KIND: ObjectKind = ObjectKind::$variant;
                fn from_ref(object: &Object) -> Option<&Self> {
                    match object {
                        Object::$variant(o) => Some(o),
                        #[allow(unreachable_patterns)]
                        _ => None,
                    }
                }
                fn from_mut(object: &mut Object) -> Option<&mut Self> {
                    match object {
                        Object::$variant(o) => Some(o),
                        #[allow(unreachable_patterns)]
                        _ => None,
                    }
                }
                fn id(&self) -> Id {
                    self.id
                }
            }
        )*
    };
}

objects! {
    Floor => "floor",
    WallType => "wall_type",
    Wall => "wall",
    Opening => "opening",
    RoomType => "room_type",
    Room => "room",
    Layer => "layer",
    LayerSet => "layer_set",
    TextStyle => "text_style",
    DefaultSet => "default_set",
    Cad => "cad",
    Dimension => "dimension",
    Text => "text",
    PlanView => "plan_view",
    Camera => "camera",
    RoofPlane => "roof_plane",
    Stair => "stair",
    Electrical => "electrical",
    Schedule => "schedule",
    SectionView => "section_view",
    Layout => "layout",
    LayoutBox => "layout_box",
    Callout => "callout",
    RevisionCloud => "revision_cloud",
    CadDetail => "cad_detail",
    Block => "block",
    BlockRef => "block_ref",
    Material => "material",
    MaterialAssignment => "material_assignment",
    Cabinet => "cabinet",
    Countertop => "countertop",
    Appliance => "appliance",
    Picture => "picture",
    SymbolDef => "symbol_def",
    Symbol => "symbol",
    Backsplash => "backsplash",
    MoldingProfile => "molding_profile",
    MoldingPolyline => "molding_polyline",
    MaterialRegion => "material_region",
    StylePalette => "style_palette",
    Solid => "solid",
    Terrain => "terrain",
    ElevationData => "elevation_data",
    SiteFeature => "site_feature",
    Plant => "plant",
    FramingMember => "framing_member",
    JoistDirection => "joist_direction",
    Deck => "deck",
    Dormer => "dormer",
    Walkthrough => "walkthrough",
    Annotation3d => "annotation3d",
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl ObjectKind {
    /// Kinds drawn on a floor that can be moved, copied, rotated and mirrored.
    pub fn is_placed(self) -> bool {
        matches!(
            self,
            ObjectKind::Wall
                | ObjectKind::Opening
                | ObjectKind::Cad
                | ObjectKind::Dimension
                | ObjectKind::Text
                | ObjectKind::RoofPlane
                | ObjectKind::Stair
                | ObjectKind::Electrical
                | ObjectKind::Schedule
                | ObjectKind::Callout
                | ObjectKind::RevisionCloud
                | ObjectKind::BlockRef
                | ObjectKind::Cabinet
                | ObjectKind::Countertop
                | ObjectKind::Appliance
                | ObjectKind::Picture
                | ObjectKind::Symbol
                | ObjectKind::Backsplash
                | ObjectKind::MoldingPolyline
                | ObjectKind::Solid
                | ObjectKind::Terrain
                | ObjectKind::ElevationData
                | ObjectKind::SiteFeature
                | ObjectKind::Plant
                | ObjectKind::FramingMember
                | ObjectKind::JoistDirection
                | ObjectKind::Deck
                | ObjectKind::Dormer
                | ObjectKind::MaterialRegion
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub settings: Settings,
    /// Next id to hand out; always greater than every id in the document.
    pub next_id: u64,
    pub objects: BTreeMap<Id, Object>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Settings {
    pub display_unit: DisplayUnit,
    /// The default set used by commands that do not name one.
    pub active_default_set: Id,
    /// Direction of true north in plan, degrees counter-clockwise from +x
    /// (90 = up the screen).
    #[serde(default = "north_up")]
    pub north: f64,
    /// Where the north pointer is drawn on the first floor's plan, if shown.
    #[serde(default)]
    pub north_pointer: Option<housecraft_units::Point2>,
    #[serde(default)]
    pub sun: Sun,
    /// Materials list unit prices, keyed "Category: Item".
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub prices: BTreeMap<String, f64>,
    /// Markup applied to the materials list total, percent.
    #[serde(default)]
    pub markup_percent: f64,
    /// Default sets used for drafting in each kind of 2D view.
    #[serde(default)]
    pub view_defaults: ViewDefaults,
    /// Object labels typed for one object, replacing its kind's template
    /// ("-" hides that object's label).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<Id, String>,
    /// The plan's own line styles (`LineStyle::Custom(index)`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_types: Vec<LineType>,
    /// How angles and directions are shown.
    #[serde(default)]
    pub angle_format: housecraft_units::AngleFormat,
    /// Code, description, manufacturer, comment and custom fields by object.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_info: BTreeMap<Id, InfoFields>,
    /// Named dimension defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimension_styles: Vec<DimensionStyle>,
    /// Client, designer and custom project fields (text macros).
    #[serde(default)]
    pub project: ProjectInfo,
    /// Revisions issued, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<Revision>,
    /// Text stamped across every layout page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark: Option<Watermark>,
    /// Materials list components set on objects (extra items, or ones
    /// replacing what the take-off counts for them).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: BTreeMap<Id, ObjectComponents>,
    /// User-defined text macros: `{name}` is replaced by the text (which may
    /// use other macros).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub macros: BTreeMap<String, String>,
    /// The dimension tiers a floor's automatic exterior dimensions were
    /// made with (they refresh the same way); the default set's otherwise.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub auto_dims: BTreeMap<Id, AutoDims>,
    /// Saved sets of automatic dimension tiers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_dim_presets: Vec<NamedAutoDims>,
}

/// An object's own materials list lines.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ObjectComponents {
    /// Leave out what the take-off would count for this object.
    pub replace: bool,
    pub items: Vec<Component>,
}

/// One materials list line for an object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Component {
    pub category: String,
    pub item: String,
    pub quantity: f64,
    /// "ea", "lin ft", "sq ft"…
    #[serde(default = "each")]
    pub unit: String,
    /// Unit price; the price list's when `None`.
    #[serde(default)]
    pub price: Option<f64>,
}

fn each() -> String {
    "ea".into()
}

/// Who the plan is for and who drew it. Each field is a text macro of the
/// same name (`{client}`, `{designer_phone}`); `custom` fields too.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ProjectInfo {
    pub client: String,
    pub client_address: String,
    pub client_phone: String,
    pub client_email: String,
    pub designer: String,
    pub company: String,
    pub designer_address: String,
    pub designer_phone: String,
    pub designer_email: String,
    pub license: String,
    pub custom: BTreeMap<String, String>,
}

impl ProjectInfo {
    /// The built-in fields as (macro name, label, value).
    pub fn fields(&self) -> [(&'static str, &'static str, &String); 10] {
        [
            ("client", "Client", &self.client),
            ("client_address", "Client Address", &self.client_address),
            ("client_phone", "Client Phone", &self.client_phone),
            ("client_email", "Client Email", &self.client_email),
            ("designer", "Designer", &self.designer),
            ("company", "Company", &self.company),
            ("designer_address", "Designer Address", &self.designer_address),
            ("designer_phone", "Designer Phone", &self.designer_phone),
            ("designer_email", "Designer Email", &self.designer_email),
            ("license", "License", &self.license),
        ]
    }

    /// The value of macro `name`, built-in or custom.
    pub fn get(&self, name: &str) -> Option<&String> {
        self.fields().into_iter().find(|f| f.0 == name).map(|f| f.2).or_else(|| self.custom.get(name))
    }
}

/// One issued revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Revision {
    pub number: u32,
    /// YYYY-MM-DD.
    pub date: String,
    pub description: String,
    #[serde(default)]
    pub by: String,
}

/// A watermark: large pale text across the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Watermark {
    pub text: String,
    /// Letter height on paper.
    #[serde(default = "watermark_height")]
    pub height: Length,
    /// Degrees counter-clockwise.
    #[serde(default = "watermark_angle")]
    pub angle: f64,
    /// 0 (invisible) to 1 (black).
    #[serde(default = "watermark_darkness")]
    pub darkness: f64,
}

fn watermark_height() -> Length {
    Length::from_mm(40.0)
}

fn watermark_angle() -> f64 {
    30.0
}

fn watermark_darkness() -> f64 {
    0.15
}

/// Camera default sets: the default set (dimension, text and layer
/// defaults) each kind of 2D view uses for drafting; `None` uses the active one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViewDefaults {
    #[serde(default)]
    pub elevation: Option<Id>,
    #[serde(default)]
    pub section: Option<Id>,
    #[serde(default)]
    pub detail: Option<Id>,
}

/// Kinds of 2D view with their own camera default set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    Elevation,
    Section,
    Detail,
}

impl ViewDefaults {
    pub fn get(&self, kind: ViewKind) -> Option<Id> {
        match kind {
            ViewKind::Elevation => self.elevation,
            ViewKind::Section => self.section,
            ViewKind::Detail => self.detail,
        }
    }

    pub fn set(&mut self, kind: ViewKind, set: Option<Id>) {
        match kind {
            ViewKind::Elevation => self.elevation = set,
            ViewKind::Section => self.section = set,
            ViewKind::Detail => self.detail = set,
        }
    }
}

fn north_up() -> f64 {
    90.0
}

/// Sun position for 3D lighting and shadows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Sun {
    /// Compass bearing to the sun, degrees clockwise from true north.
    pub azimuth: f64,
    /// Height above the horizon, degrees.
    pub altitude: f64,
    #[serde(default = "shadows_on")]
    pub shadows: bool,
}

fn shadows_on() -> bool {
    true
}

impl Default for Sun {
    fn default() -> Self {
        // Mid-morning, south-east: lights the front-left of a south-facing plan.
        Sun {
            azimuth: 135.0,
            altitude: 45.0,
            shadows: true,
        }
    }
}

impl Document {
    /// A document with no objects. Its `active_default_set` dangles until one
    /// is added, so it does not validate on its own.
    pub fn empty(name: impl Into<String>) -> Self {
        Document {
            name: name.into(),
            settings: Settings {
                display_unit: DisplayUnit::default(),
                active_default_set: Id(0),
                north: north_up(),
                north_pointer: None,
                sun: Sun::default(),
                prices: BTreeMap::new(),
                markup_percent: 0.0,
                view_defaults: ViewDefaults::default(),
                labels: BTreeMap::new(),
                line_types: Vec::new(),
                angle_format: Default::default(),
                object_info: BTreeMap::new(),
                dimension_styles: Vec::new(),
                project: ProjectInfo::default(),
                revisions: Vec::new(),
                watermark: None,
                components: BTreeMap::new(),
                macros: BTreeMap::new(),
                auto_dims: BTreeMap::new(),
                auto_dim_presets: Vec::new(),
            },
            next_id: 1,
            objects: BTreeMap::new(),
        }
    }

    /// A new plan with one floor, standard US wood-frame wall types, layers,
    /// room types, text styles, a default set, layer sets, a plan view and a camera.
    pub fn new(name: impl Into<String>) -> Self {
        template::new_document(name.into())
    }

    pub fn alloc_id(&mut self) -> Id {
        let id = Id(self.next_id);
        self.next_id += 1;
        id
    }

    /// Insert an object under a fresh id (its own `id` field is overwritten).
    pub fn insert(&mut self, object: impl Into<Object>) -> Id {
        let mut object = object.into();
        let id = self.alloc_id();
        object.set_id(id);
        self.objects.insert(id, object);
        id
    }

    pub fn object(&self, id: Id) -> Option<&Object> {
        self.objects.get(&id)
    }

    pub fn object_mut(&mut self, id: Id) -> Option<&mut Object> {
        self.objects.get_mut(&id)
    }

    pub fn kind_of(&self, id: Id) -> Option<ObjectKind> {
        self.object(id).map(Object::kind)
    }

    pub fn get<T: ObjectData>(&self, id: Id) -> Option<&T> {
        self.objects.get(&id).and_then(T::from_ref)
    }

    pub fn get_mut<T: ObjectData>(&mut self, id: Id) -> Option<&mut T> {
        self.objects.get_mut(&id).and_then(T::from_mut)
    }

    /// All objects of one type, in id order.
    pub fn iter<T: ObjectData>(&self) -> impl Iterator<Item = &T> {
        self.objects.values().filter_map(T::from_ref)
    }

    pub fn ids<T: ObjectData>(&self) -> Vec<Id> {
        self.iter::<T>().map(ObjectData::id).collect()
    }

    pub fn count<T: ObjectData>(&self) -> usize {
        self.iter::<T>().count()
    }

    pub fn remove(&mut self, id: Id) -> Option<Object> {
        self.objects.remove(&id)
    }

    /// Find an object of type `T` by exact name.
    pub fn find_named<T: ObjectData + Named>(&self, name: &str) -> Option<&T> {
        self.iter::<T>().find(|o| o.name() == name)
    }

    /// Floors ordered from lowest to highest elevation.
    pub fn floors_by_elevation(&self) -> Vec<&Floor> {
        let mut floors: Vec<_> = self.iter::<Floor>().collect();
        floors.sort_by(|a, b| {
            a.elevation
                .mm()
                .total_cmp(&b.elevation.mm())
                .then(a.id.cmp(&b.id))
        });
        floors
    }

    /// The floor directly below `floor`, if any.
    pub fn floor_below(&self, floor: Id) -> Option<&Floor> {
        let floors = self.floors_by_elevation();
        let i = floors.iter().position(|f| f.id == floor)?;
        i.checked_sub(1).map(|j| floors[j])
    }

    pub fn walls_on(&self, floor: Id) -> impl Iterator<Item = &Wall> {
        self.iter::<Wall>().filter(move |w| w.floor == floor)
    }

    pub fn openings_in(&self, wall: Id) -> impl Iterator<Item = &Opening> {
        self.iter::<Opening>().filter(move |o| o.wall == wall)
    }

    pub fn rooms_on(&self, floor: Id) -> impl Iterator<Item = &Room> {
        self.iter::<Room>().filter(move |r| r.floor == floor)
    }

    /// The floor an object is drawn on (placed objects and rooms).
    pub fn floor_of(&self, id: Id) -> Option<Id> {
        match self.object(id)? {
            Object::Wall(w) => Some(w.floor),
            Object::Opening(o) => self.get::<Wall>(o.wall).map(|w| w.floor),
            Object::Room(r) => Some(r.floor),
            Object::Cad(c) => Some(c.floor),
            Object::Dimension(d) => Some(d.floor),
            Object::Text(t) => Some(t.floor),
            Object::RoofPlane(r) => Some(r.floor),
            Object::Stair(s) => Some(s.floor),
            Object::Electrical(e) => Some(e.floor),
            Object::Schedule(s) => Some(s.floor),
            Object::SectionView(v) => Some(v.floor),
            Object::Callout(c) => Some(c.floor),
            Object::RevisionCloud(c) => Some(c.floor),
            Object::BlockRef(b) => Some(b.floor),
            Object::Cabinet(c) => Some(c.floor),
            Object::Countertop(c) => Some(c.floor),
            Object::Appliance(a) => Some(a.floor),
            Object::Picture(p) => Some(p.floor),
            Object::Symbol(s) => Some(s.floor),
            Object::Backsplash(b) => Some(b.floor),
            Object::MoldingPolyline(m) => Some(m.floor),
            Object::Solid(x) => Some(x.floor),
            Object::Terrain(t) => Some(t.floor),
            Object::ElevationData(e) => Some(e.floor),
            Object::SiteFeature(f) => Some(f.floor),
            Object::Plant(p) => Some(p.floor),
            Object::FramingMember(m) => Some(m.floor),
            Object::JoistDirection(j) => Some(j.floor),
            Object::Deck(d) => Some(d.floor),
            Object::Dormer(d) => Some(d.floor),
            Object::MaterialRegion(r) => Some(r.floor),
            _ => None,
        }
    }

    /// The layer an object is drawn on.
    pub fn layer_of(&self, id: Id) -> Option<Id> {
        match self.object(id)? {
            Object::Wall(w) => Some(w.layer),
            Object::Opening(o) => Some(o.layer),
            Object::Room(r) => Some(r.layer),
            Object::Cad(c) => Some(c.layer),
            Object::Dimension(d) => Some(d.layer),
            Object::Text(t) => Some(t.layer),
            Object::RoofPlane(r) => Some(r.layer),
            Object::Stair(s) => Some(s.layer),
            Object::Electrical(e) => Some(e.layer),
            Object::Schedule(s) => Some(s.layer),
            Object::SectionView(v) => Some(v.layer),
            Object::Callout(c) => Some(c.layer),
            Object::RevisionCloud(c) => Some(c.layer),
            Object::BlockRef(b) => Some(b.layer),
            Object::Cabinet(c) => Some(c.layer),
            Object::Countertop(c) => Some(c.layer),
            Object::Appliance(a) => Some(a.layer),
            Object::Picture(p) => Some(p.layer),
            Object::Symbol(s) => Some(s.layer),
            Object::Backsplash(b) => Some(b.layer),
            Object::MoldingPolyline(m) => Some(m.layer),
            Object::Solid(x) => Some(x.layer),
            Object::Terrain(t) => Some(t.layer),
            Object::ElevationData(e) => Some(e.layer),
            Object::SiteFeature(f) => Some(f.layer),
            Object::Plant(p) => Some(p.layer),
            Object::FramingMember(m) => Some(m.layer),
            Object::JoistDirection(j) => Some(j.layer),
            Object::Deck(d) => Some(d.layer),
            Object::Dormer(d) => Some(d.layer),
            Object::MaterialRegion(r) => Some(r.layer),
            _ => None,
        }
    }

    /// Switch label for each switch: "S", "S3" or "S4" (with "D" for dimmers),
    /// from how many switches share fixtures. Two switches on a fixture are
    /// both 3-way; with more, the first two (by id) are 3-way and the rest 4-way.
    pub fn switch_labels(&self) -> BTreeMap<Id, String> {
        let switches: Vec<&Electrical> = self
            .iter::<Electrical>()
            .filter(|e| e.electrical_kind.is_switch())
            .collect();
        // Union-find over switches that share a controlled fixture.
        let n = switches.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for i in 0..n {
            for j in i + 1..n {
                if switches[i]
                    .controls
                    .iter()
                    .any(|c| switches[j].controls.contains(c))
                {
                    let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                    parent[rj] = ri;
                }
            }
        }
        // Each switch's group root; switches come in id order.
        let groups: Vec<usize> = (0..n).map(|i| root(&mut parent, i)).collect();
        let mut labels = BTreeMap::new();
        for (i, s) in switches.iter().enumerate() {
            let size = groups.iter().filter(|g| **g == groups[i]).count();
            let pos = groups[..i].iter().filter(|g| **g == groups[i]).count();
            let mut label = match (size, pos) {
                (1, _) => "S".to_string(),
                (_, 0 | 1) => "S3".to_string(),
                _ => "S4".to_string(),
            };
            if s.electrical_kind == ElectricalKind::Dimmer {
                label.push('D');
            }
            labels.insert(s.id, label);
        }
        labels
    }
    /// The active default set. Panics only on a document that fails [`Document::validate`].
    /// Whether drafting objects (lines, dimensions, text...) can be drawn
    /// on `id`: a floor plan, an elevation or section view, or a CAD detail.
    pub fn is_sheet(&self, id: Id) -> bool {
        matches!(
            self.kind_of(id),
            Some(ObjectKind::Floor | ObjectKind::SectionView | ObjectKind::CadDetail)
        )
    }

    /// The kind of 2D view `sheet` is, if it is not a floor plan.
    pub fn view_kind(&self, sheet: Id) -> Option<ViewKind> {
        match self.object(sheet)? {
            Object::SectionView(v) => Some(match v.section_kind {
                SectionKind::Elevation => ViewKind::Elevation,
                SectionKind::Section => ViewKind::Section,
            }),
            Object::CadDetail(_) => Some(ViewKind::Detail),
            _ => None,
        }
    }

    /// The default set drafting on `sheet` uses: its view kind's camera
    /// default set, or else the active one.
    pub fn defaults_for(&self, sheet: Id) -> &DefaultSet {
        self.view_kind(sheet)
            .and_then(|k| self.settings.view_defaults.get(k))
            .and_then(|d| self.get::<DefaultSet>(d))
            .unwrap_or_else(|| self.active_defaults())
    }

    pub fn active_defaults(&self) -> &DefaultSet {
        self.get::<DefaultSet>(self.settings.active_default_set)
            .or_else(|| self.iter::<DefaultSet>().next())
            .expect("a valid document has a default set")
    }

    /// Height of a wall, falling back to its floor's ceiling height.
    pub fn wall_height(&self, wall: &Wall) -> Option<Length> {
        wall.height
            .or_else(|| self.get::<Floor>(wall.floor).map(|f| f.ceiling_height))
    }

    /// Effective ceiling height of a room: room override, then room type, then floor.
    pub fn room_ceiling_height(&self, room: &Room) -> Option<Length> {
        room.ceiling_height
            .or_else(|| {
                self.get::<RoomType>(room.room_type)
                    .and_then(|t| t.ceiling_height)
            })
            .or_else(|| self.get::<Floor>(room.floor).map(|f| f.ceiling_height))
    }

    /// The display name of a room: its own name, else its type's.
    pub fn room_name(&self, room: &Room) -> String {
        room.name.clone().unwrap_or_else(|| {
            self.get::<RoomType>(room.room_type)
                .map_or_else(|| "Room".into(), |t| t.name.clone())
        })
    }

    /// The floor directly above `floor`, if any.
    pub fn floor_above(&self, floor: Id) -> Option<&Floor> {
        let floors = self.floors_by_elevation();
        let i = floors.iter().position(|f| f.id == floor)?;
        floors.get(i + 1).copied()
    }

    /// Total rise of a stair: its own, or up to the finished floor above
    /// (or the ceiling when it is on the top floor).
    pub fn stair_rise(&self, stair: &Stair) -> Length {
        if let Some(r) = stair.rise {
            return r;
        }
        let Some(floor) = self.get::<Floor>(stair.floor) else {
            return Length::from_feet(9.0);
        };
        match self.floor_above(floor.id) {
            Some(above) => above.elevation - floor.elevation,
            None => floor.ceiling_height,
        }
    }

    /// Check referential integrity and invariants. A document with no issues
    /// is safe for every reader and generator.
    pub fn validate(&self) -> Vec<Issue> {
        validate::validate(self)
    }
}

/// Objects with a user-visible name.
pub trait Named {
    fn name(&self) -> &str;
}

macro_rules! named {
    ($($t:ty),*) => {
        $(impl Named for $t {
            fn name(&self) -> &str {
                &self.name
            }
        })*
    };
}
named!(
    Floor, WallType, RoomType, Layer, LayerSet, TextStyle, DefaultSet, PlanView, Camera, SectionView,
    CadDetail, Block, Layout, Material, SymbolDef, MoldingProfile, StylePalette, Walkthrough
);

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_units::Point2;

    #[test]
    fn new_document_is_valid() {
        let doc = Document::new("Test");
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
        assert_eq!(doc.count::<Floor>(), 1);
        let ext = doc
            .get::<WallType>(doc.active_defaults().exterior_wall_type)
            .unwrap();
        assert!((ext.thickness().inches() - 7.1875).abs() < 1e-9);
        assert!(doc.count::<Layer>() >= 6);
        assert!(doc.count::<PlanView>() >= 1);
    }

    #[test]
    fn validate_catches_dangling_refs() {
        let mut doc = Document::new("Test");
        let layer = doc.active_defaults().layers.walls;
        doc.insert(Wall {
            id: Id(0),
            floor: Id(999),
            start: Point2::from_mm(0.0, 0.0),
            end: Point2::from_mm(1000.0, 0.0),
            wall_type: Id(998),
            height: None,
            layer,
            footing: None,
            roof: Default::default(),
            renovation: Default::default(),
            shape: Default::default(),
        });
        assert_eq!(doc.validate().len(), 2, "{:?}", doc.validate());
    }

    #[test]
    fn validate_catches_openings_that_do_not_fit() {
        let mut doc = Document::new("Test");
        let d = doc.active_defaults().clone();
        let floor = doc.floors_by_elevation()[0].id;
        let wall = doc.insert(Wall {
            id: Id(0),
            floor,
            start: Point2::from_mm(0.0, 0.0),
            end: Point2::from_mm(1000.0, 0.0),
            wall_type: d.exterior_wall_type,
            height: None,
            layer: d.layers.walls,
            footing: None,
            roof: Default::default(),
            renovation: Default::default(),
            shape: Default::default(),
        });
        doc.insert(Opening {
            id: Id(0),
            wall,
            opening_kind: OpeningKind::Door(DoorStyle::Hinged),
            position: Length::from_mm(500.0),
            width: Length::from_mm(1200.0),
            height: Length::from_mm(2000.0),
            sill: Length::ZERO,
            hinge: Hinge::Left,
            swing_in: true,
            open_percent: 0.0,
            plan_swing: 90.0,
            renovation: Default::default(),
            layer: d.layers.doors,
            details: Default::default(),
        });
        assert_eq!(doc.validate().len(), 1, "{:?}", doc.validate());
    }

    #[test]
    fn round_trips_through_json() {
        let doc = Document::new("Test");
        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, back);
    }
}
