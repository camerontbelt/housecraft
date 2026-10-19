//! The library: reusable objects to place in any plan.
//!
//! Items come from a built-in catalogue (standard cabinet sizes,
//! appliances and fixtures, the starter materials and wall types) and from
//! the user's library folder, where each item is a JSON file under
//! `<library>/items/` (blocks saved by earlier versions under
//! `<library>/blocks/` are read too). A whole library can be exported to and
//! imported from a single `.hclib` bundle.
//!
//! [`Query`] parses searches such as `cabinet AND (base OR sink) NOT corner`,
//! `"white quartz"`, `kind:material category:stone` or `width>=30`.

use std::path::{Path, PathBuf};

use housecraft_model::*;
use housecraft_units::Length;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Error;

/// What a library item places.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryKind {
    /// CAD shapes ([`LibraryBlock`] in `data`).
    Block,
    /// A [`WallType`] (without its id).
    WallType,
    /// A [`Material`] (without its id).
    Material,
    /// A [`Cabinet`] template.
    Cabinet,
    /// An [`Appliance`] template.
    Appliance,
    /// Several plan objects kept together (a whole kitchen run): `data` is
    /// `{"objects": [...], "origin": [x, y]}`; placing puts `origin` at the
    /// click (older groups have no origin and are drawn from (0, 0)).
    Group,
    /// A 3D [`SymbolDef`] (without its id).
    Symbol,
    /// A [`MoldingProfile`] (without its id).
    Molding,
    /// A door or window: an [`Opening`] without its wall, place or id.
    Opening,
    /// A [`HatchPattern`] for fills.
    HatchPattern,
    /// A [`FillStyle`] (pattern, colour, spacing, angle) to apply to shapes.
    FillStyle,
}

impl LibraryKind {
    pub fn name(self) -> &'static str {
        match self {
            LibraryKind::Block => "block",
            LibraryKind::WallType => "wall type",
            LibraryKind::Material => "material",
            LibraryKind::Cabinet => "cabinet",
            LibraryKind::Appliance => "appliance",
            LibraryKind::Group => "group",
            LibraryKind::Symbol => "symbol",
            LibraryKind::Molding => "molding",
            LibraryKind::Opening => "door or window",
            LibraryKind::HatchPattern => "hatch pattern",
            LibraryKind::FillStyle => "fill style",
        }
    }

    fn folder(self) -> &'static str {
        match self {
            LibraryKind::Block => "blocks",
            LibraryKind::WallType => "wall_types",
            LibraryKind::Material => "materials",
            LibraryKind::Cabinet => "cabinets",
            LibraryKind::Appliance => "appliances",
            LibraryKind::Group => "groups",
            LibraryKind::Symbol => "symbols",
            LibraryKind::Molding => "moldings",
            LibraryKind::Opening => "openings",
            LibraryKind::HatchPattern => "hatch_patterns",
            LibraryKind::FillStyle => "fill_styles",
        }
    }
}

/// Where an item came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Source {
    #[default]
    BuiltIn,
    /// The user's library, in this file.
    User(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryItem {
    pub name: String,
    /// Folder path in the browser, `/`-separated, e.g. "Cabinets/Base".
    pub category: String,
    pub kind: LibraryKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The object template (see [`LibraryKind`]).
    pub data: Value,
    #[serde(skip)]
    pub source: Source,
}

impl LibraryItem {
    pub fn is_user(&self) -> bool {
        matches!(self.source, Source::User(_))
    }

    /// A numeric attribute for searches, in inches for lengths.
    pub fn attribute(&self, key: &str) -> Option<f64> {
        let v = self.data.get(key)?;
        let len: Length = serde_json::from_value(v.clone()).ok()?;
        Some(len.inches())
    }
}

/// A block saved in the library: shapes relative to its insertion point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryBlock {
    pub name: String,
    pub shapes: Vec<DetailShape>,
}

impl LibraryBlock {
    pub fn item(&self) -> LibraryItem {
        LibraryItem {
            name: self.name.clone(),
            category: "CAD Blocks".into(),
            kind: LibraryKind::Block,
            tags: Vec::new(),
            data: serde_json::to_value(self).expect("serialisable"),
            source: Source::BuiltIn,
        }
    }
}

/// The library folder: `$HOUSECRAFT_LIBRARY`, or `HouseCraft/Library` under
/// the user's application-data folder (`%APPDATA%`, `$XDG_DATA_HOME` or
/// `~/.local/share`; `~/Library/Application Support` on macOS).
pub fn library_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("HOUSECRAFT_LIBRARY") {
        return Some(PathBuf::from(p));
    }
    Some(app_data_dir()?.join("Library"))
}

/// Plan templates: `HouseCraft/Templates` in the application-data folder.
pub fn templates_dir() -> Option<PathBuf> {
    Some(app_data_dir()?.join("Templates"))
}

/// The master price list shared by every plan ("Category: Item" → price).
pub fn master_prices_path() -> Option<PathBuf> {
    Some(app_data_dir()?.join("master-prices.json"))
}

/// Save `prices` as the master price list, merged over what it held.
pub fn save_master_prices(prices: &std::collections::BTreeMap<String, f64>) -> std::io::Result<PathBuf> {
    let path = master_prices_path().ok_or_else(|| std::io::Error::other("no application-data folder"))?;
    let mut all = load_master_prices().unwrap_or_default();
    all.extend(prices.iter().map(|(k, v)| (k.clone(), *v)));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&all).map_err(std::io::Error::other)?)?;
    Ok(path)
}

/// The master price list (empty when none was saved).
pub fn load_master_prices() -> std::io::Result<std::collections::BTreeMap<String, f64>> {
    let Some(path) = master_prices_path().filter(|p| p.exists()) else {
        return Ok(Default::default());
    };
    serde_json::from_str(&std::fs::read_to_string(path)?).map_err(std::io::Error::other)
}

/// HouseCraft's folder under the user's application-data folder.
pub fn app_data_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join("HouseCraft"))
}

/// A file name for `name` with characters that are unsafe in paths replaced.
fn file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' })
        .collect();
    let stem = stem.trim().trim_start_matches('.').to_string();
    if stem.is_empty() { "item".into() } else { stem }
}

fn corrupt(e: impl std::fmt::Display) -> Error {
    Error::Corrupt(e.to_string())
}

/// Save `item` into the user library in `dir`, replacing one of the same
/// kind and name.
pub fn save_item(dir: &Path, item: &LibraryItem) -> Result<PathBuf, Error> {
    let folder = dir.join("items").join(item.kind.folder());
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(format!("{}.json", file_stem(&item.name)));
    std::fs::write(&path, serde_json::to_vec_pretty(item).map_err(corrupt)?)?;
    Ok(path)
}

/// Remove a user item.
pub fn delete_item(item: &LibraryItem) -> Result<(), Error> {
    match &item.source {
        Source::User(path) => Ok(std::fs::remove_file(path)?),
        Source::BuiltIn => Err(Error::Invalid("built-in items can't be deleted".into())),
    }
}

/// Save a block into `dir`'s library.
pub fn save_block(dir: &Path, block: &LibraryBlock) -> Result<PathBuf, Error> {
    save_item(dir, &block.item())
}

/// Every item in the user library in `dir`; unreadable files are skipped.
pub fn user_items(dir: &Path) -> Vec<LibraryItem> {
    let mut out = Vec::new();
    let mut read_folder = |folder: PathBuf, legacy_block: bool| {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            return;
        };
        for path in entries.flatten().map(|e| e.path()) {
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let item = if legacy_block {
                serde_json::from_slice::<LibraryBlock>(&bytes).ok().map(|b| b.item())
            } else {
                serde_json::from_slice::<LibraryItem>(&bytes).ok()
            };
            if let Some(mut item) = item {
                item.source = Source::User(path);
                out.push(item);
            }
        }
    };
    read_folder(dir.join("blocks"), true);
    if let Ok(kinds) = std::fs::read_dir(dir.join("items")) {
        for k in kinds.flatten() {
            read_folder(k.path(), false);
        }
    }
    out.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));
    out
}

/// The blocks saved in `dir`, by name (for older callers).
pub fn list_blocks(dir: &Path) -> Vec<(PathBuf, LibraryBlock)> {
    user_items(dir)
        .into_iter()
        .filter(|i| i.kind == LibraryKind::Block)
        .filter_map(|i| {
            let Source::User(path) = i.source else {
                return None;
            };
            Some((path, serde_json::from_value(i.data).ok()?))
        })
        .collect()
}

pub fn load_block(path: &Path) -> Result<LibraryBlock, Error> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice::<LibraryBlock>(&bytes)
        .or_else(|_| {
            let item: LibraryItem = serde_json::from_slice(&bytes)?;
            serde_json::from_value(item.data)
        })
        .map_err(corrupt)
}

/// Write `items` to a bundle file.
pub fn export_bundle(path: &Path, items: &[LibraryItem]) -> Result<(), Error> {
    let bundle = json!({ "format": "housecraft-library", "version": 1, "items": items });
    std::fs::write(path, serde_json::to_vec_pretty(&bundle).map_err(corrupt)?)?;
    Ok(())
}

/// Read a bundle file and add its items to the user library in `dir`.
pub fn import_bundle(dir: &Path, path: &Path) -> Result<usize, Error> {
    let v: Value = serde_json::from_slice(&std::fs::read(path)?).map_err(corrupt)?;
    if v.get("format").and_then(Value::as_str) != Some("housecraft-library") {
        return Err(Error::NotHouseCraft("not a HouseCraft library bundle".into()));
    }
    let items: Vec<LibraryItem> =
        serde_json::from_value(v.get("items").cloned().unwrap_or(Value::Null)).map_err(corrupt)?;
    for item in &items {
        save_item(dir, item)?;
    }
    Ok(items.len())
}

/// A plan's hatch pattern as a library item.
pub fn hatch_item(p: &HatchPattern) -> LibraryItem {
    LibraryItem {
        name: p.name.clone(),
        category: "My Library/Hatch Patterns".into(),
        kind: LibraryKind::HatchPattern,
        tags: vec!["hatch".into(), "pattern".into()],
        data: serde_json::to_value(p).expect("serialisable"),
        source: Source::BuiltIn,
    }
}

/// A fill style as a library item.
pub fn fill_item(name: &str, f: &FillStyle) -> LibraryItem {
    LibraryItem {
        name: name.into(),
        category: "My Library/Fill Styles".into(),
        kind: LibraryKind::FillStyle,
        tags: vec!["fill".into()],
        data: serde_json::to_value(f).expect("serialisable"),
        source: Source::BuiltIn,
    }
}

/// Library folders kept as shortcuts at the top of the browser.
pub fn load_shortcuts(dir: &Path) -> Vec<String> {
    std::fs::read(dir.join("shortcuts.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_shortcuts(dir: &Path, folders: &[String]) -> Result<(), Error> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("shortcuts.json"), serde_json::to_vec_pretty(folders).map_err(corrupt)?)?;
    Ok(())
}

/// Built-in hatch patterns and fill styles.
fn builtin_fills() -> Vec<LibraryItem> {
    let line = |angle: f64, spacing: f64, offset: f64, dash: &[f64], stagger: f64| HatchLine { angle, spacing, offset, dash: dash.to_vec(), stagger };
    let mut out: Vec<LibraryItem> = [
        ("Running Bond Brick", vec![line(0.0, 1.0, 0.0, &[], 0.0), line(90.0, 2.0, 0.0, &[1.0, 1.0], 1.0)]),
        ("Concrete Block", vec![line(0.0, 1.0, 0.0, &[], 0.0), line(90.0, 2.0, 0.0, &[1.0, 1.0], 1.0)]),
        ("Square Tile", vec![line(0.0, 1.0, 0.0, &[], 0.0), line(90.0, 1.0, 0.0, &[], 0.0)]),
        ("Diagonal Tile", vec![line(45.0, 1.0, 0.0, &[], 0.0), line(135.0, 1.0, 0.0, &[], 0.0)]),
        ("Herringbone", vec![line(45.0, 1.0, 0.0, &[2.0, 2.0], 2.0), line(135.0, 1.0, 0.0, &[2.0, 2.0], 0.0)]),
        ("Earth", vec![line(45.0, 1.0, 0.0, &[], 0.0), line(45.0, 1.0, 0.25, &[], 0.0), line(45.0, 1.0, 0.5, &[], 0.0), line(135.0, 3.0, 0.0, &[1.0, 2.0], 0.0)]),
        ("Insulation Batt", vec![line(60.0, 1.0, 0.0, &[0.5, 0.5], 0.0), line(120.0, 1.0, 0.0, &[0.5, 0.5], 0.5)]),
        ("Wood Grain", vec![line(0.0, 0.4, 0.0, &[3.0, 0.4, 1.0, 0.3], 1.5)]),
    ]
    .into_iter()
    .map(|(name, lines)| {
        let mut i = hatch_item(&HatchPattern { name: name.into(), lines });
        i.category = "Fills/Hatch Patterns".into();
        i
    })
    .collect();
    let fill = |pattern, spacing: f64, angle: f64, color: Option<Color>| FillStyle { pattern, color, spacing: Length::from_inches(spacing), angle, opacity: 1.0 };
    for (name, f) in [
        ("Concrete (dots)", fill(FillPattern::Dots, 3.0, 0.0, None)),
        ("Section Cut (hatch)", fill(FillPattern::Hatch, 3.0, 45.0, None)),
        ("Masonry (crosshatch)", fill(FillPattern::Crosshatch, 4.0, 45.0, None)),
        ("Lawn (green)", fill(FillPattern::Solid, 6.0, 0.0, Some(Color::rgb(150, 190, 120)))),
        ("Water (blue)", fill(FillPattern::Solid, 6.0, 0.0, Some(Color::rgb(140, 180, 220)))),
    ] {
        let mut i = fill_item(name, &f);
        i.category = "Fills/Fill Styles".into();
        out.push(i);
    }
    out
}

/// The built-in catalogue.
pub fn builtin() -> Vec<LibraryItem> {
    let mut out = builtin_fills();
    let item = |name: String, category: &str, kind: LibraryKind, tags: &[&str], data: Value| {
        LibraryItem {
            name,
            category: category.into(),
            kind,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            data,
            source: Source::BuiltIn,
        }
    };
    // Materials and wall types from a fresh plan.
    let doc = Document::new("library");
    for m in doc.iter::<Material>() {
        let category = format!(
            "Materials/{}",
            if m.category.is_empty() { "Other" } else { &m.category }
        );
        out.push(item(
            m.name.clone(),
            &category,
            LibraryKind::Material,
            &[m.class.name()],
            strip_id(m),
        ));
    }
    for w in doc.iter::<WallType>() {
        out.push(item(w.name.clone(), "Walls", LibraryKind::WallType, &[], strip_id(w)));
    }
    // General notes.
    for (name, callout) in [
        ("Verify All Dimensions in Field", false),
        ("Do Not Scale Drawings", false),
        ("Match Existing", true),
        ("See Detail", true),
        ("Provide Blocking", true),
    ] {
        let o = if callout {
            Object::Callout(Callout {
                id: Id(1),
                floor: Id(1),
                layer: Id(1),
                text: name.into(),
                at: housecraft_units::Point2::default(),
                target: Some(housecraft_units::Point2(Length::from_inches(-24.0), Length::from_inches(-12.0))),
                marker: None,
                height: None,
                link: None,
                    note: false,
            })
        } else {
            Object::Text(Text {
                id: Id(1),
                floor: Id(1),
                layer: Id(1),
                position: housecraft_units::Point2::default(),
                text: name.into(),
                style: None,
                rotation: 0.0,
                justify: Default::default(),
                height: None,
                bold: None,
                color: None,
                leaders: Vec::new(),
                border: false,
                wrap: None,
            })
        };
        out.push(item(
            name.into(),
            "Notes",
            LibraryKind::Group,
            &["note"],
            json!({ "objects": [o], "origin": [0, 0] }),
        ));
    }
    // Standard cabinet sizes.
    let d = CabinetDefaults::default();
    let i = Length::from_inches;
    let cab = |kind: CabinetKind, width: f64, depth: Length, height: Length, face: Vec<FaceRow>, sink: bool| {
        let top = (kind == CabinetKind::Base).then_some(CabinetTop {
            thickness: d.countertop_thickness,
            overhang: d.countertop_overhang,
            edge: d.edge,
            backsplash: d.backsplash,
            corner: CornerTreatment::Square,
            corner_left: false,
            corner_right: false,
            waterfall_left: false,
            waterfall_right: false,
        });
        let toe = if kind == CabinetKind::Wall {
            Value::Null
        } else {
            json!([d.toe_kick_height, d.toe_kick_depth])
        };
        json!({
            "cabinet_kind": kind,
            "width": i(width),
            "depth": depth,
            "height": height,
            "elevation": if kind == CabinetKind::Wall { d.wall_elevation } else { Length::ZERO },
            "construction": d.construction,
            "shape": CabinetShape::Standard,
            "face": face,
            "door_style": d.door_style,
            "hardware": d.hardware,
            "shelves": if kind == CabinetKind::Base { 1 } else { 2 },
            "toe_kick": toe,
            "countertop": top,
            "sink": sink,
        })
    };
    for w in [9.0, 12.0, 15.0, 18.0, 21.0, 24.0, 27.0, 30.0, 33.0, 36.0, 42.0, 48.0] {
        let face = d.face(CabinetKind::Base, i(w), false);
        out.push(item(
            format!("Base Cabinet {w}\""),
            "Cabinets/Base",
            LibraryKind::Cabinet,
            &["base", "door", "drawer"],
            cab(CabinetKind::Base, w, d.base_depth, d.base_height, face, false),
        ));
    }
    for w in [30.0, 33.0, 36.0] {
        let face = d.face(CabinetKind::Base, i(w), true);
        out.push(item(
            format!("Sink Base {w}\""),
            "Cabinets/Base",
            LibraryKind::Cabinet,
            &["base", "sink"],
            cab(CabinetKind::Base, w, d.base_depth, d.base_height, face, true),
        ));
    }
    for w in [12.0, 15.0, 18.0, 24.0, 30.0] {
        let drawers = vec![
            FaceRow { height: Some(i(6.0)), items: vec![FaceItem::Drawer], shelves: None },
            FaceRow { height: None, items: vec![FaceItem::Drawer], shelves: None },
            FaceRow { height: None, items: vec![FaceItem::Drawer], shelves: None },
        ];
        out.push(item(
            format!("Drawer Base {w}\""),
            "Cabinets/Base",
            LibraryKind::Cabinet,
            &["base", "drawer"],
            cab(CabinetKind::Base, w, d.base_depth, d.base_height, drawers, false),
        ));
    }
    for h in [30.0, 36.0, 42.0] {
        for w in [12.0, 15.0, 18.0, 24.0, 30.0, 36.0] {
            let face = d.face(CabinetKind::Wall, i(w), false);
            out.push(item(
                format!("Wall Cabinet {w}\" x {h}\""),
                "Cabinets/Wall",
                LibraryKind::Cabinet,
                &["wall", "upper", "door"],
                cab(CabinetKind::Wall, w, d.wall_depth, i(h), face, false),
            ));
        }
    }
    for (w, h) in [(18.0, 84.0), (24.0, 84.0), (24.0, 96.0), (30.0, 84.0)] {
        let face = d.face(CabinetKind::Full, i(w), false);
        out.push(item(
            format!("Tall Pantry {w}\" x {h}\""),
            "Cabinets/Tall",
            LibraryKind::Cabinet,
            &["full", "tall", "pantry"],
            cab(CabinetKind::Full, w, d.full_depth, i(h), face, false),
        ));
    }
    // Appliances and fixtures at typical sizes.
    for k in ApplianceKind::ALL {
        let (w, dp, h, e) = k.typical();
        let category = match k {
            ApplianceKind::Toilet
            | ApplianceKind::Bathtub
            | ApplianceKind::Shower
            | ApplianceKind::VanitySink
            | ApplianceKind::Pedestal => "Bath",
            ApplianceKind::Washer | ApplianceKind::Dryer | ApplianceKind::WaterHeater => "Utility",
            _ => "Kitchen Appliances",
        };
        out.push(item(
            k.name().to_string(),
            category,
            LibraryKind::Appliance,
            &[],
            json!({
                "appliance_kind": k,
                "width": i(w),
                "depth": i(dp),
                "height": i(h),
                "elevation": i(e),
            }),
        ));
    }
    out
}

/// An object as JSON without its id (the plan assigns one on insert).
fn strip_id(o: &impl Serialize) -> Value {
    let mut v = serde_json::to_value(o).expect("serialisable");
    if let Some(m) = v.as_object_mut() {
        m.remove("id");
    }
    v
}

/// A library item made from a plan object (for "Add to Library").
pub fn item_from_object(doc: &Document, id: Id) -> Option<LibraryItem> {
    // Fields that place an object rather than describe it.
    let template = |mut v: Value| {
        if let Some(m) = v.as_object_mut() {
            for k in [
                "id", "floor", "layer", "position", "facing", "renovation", "filler_left", "filler_right",
            ] {
                m.remove(k);
            }
        }
        v
    };
    let (name, category, kind, data) = match doc.object(id)? {
        Object::Cabinet(c) => (
            format!("{} {}\"", c.cabinet_kind.name(), c.width.inches().round()),
            "My Library/Cabinets",
            LibraryKind::Cabinet,
            template(serde_json::to_value(c).ok()?),
        ),
        Object::Appliance(a) => (
            a.appliance_kind.name().to_string(),
            "My Library/Appliances",
            LibraryKind::Appliance,
            template(serde_json::to_value(a).ok()?),
        ),
        Object::Material(m) => (
            m.name.clone(),
            "My Library/Materials",
            LibraryKind::Material,
            strip_id(m),
        ),
        Object::WallType(w) => (
            w.name.clone(),
            "My Library/Walls",
            LibraryKind::WallType,
            strip_id(w),
        ),
        Object::Symbol(s) => {
            let def = doc.get::<SymbolDef>(s.def)?;
            return Some(symbol_item(def, "My Library/Symbols"));
        }
        Object::SymbolDef(def) => return Some(symbol_item(def, "My Library/Symbols")),
        Object::Stair(s) => {
            let mut item = group_from_objects(doc, &[s.id])?;
            item.name = match s.shape {
                StairShape::Straight => "Straight Stair",
                StairShape::L { .. } => "L-shaped Stair",
                StairShape::U { .. } => "U-shaped Stair",
                StairShape::Curved { .. } => "Curved Stair",
            }
            .into();
            item.category = "My Library/Stairs".into();
            return Some(item);
        }
        Object::Solid(_) => {
            let mut item = group_from_objects(doc, &[id])?;
            item.name = "3D Solid".into();
            item.category = "My Library/Solids".into();
            return Some(item);
        }
        Object::Text(t) => {
            let mut item = group_from_objects(doc, &[id])?;
            item.name = t.text.lines().next().unwrap_or("Text").chars().take(40).collect();
            item.category = "My Library/Notes".into();
            return Some(item);
        }
        Object::Callout(c) => {
            let mut item = group_from_objects(doc, &[id])?;
            item.name = c.text.lines().next().unwrap_or("Callout").chars().take(40).collect();
            item.category = "My Library/Notes".into();
            return Some(item);
        }
        Object::Schedule(s) => {
            let mut item = group_from_objects(doc, &[id])?;
            item.name = format!("{} Schedule", s.schedule_kind.name());
            item.category = "My Library/Schedules".into();
            return Some(item);
        }
        Object::MoldingProfile(m) => (
            m.name.clone(),
            "My Library/Moldings",
            LibraryKind::Molding,
            strip_id(m),
        ),
        Object::Opening(o) => {
            let mut v = serde_json::to_value(o).ok()?;
            if let Some(m) = v.as_object_mut() {
                for k in ["id", "wall", "position", "layer", "renovation"] {
                    m.remove(k);
                }
            }
            let what = match o.opening_kind {
                OpeningKind::Door(s) => format!("{} Door", s.name()),
                OpeningKind::Window(s) => format!("{} Window", s.name()),
            };
            (
                format!(
                    "{what} {}\" x {}\"",
                    o.width.inches().round(),
                    o.height.inches().round()
                ),
                if o.opening_kind.is_door() { "My Library/Doors" } else { "My Library/Windows" },
                LibraryKind::Opening,
                v,
            )
        }
        Object::BlockRef(b) => {
            let block = doc.get::<Block>(b.block)?;
            let shapes: Vec<DetailShape> = doc
                .iter::<Cad>()
                .filter(|c| c.floor == block.id)
                .map(|c| DetailShape {
                    shape: c.shape.clone(),
                    color: c.color,
                    fill: c.fill,
                })
                .collect();
            let lb = LibraryBlock {
                name: block.name.clone(),
                shapes,
            };
            (
                lb.name.clone(),
                "My Library/CAD Blocks",
                LibraryKind::Block,
                serde_json::to_value(lb).ok()?,
            )
        }
        _ => return None,
    };
    Some(LibraryItem {
        name,
        category: category.into(),
        kind,
        tags: Vec::new(),
        data,
        source: Source::BuiltIn,
    })
}

/// Where an object sits, for finding a group's corner.
fn anchors(o: &Object) -> Vec<housecraft_units::Point2> {
    match o {
        Object::Cabinet(c) => vec![c.position],
        Object::Appliance(a) => vec![a.position],
        Object::Symbol(s) => vec![s.position],
        Object::Countertop(t) => t.points.clone(),
        Object::Backsplash(b) => b.points.clone(),
        Object::Stair(s) => vec![s.start],
        Object::Text(t) => vec![t.position],
        Object::Callout(c) => vec![c.at],
        Object::Schedule(s) => vec![s.position],
        Object::Solid(x) => match &x.shape {
            SolidShape::Extrude { points, .. } => points.clone(),
            SolidShape::Revolve { center, .. } => vec![*center],
        },
        _ => Vec::new(),
    }
}

/// A group item from plan objects (cabinets, appliances, countertops,
/// backsplashes, symbols, stairs and solids), with the 3D symbols they use.
/// Its `origin` is the group's lower-left corner; placing puts that corner
/// at the click.
pub fn group_from_objects(doc: &Document, ids: &[Id]) -> Option<LibraryItem> {
    let objects: Vec<Object> = ids
        .iter()
        .filter_map(|id| doc.object(*id))
        .filter(|o| {
            matches!(
                o.kind(),
                ObjectKind::Cabinet
                    | ObjectKind::Appliance
                    | ObjectKind::Countertop
                    | ObjectKind::Backsplash
                    | ObjectKind::Symbol
                    | ObjectKind::Stair
                    | ObjectKind::Solid
                    | ObjectKind::Text
                    | ObjectKind::Callout
                    | ObjectKind::Schedule
            )
        })
        .cloned()
        .map(|mut o| {
            // Text styles belong to the plan; the new plan's are used.
            if let Object::Text(t) = &mut o {
                t.style = None;
            }
            o
        })
        .collect();
    if objects.is_empty() {
        return None;
    }
    let all: Vec<_> = objects.iter().flat_map(anchors).collect();
    let min_x = all.iter().map(|p| p.x().mm()).fold(f64::MAX, f64::min);
    let min_y = all.iter().map(|p| p.y().mm()).fold(f64::MAX, f64::min);
    // The symbols they use travel with them.
    let mut defs: Vec<Id> = Vec::new();
    for o in &objects {
        let used = match o {
            Object::Symbol(s) => vec![Some(s.def)],
            Object::Stair(s) => vec![s.baluster, s.newel],
            _ => Vec::new(),
        };
        for d in used.into_iter().flatten() {
            if !defs.contains(&d) {
                defs.push(d);
            }
        }
    }
    let mut all_objects: Vec<Object> = defs
        .iter()
        .filter_map(|d| doc.object(*d))
        .cloned()
        .collect();
    let n = objects.len();
    all_objects.extend(objects);
    Some(LibraryItem {
        name: format!("Group of {n}"),
        category: "My Library/Groups".into(),
        kind: LibraryKind::Group,
        tags: Vec::new(),
        data: json!({
            "objects": all_objects,
            "origin": [Length::from_mm(min_x), Length::from_mm(min_y)],
        }),
        source: Source::BuiltIn,
    })
}

/// A search saved under a name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedFilter {
    pub name: String,
    pub query: String,
}

/// The saved searches in `dir`'s library.
pub fn load_filters(dir: &Path) -> Vec<SavedFilter> {
    std::fs::read(dir.join("filters.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_filters(dir: &Path, filters: &[SavedFilter]) -> Result<(), Error> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("filters.json"), serde_json::to_vec_pretty(filters).map_err(corrupt)?)?;
    Ok(())
}

/// The key a favourite is remembered by.
pub fn favorite_key(item: &LibraryItem) -> String {
    format!("{}|{}|{}", item.kind.name(), item.category, item.name)
}

/// The favourite items' keys in `dir`'s library.
pub fn load_favorites(dir: &Path) -> std::collections::BTreeSet<String> {
    std::fs::read(dir.join("favorites.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_favorites(dir: &Path, keys: &std::collections::BTreeSet<String>) -> Result<(), Error> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("favorites.json"), serde_json::to_vec_pretty(keys).map_err(corrupt)?)?;
    Ok(())
}

/// Replace `old` with `new` in the user library (an edited built-in item
/// is saved as a user item).
pub fn update_item(dir: &Path, old: &LibraryItem, new: &LibraryItem) -> Result<PathBuf, Error> {
    if let Source::User(path) = &old.source {
        std::fs::remove_file(path)?;
    }
    save_item(dir, new)
}

/// Plan settings that can be shared between plans: wall types, room types,
/// text styles, layers, materials and molding profiles.
pub const SETTINGS_KINDS: [ObjectKind; 6] = [
    ObjectKind::WallType,
    ObjectKind::RoomType,
    ObjectKind::TextStyle,
    ObjectKind::Layer,
    ObjectKind::Material,
    ObjectKind::MoldingProfile,
];

/// Plan-wide settings shared along with the objects: text macros, line
/// styles, hatch patterns, named dimension styles, auto-dimension presets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanSettings {
    pub macros: std::collections::BTreeMap<String, String>,
    pub line_types: Vec<LineType>,
    pub hatch_patterns: Vec<HatchPattern>,
    pub dimension_styles: Vec<DimensionStyle>,
    pub auto_dim_presets: Vec<NamedAutoDims>,
}

impl PlanSettings {
    pub fn of(doc: &Document) -> Self {
        let s = &doc.settings;
        PlanSettings {
            macros: s.macros.clone(),
            line_types: s.line_types.clone(),
            hatch_patterns: s.hatch_patterns.clone(),
            dimension_styles: s.dimension_styles.clone(),
            auto_dim_presets: s.auto_dim_presets.clone(),
        }
    }

    pub fn count(&self) -> usize {
        self.macros.len() + self.line_types.len() + self.hatch_patterns.len() + self.dimension_styles.len() + self.auto_dim_presets.len()
    }
}

/// The plan's shareable settings as a `.hcsettings` file (with the app's
/// hotkeys when given).
pub fn export_settings(doc: &Document, path: &Path) -> Result<usize, Error> {
    export_settings_with(doc, path, Value::Null)
}

pub fn export_settings_with(doc: &Document, path: &Path, hotkeys: Value) -> Result<usize, Error> {
    let objects: Vec<&Object> = doc
        .objects
        .values()
        .filter(|o| SETTINGS_KINDS.contains(&o.kind()))
        .collect();
    let plan = PlanSettings::of(doc);
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&json!({ "housecraft_settings": 1, "objects": objects, "plan": plan, "hotkeys": hotkeys })).map_err(corrupt)?,
    )?;
    Ok(objects.len() + plan.count())
}

/// The plan-wide settings and hotkeys in a `.hcsettings` file (empty for
/// older files).
pub fn read_plan_settings(path: &Path) -> Result<(PlanSettings, Value), Error> {
    let v: Value = serde_json::from_slice(&std::fs::read(path)?).map_err(corrupt)?;
    let plan = v.get("plan").cloned().map(serde_json::from_value).transpose().map_err(corrupt)?.unwrap_or_default();
    Ok((plan, v.get("hotkeys").cloned().unwrap_or(Value::Null)))
}

/// The settings in a `.hcsettings` file. Layers lose their text style
/// (it belongs to the other plan).
pub fn read_settings(path: &Path) -> Result<Vec<Object>, Error> {
    let v: Value = serde_json::from_slice(&std::fs::read(path)?).map_err(corrupt)?;
    let objects = v
        .get("objects")
        .cloned()
        .ok_or_else(|| Error::Corrupt("not a HouseCraft settings file".into()))?;
    let mut objects: Vec<Object> = serde_json::from_value(objects).map_err(corrupt)?;
    objects.retain(|o| SETTINGS_KINDS.contains(&o.kind()));
    for o in &mut objects {
        if let Object::Layer(l) = o {
            l.text_style = None;
        }
    }
    Ok(objects)
}

/// A library item for a 3D symbol.
pub fn symbol_item(def: &SymbolDef, category: &str) -> LibraryItem {
    LibraryItem {
        name: def.name.clone(),
        category: category.into(),
        kind: LibraryKind::Symbol,
        tags: Vec::new(),
        data: strip_id(def),
        source: Source::BuiltIn,
    }
}

/// Library items for every OBJ and STL model in a folder (not its
/// subfolders), under `category`; files that can't be read are listed
/// with their error.
pub fn import_models(
    dir: &Path,
    category: &str,
) -> (Vec<LibraryItem>, Vec<(PathBuf, String)>) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| matches!(e.to_string_lossy().to_lowercase().as_str(), "obj" | "stl"))
        })
        .collect();
    files.sort();
    let mut items = Vec::new();
    let mut failed = Vec::new();
    for f in files {
        match crate::symbols::read_model(&f, Default::default()) {
            Ok(def) => items.push(symbol_item(&def, category)),
            Err(e) => failed.push((f, e.to_string())),
        }
    }
    (items, failed)
}

/// A parsed library search.
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    /// Matches everything.
    All,
    /// A word or quoted phrase in the name, category, kind or tags.
    Text(String),
    /// `field:value` — field is name, category, kind or tag.
    Field(String, String),
    /// `attr>=n` etc. on a numeric attribute (lengths in inches).
    Compare(String, Cmp, f64),
    Not(Box<Query>),
    And(Vec<Query>),
    Or(Vec<Query>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Phrase(String),
    LParen,
    RParen,
    And,
    Or,
    Not,
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            '(' => {
                chars.next();
                out.push(Tok::LParen);
            }
            ')' => {
                chars.next();
                out.push(Tok::RParen);
            }
            '"' => {
                chars.next();
                let mut phrase = String::new();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some(c) => phrase.push(c),
                        None => return Err("a quote is not closed".into()),
                    }
                }
                out.push(Tok::Phrase(phrase));
            }
            // A leading minus excludes (a hyphen inside a word is kept).
            '-' => {
                chars.next();
                out.push(Tok::Not);
            }
            '|' => {
                chars.next();
                out.push(Tok::Or);
            }
            '&' => {
                chars.next();
                out.push(Tok::And);
            }
            _ => {
                let mut w = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || "()|&\"".contains(c) {
                        break;
                    }
                    w.push(c);
                    chars.next();
                }
                out.push(match w.as_str() {
                    "AND" => Tok::And,
                    "OR" => Tok::Or,
                    "NOT" => Tok::Not,
                    _ => Tok::Word(w),
                });
            }
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn or(&mut self) -> Result<Query, String> {
        let mut terms = vec![self.and()?];
        while self.peek() == Some(&Tok::Or) {
            self.at += 1;
            terms.push(self.and()?);
        }
        Ok(if terms.len() == 1 {
            terms.pop().expect("one")
        } else {
            Query::Or(terms)
        })
    }

    fn and(&mut self) -> Result<Query, String> {
        let mut terms = vec![self.unary()?];
        loop {
            match self.peek() {
                Some(Tok::And) => {
                    self.at += 1;
                    terms.push(self.unary()?);
                }
                // Words side by side mean AND.
                Some(Tok::Word(_) | Tok::Phrase(_) | Tok::LParen | Tok::Not) => {
                    terms.push(self.unary()?)
                }
                _ => break,
            }
        }
        Ok(if terms.len() == 1 {
            terms.pop().expect("one")
        } else {
            Query::And(terms)
        })
    }

    fn unary(&mut self) -> Result<Query, String> {
        match self.peek() {
            Some(Tok::Not) => {
                self.at += 1;
                Ok(Query::Not(Box::new(self.unary()?)))
            }
            Some(Tok::LParen) => {
                self.at += 1;
                let q = self.or()?;
                if self.peek() != Some(&Tok::RParen) {
                    return Err("a bracket is not closed".into());
                }
                self.at += 1;
                Ok(q)
            }
            Some(Tok::Phrase(p)) => {
                let q = Query::Text(p.to_lowercase());
                self.at += 1;
                Ok(q)
            }
            Some(Tok::Word(w)) => {
                let w = w.clone();
                self.at += 1;
                Ok(word(&w))
            }
            Some(t) => Err(format!("unexpected {t:?}")),
            None => Err("the search ends too early".into()),
        }
    }
}

fn word(w: &str) -> Query {
    for (op, cmp) in [
        (">=", Cmp::Ge),
        ("<=", Cmp::Le),
        (">", Cmp::Gt),
        ("<", Cmp::Lt),
        ("=", Cmp::Eq),
    ] {
        if let Some((attr, n)) = w.split_once(op)
            && !attr.is_empty()
            && let Ok(n) = n.trim_end_matches(['"', '\'']).parse::<f64>()
        {
            return Query::Compare(attr.to_lowercase(), cmp, n);
        }
    }
    if let Some((field, value)) = w.split_once(':')
        && matches!(field, "name" | "category" | "kind" | "tag")
    {
        return Query::Field(field.into(), value.to_lowercase());
    }
    Query::Text(w.to_lowercase())
}

impl Query {
    pub fn parse(s: &str) -> Result<Query, String> {
        let toks = lex(s)?;
        if toks.is_empty() {
            return Ok(Query::All);
        }
        let mut p = Parser { toks, at: 0 };
        let q = p.or()?;
        if p.at != p.toks.len() {
            return Err("a closing bracket has no opening one".into());
        }
        Ok(q)
    }

    pub fn matches(&self, item: &LibraryItem) -> bool {
        match self {
            Query::All => true,
            Query::Text(t) => format!(
                "{} {} {} {}",
                item.name,
                item.category,
                item.kind.name(),
                item.tags.join(" ")
            )
            .to_lowercase()
            .contains(t.as_str()),
            Query::Field(f, v) => match f.as_str() {
                "name" => item.name.to_lowercase().contains(v.as_str()),
                "category" => item.category.to_lowercase().contains(v.as_str()),
                "kind" => item.kind.name().replace(' ', "_").contains(v.as_str()),
                _ => item.tags.iter().any(|t| t.to_lowercase() == *v),
            },
            Query::Compare(attr, cmp, n) => item.attribute(attr).is_some_and(|x| match cmp {
                Cmp::Lt => x < *n,
                Cmp::Le => x <= *n + 1e-6,
                Cmp::Eq => (x - n).abs() < 1e-3,
                Cmp::Ge => x >= *n - 1e-6,
                Cmp::Gt => x > *n,
            }),
            Query::Not(q) => !q.matches(item),
            Query::And(qs) => qs.iter().all(|q| q.matches(item)),
            Query::Or(qs) => qs.iter().any(|q| q.matches(item)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_units::Point2;

    #[test]
    fn save_list_and_bundle() {
        let dir = std::env::temp_dir().join(format!("hc-lib-{}", std::process::id()));
        let block = LibraryBlock {
            name: "Toilet / WC".into(),
            shapes: vec![DetailShape {
                shape: Shape::Circle {
                    center: Point2(Length::ZERO, Length::ZERO),
                    radius: Length::from_inches(8.0),
                },
                color: None,
                fill: None,
            }],
        };
        let path = save_block(&dir, &block).unwrap();
        assert_eq!(path.file_name().unwrap(), "Toilet _ WC.json");
        let listed = list_blocks(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].1, block);
        assert_eq!(load_block(&path).unwrap(), block);
        let bundle = dir.join("x.hclib");
        export_bundle(&bundle, &user_items(&dir)).unwrap();
        let other = dir.join("other");
        assert_eq!(import_bundle(&other, &bundle).unwrap(), 1);
        assert_eq!(user_items(&other)[0].name, "Toilet / WC");
        delete_item(&user_items(&other)[0]).unwrap();
        assert!(user_items(&other).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn builtin_catalogue_has_everything() {
        let items = builtin();
        for kind in [
            LibraryKind::Material,
            LibraryKind::WallType,
            LibraryKind::Cabinet,
            LibraryKind::Appliance,
        ] {
            assert!(items.iter().any(|i| i.kind == kind), "{kind:?}");
        }
        // Cabinet templates deserialise once placement fields are added.
        let c = items.iter().find(|i| i.name == "Sink Base 36\"").unwrap();
        let mut v = c.data.clone();
        let m = v.as_object_mut().unwrap();
        m.insert("id".into(), json!(1));
        m.insert("floor".into(), json!(2));
        m.insert("layer".into(), json!(3));
        m.insert("position".into(), json!([0, 0]));
        m.insert("facing".into(), json!(90));
        let cab: Cabinet = serde_json::from_value(v).unwrap();
        assert!(cab.sink);
    }

    #[test]
    fn query_language() {
        let items = builtin();
        let find = |q: &str| -> Vec<String> {
            let q = Query::parse(q).unwrap();
            items
                .iter()
                .filter(|i| q.matches(i))
                .map(|i| i.name.clone())
                .collect()
        };
        let sinks = find("cabinet AND sink");
        assert_eq!(sinks.len(), 3, "{sinks:?}");
        let wide = find("kind:cabinet (sink OR pantry) width>=33");
        assert!(!wide.is_empty() && wide.iter().all(|n| n.contains("36") || n.contains("33")), "{wide:?}");
        let plain = find("kind:cabinet base NOT sink -tag:sink -\"drawer base\"");
        assert!(!plain.is_empty() && plain.iter().all(|n| n.starts_with("Base Cabinet")), "{plain:?}");
        assert!(!find("\"white quartz\"").is_empty());
        let stone = find("kind:material category:stone");
        assert!(!stone.is_empty() && stone.iter().all(|n| n.contains("Countertop") || n.contains("Granite")), "{stone:?}");
        assert_eq!(find("tag:pantry width=24").len(), 2);
        assert!(Query::parse("(base").is_err());
        assert!(Query::parse("\"open").is_err());
        assert_eq!(Query::parse("  ").unwrap(), Query::All);
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_new_item_kinds() {
        let doc = Document::new("s");
        let dir = std::env::temp_dir().join(format!("hc-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.hcsettings");
        let n = export_settings(&doc, &path).unwrap();
        let back = read_settings(&path).unwrap();
        assert_eq!(back.len(), n);
        assert!(back.iter().any(|o| o.kind() == ObjectKind::WallType));
        assert!(back.iter().all(|o| !matches!(o, Object::Layer(l) if l.text_style.is_some())));
        // Built-in notes are valid objects.
        for item in builtin().iter().filter(|i| i.category == "Notes") {
            let objects: Vec<Object> = serde_json::from_value(item.data["objects"].clone()).unwrap();
            assert_eq!(objects.len(), 1);
        }
        // Favourites and saved searches persist.
        let mut favs = std::collections::BTreeSet::new();
        favs.insert("a|b|c".to_string());
        save_favorites(&dir, &favs).unwrap();
        assert_eq!(load_favorites(&dir), favs);
        let filters = vec![SavedFilter { name: "Sinks".into(), query: "tag:sink".into() }];
        save_filters(&dir, &filters).unwrap();
        assert_eq!(load_filters(&dir), filters);
        // Editing a user item replaces its file.
        let mut item = builtin().into_iter().find(|i| i.kind == LibraryKind::Material).unwrap();
        let p = save_item(&dir, &item).unwrap();
        item.source = Source::User(p.clone());
        let mut renamed = item.clone();
        renamed.name = "Renamed".into();
        update_item(&dir, &item, &renamed).unwrap();
        assert!(!p.exists());
        assert!(user_items(&dir).iter().any(|i| i.name == "Renamed"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod fill_tests {
    use super::*;

    #[test]
    fn built_in_fills_and_shortcuts() {
        let items = builtin();
        assert!(items.iter().any(|i| i.kind == LibraryKind::HatchPattern && i.name == "Running Bond Brick"));
        let fill = items.iter().find(|i| i.kind == LibraryKind::FillStyle).unwrap();
        assert!(serde_json::from_value::<FillStyle>(fill.data.clone()).is_ok());
        let dir = std::env::temp_dir().join(format!("hc-shortcuts-{}", std::process::id()));
        save_shortcuts(&dir, &["Fills/Hatch Patterns".to_string()]).unwrap();
        assert_eq!(load_shortcuts(&dir), ["Fills/Hatch Patterns"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod plan_settings_tests {
    use super::*;

    #[test]
    fn settings_files_carry_macros_patterns_and_hotkeys() {
        let mut doc = Document::new("a");
        doc.settings.macros.insert("builder".into(), "Acme".into());
        doc.settings.hatch_patterns.push(HatchPattern { name: "Brick".into(), lines: vec![HatchLine::default()] });
        let path = std::env::temp_dir().join(format!("hc-settings-{}.hcsettings", std::process::id()));
        let n = export_settings_with(&doc, &path, json!([{"keys": "G W", "tool": "Straight Exterior Wall"}])).unwrap();
        assert!(n >= 2);
        let (plan, keys) = read_plan_settings(&path).unwrap();
        assert_eq!(plan.macros["builder"], "Acme");
        assert_eq!(plan.hatch_patterns[0].name, "Brick");
        assert_eq!(keys[0]["keys"], "G W");
        let _ = std::fs::remove_file(&path);
    }
}
