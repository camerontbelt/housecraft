//! Read-only views of a document, shaped for CLI output and MCP responses.

use housecraft_geom::{rooms, v2};
use housecraft_model::*;
use housecraft_units::{Length, format_area};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentSummary {
    pub name: String,
    pub floors: Vec<FloorSummary>,
    pub wall_types: Vec<ObjectInfo>,
    pub wall_count: usize,
    /// Sum of rooms whose type counts as living area, all floors.
    pub living_area: String,
    pub default_set: ObjectInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FloorSummary {
    pub id: Id,
    pub name: String,
    /// Formatted in the document's display unit.
    pub elevation: String,
    pub ceiling_height: String,
    pub wall_count: usize,
    pub total_wall_length: String,
    pub openings: usize,
    pub rooms: Vec<RoomSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RoomSummary {
    pub id: Id,
    pub name: String,
    pub room_type: String,
    pub area: String,
    pub ceiling_height: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObjectInfo {
    pub id: Id,
    pub kind: ObjectKind,
    pub label: String,
}

/// Interior area in mm² of each room on a floor.
pub fn room_areas(doc: &Document, floor: Id) -> Vec<(Id, f64)> {
    let regions = rooms::detect(doc, floor);
    doc.rooms_on(floor)
        .filter_map(|r| {
            regions
                .room_at(v2(r.anchor))
                .map(|i| (r.id, regions.rooms[i].area))
        })
        .collect()
}

pub fn summary(doc: &Document) -> DocumentSummary {
    let unit = doc.settings.display_unit;
    let fmt = |l: Length| l.format(unit);
    let mut living = 0.0;
    let floors = doc
        .floors_by_elevation()
        .into_iter()
        .map(|f| {
            let areas = room_areas(doc, f.id);
            let rooms = doc
                .rooms_on(f.id)
                .map(|r| {
                    let area = areas
                        .iter()
                        .find(|(id, _)| *id == r.id)
                        .map_or(0.0, |(_, a)| *a);
                    let ty = doc.get::<RoomType>(r.room_type);
                    // Crawlspaces and slabs are not living space; basements can be.
                    let below_grade_shell = f
                        .foundation
                        .as_ref()
                        .is_some_and(|fd| fd.kind != FoundationKind::Basement);
                    if r.living_area.unwrap_or(ty.is_some_and(|t| t.living_area)) && !below_grade_shell {
                        living += area;
                    }
                    RoomSummary {
                        id: r.id,
                        name: doc.room_name(r),
                        room_type: ty.map_or_else(String::new, |t| t.name.clone()),
                        area: format_area(area, unit),
                        ceiling_height: doc.room_ceiling_height(r).map_or_else(String::new, fmt),
                    }
                })
                .collect();
            FloorSummary {
                id: f.id,
                name: f.name.clone(),
                elevation: fmt(f.elevation),
                ceiling_height: fmt(f.ceiling_height),
                wall_count: doc.walls_on(f.id).count(),
                total_wall_length: fmt(doc.walls_on(f.id).map(|w| w.length()).sum()),
                openings: doc
                    .walls_on(f.id)
                    .map(|w| doc.openings_in(w.id).count())
                    .sum(),
                rooms,
            }
        })
        .collect();
    let defaults = doc.active_defaults();
    DocumentSummary {
        name: doc.name.clone(),
        floors,
        wall_types: list_objects(doc, Some(ObjectKind::WallType)),
        wall_count: doc.count::<Wall>(),
        living_area: format_area(living, unit),
        default_set: describe(doc, &Object::DefaultSet(defaults.clone())),
    }
}

/// List objects, optionally filtered to one kind, in id order.
pub fn list_objects(doc: &Document, kind: Option<ObjectKind>) -> Vec<ObjectInfo> {
    doc.objects
        .values()
        .filter(|o| kind.is_none_or(|k| o.kind() == k))
        .map(|o| describe(doc, o))
        .collect()
}

/// Full JSON for one object (tagged with `kind`), or `None` if the id is unknown.
pub fn get_object(doc: &Document, id: Id) -> Option<serde_json::Value> {
    doc.object(id)
        .map(|o| serde_json::to_value(o).expect("model serializes"))
}

/// A one-line human description of an object.
pub fn describe(doc: &Document, obj: &Object) -> ObjectInfo {
    let unit = doc.settings.display_unit;
    let fmt = |l: Length| l.format(unit);
    let name_of = |id: Id| -> String {
        match doc.object(id) {
            Some(Object::Floor(f)) => f.name.clone(),
            Some(Object::WallType(t)) => t.name.clone(),
            Some(Object::Layer(l)) => l.name.clone(),
            _ => id.to_string(),
        }
    };
    let label = match obj {
        Object::Floor(f) => format!("{} (elev {})", f.name, fmt(f.elevation)),
        Object::WallType(t) => format!("{} ({})", t.name, fmt(t.thickness())),
        Object::Wall(w) => format!(
            "{}, {} long, {}",
            name_of(w.wall_type),
            fmt(w.length()),
            name_of(w.floor)
        ),
        Object::Opening(o) => {
            let what = match o.opening_kind {
                OpeningKind::Door(s) => s.name(),
                OpeningKind::Window(s) => s.name(),
            };
            let mull = if o.details.mulled > 1 { format!(", mulled x{}", o.details.mulled) } else { String::new() };
            format!(
                "{what} {} x {} in wall {}{mull}",
                fmt(o.width),
                fmt(o.height),
                o.wall
            )
        }
        Object::RoomType(t) => t.name.clone(),
        Object::Room(r) => format!("{} on {}", doc.room_name(r), name_of(r.floor)),
        Object::Layer(l) => l.name.clone(),
        Object::LayerSet(s) => s.name.clone(),
        Object::TextStyle(s) => format!("{} ({})", s.name, fmt(s.height)),
        Object::DefaultSet(s) => s.name.clone(),
        Object::Cad(c) => {
            let shape = match &c.shape {
                Shape::Point { .. } => "point".to_string(),
                Shape::Line { a, b, .. } => format!("line {} long", fmt(a.distance(*b))),
                Shape::Polyline { points, closed } => {
                    format!(
                        "{} polyline, {} points",
                        if *closed { "closed" } else { "open" },
                        points.len()
                    )
                }
                Shape::Arc { radius, .. } => format!("arc r {}", fmt(*radius)),
                Shape::Circle { radius, .. } => format!("circle r {}", fmt(*radius)),
                Shape::Ellipse { .. } => "ellipse".to_string(),
                Shape::Spline { points, .. } => format!("spline, {} points", points.len()),
                Shape::CrossBox { a, b, .. } => format!("cross box {} long", fmt(a.distance(*b))),
                Shape::BlockingBox { a, b, .. } => {
                    format!("blocking box {} long", fmt(a.distance(*b)))
                }
                Shape::Insulation { a, b, .. } => {
                    format!("insulation {} long", fmt(a.distance(*b)))
                }
            };
            format!("{shape} on {}", name_of(c.layer))
        }
        Object::Dimension(d) => match &d.geometry {
            DimensionGeometry::Linear { points, .. } => {
                format!("linear dimension, {} points", points.len())
            }
            DimensionGeometry::Angular { .. } => "angular dimension".to_string(),
        },
        Object::Text(t) => format!("text {:?}", t.text),
        Object::RoofPlane(r) if r.gable => "gable infill".to_string(),
        Object::RoofPlane(r) => format!(
            "roof plane, {:.1}° ({:.1} in 12){}",
            r.pitch,
            r.pitch.to_radians().tan() * 12.0,
            if r.auto { ", automatic" } else { "" }
        ),
        Object::Electrical(e) if e.electrical_kind.is_switch() => {
            let l = doc.switch_labels().remove(&e.id).unwrap_or_default();
            format!(
                "{} ({l}) controlling {} fixture(s)",
                e.electrical_kind.name(),
                e.controls.len()
            )
        }
        Object::Electrical(e) => e.electrical_kind.name().to_string(),
        Object::Schedule(s) => format!("{} on {}", s.schedule_kind.name(), name_of(s.floor)),
        Object::SectionView(v) => v.name.clone(),
        Object::Layout(l) => format!(
            "{} ({} page{})",
            l.name,
            l.pages.len(),
            if l.pages.len() == 1 { "" } else { "s" }
        ),
        Object::LayoutBox(b) => format!(
            "{} on {} page {}",
            crate::layouts::source_name(doc, &b.source),
            doc.get::<Layout>(b.layout).map_or("?", |l| l.name.as_str()),
            b.page + 1
        ),
        Object::Callout(c) => format!("callout {:?}", c.text),
        Object::CadDetail(c) => format!("CAD detail {:?}", c.name),
        Object::Material(m) => format!("material {:?} ({})", m.name, m.class.name()),
        Object::Cabinet(c) => format!(
            "{} {} wide{}",
            c.cabinet_kind.name(),
            fmt(c.width),
            if c.sink { ", sink" } else { "" }
        ),
        Object::Countertop(c) => format!(
            "countertop, {} thick, {} hole{}",
            fmt(c.thickness),
            c.holes.len(),
            if c.holes.len() == 1 { "" } else { "s" }
        ),
        Object::Appliance(a) => format!("{} {} wide", a.appliance_kind.name(), fmt(a.width)),
        Object::Backsplash(b) => format!("backsplash {} high", fmt(b.height)),
        Object::Terrain(t) => format!("terrain, {} points", t.perimeter.len()),
        Object::SiteFeature(f) => f.feature.name().to_lowercase(),
        Object::FramingMember(m) => format!("{} {} x {}", m.size_name(), m.role.name().to_lowercase(), fmt(m.length())),
        Object::Annotation3d(a) => match &a.mark {
            Mark3d::Note { text, .. } => format!("3D note {:?}", text),
            Mark3d::Dimension { a, b } => {
                let d: f64 = a.mm().iter().zip(b.mm()).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt();
                format!("3D dimension {}", fmt(Length::from_mm(d)))
            }
        },
        Object::Walkthrough(w) => format!("walkthrough {:?}, {} keyframes, {:.0} s", w.name, w.keyframes.len(), w.duration()),
        Object::Dormer(d) => format!("{:?} dormer, {} wide", d.style, fmt(d.width)).to_lowercase(),
        Object::Deck(dk) => format!("{}, top at {}", if dk.platform { "platform" } else { "deck" }, fmt(dk.height)),
        Object::JoistDirection(j) => format!("{} joist direction {:.0}°", if j.ceiling { "ceiling" } else { "floor" }, j.angle),
        Object::Plant(p) => format!("{} ({}), {} high", p.name, p.form.name().to_lowercase(), fmt(p.height)),
        Object::ElevationData(e) => match &e.shape {
            ElevationShape::Point { elevation, .. }
            | ElevationShape::Line { elevation, .. }
            | ElevationShape::Region { elevation, .. } => format!("{} at {}", e.shape.name(), fmt(*elevation)),
            ElevationShape::Hill { height, .. } => format!("hill {} high", fmt(*height)),
            ElevationShape::Valley { depth, .. } => format!("valley {} deep", fmt(*depth)),
            ElevationShape::BreakLine { elevations, .. } => format!(
                "break line from {} to {}",
                elevations.first().map_or(String::new(), |l| fmt(*l)),
                elevations.last().map_or(String::new(), |l| fmt(*l))
            ),
            ElevationShape::Subfloor { below, .. } => format!("subfloor marker, ground {} below", fmt(*below)),
        },
        Object::Solid(x) => match &x.shape {
            SolidShape::Extrude { height, .. } => format!("solid {} high", fmt(*height)),
            SolidShape::Revolve { sweep, .. } => format!("revolved solid, {sweep}°"),
        },
        Object::MoldingProfile(p) => format!("molding profile {:?}", p.name),
        Object::MaterialRegion(r) => format!(
            "{} region of {}",
            match r.target {
                RegionTarget::Floor { .. } => "floor",
                RegionTarget::Wall { .. } => "wall",
            },
            doc.get::<Material>(r.material).map_or("?", |m| m.name.as_str())
        ),
        Object::StylePalette(st) => format!("style {:?}", st.name),
        Object::MoldingPolyline(m) => format!("molding, {} profile{}", m.stack.len(), if m.stack.len() == 1 { "" } else { "s" }),
        Object::Picture(p) => format!(
            "picture {}",
            std::path::Path::new(&p.path)
                .file_name()
                .map_or(p.path.clone(), |f| f.to_string_lossy().into_owned())
        ),
        Object::SymbolDef(s) => format!("3D symbol {:?}, {} triangles", s.name, s.triangles.len()),
        Object::Symbol(s) => format!(
            "{} {} wide",
            doc.get::<SymbolDef>(s.def).map_or("symbol", |d| d.name.as_str()),
            fmt(s.width)
        ),
        Object::MaterialAssignment(a) => format!(
            "{} painted on {}{}",
            doc.get::<Material>(a.material).map_or("?", |m| m.name.as_str()),
            a.object,
            a.surface.map_or(String::new(), |s| format!(" ({})", s.name()))
        ),
        Object::Block(b) => format!("block {:?}", b.name),
        Object::BlockRef(r) => format!(
            "block {:?}",
            doc.get::<Block>(r.block).map_or("?", |b| b.name.as_str())
        ),
        Object::RevisionCloud(c) => format!(
            "revision cloud{}",
            c.label.as_ref().map_or(String::new(), |l| format!(" {l}"))
        ),
        Object::Stair(s) => {
            let shape = match s.shape {
                StairShape::Straight => "straight",
                StairShape::L { .. } => "L-shaped",
                StairShape::U { .. } => "U-shaped",
                StairShape::Curved { .. } => "curved",
            };
            let rise = doc.stair_rise(s);
            let risers = ((rise.mm() / s.max_riser.mm()).ceil() as u32).max(1);
            format!(
                "{shape} stair, {risers} risers of {}, {} wide",
                fmt(rise / risers as f64),
                fmt(s.width)
            )
        }
        Object::PlanView(v) => v.name.clone(),
        Object::Camera(c) => c.name.clone(),
    };
    ObjectInfo {
        id: obj.id(),
        kind: obj.kind(),
        label,
    }
}
