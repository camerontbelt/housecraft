//! Schedules: tables of doors, windows, rooms, walls and other objects, built
//! from the live model, with CSV / text / XML / HTML export.

use std::collections::BTreeMap;

use housecraft_model::*;
use housecraft_units::{Length, format_area};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use housecraft_model::ScheduleKind;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleTable {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<ScheduleRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleRow {
    /// The objects this row stands for (several when identical ones are grouped).
    pub ids: Vec<Id>,
    pub cells: Vec<String>,
}

/// What to include and how.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleOptions {
    /// Only objects on this floor; `None` for every floor.
    #[serde(default)]
    pub floor: Option<Id>,
    /// Columns to show, in order, by name; empty for all of the kind's columns.
    #[serde(default)]
    pub columns: Vec<String>,
    /// More kinds listed in the same table (a mixed schedule).
    #[serde(default)]
    pub also: Vec<ScheduleKind>,
    /// Only objects that are existing, new or to be demolished.
    #[serde(default)]
    pub renovation: Option<Renovation>,
    /// The first label number (D01 when 1).
    #[serde(default)]
    pub start: Option<u32>,
    /// Rows whose objects are listed here come first, in this order (rows
    /// moved up or down by hand); the rest follow in the usual order.
    #[serde(default)]
    pub order: Vec<Id>,
}

/// Every column a schedule of `kind` can show, in default order.
pub fn available_columns(kind: ScheduleKind) -> &'static [&'static str] {
    match kind {
        ScheduleKind::Doors => &[
            "Label",
            "Qty",
            "Type",
            "Width",
            "Height",
            "Swing",
            "Glass Area",
            "U-Factor",
            "Floor",
            "Renovation",
        ],
        ScheduleKind::Windows => &[
            "Label",
            "Qty",
            "Type",
            "Width",
            "Height",
            "Sill",
            "Glass Area",
            "U-Factor",
            "SHGC",
            "Floor",
            "Renovation",
        ],
        ScheduleKind::Rooms => &[
            "Name",
            "Type",
            "Floor",
            "Area",
            "Ceiling",
            "Floor Finish",
            "Wall Finish",
            "Living",
        ],
        ScheduleKind::Walls => &[
            "Type",
            "Qty",
            "Thickness",
            "Length",
            "Height",
            "Area",
            "Floor",
        ],
        ScheduleKind::Fixtures => &["Trade", "Item", "Qty", "Floor"],
        ScheduleKind::Stairs => &[
            "Shape",
            "Width",
            "Risers",
            "Riser",
            "Tread",
            "Total Rise",
            "Floor",
        ],
        ScheduleKind::RoofPlanes => &["Pitch", "Area", "Floor"],
        ScheduleKind::Cabinets => &[
            "Code", "Type", "Qty", "Width", "Depth", "Height", "Doors", "Drawers", "Floor",
        ],
        ScheduleKind::Plants => &["Plant", "Form", "Qty", "Mature Height", "Spread"],
        ScheduleKind::Hardscape => &["Type", "Name", "Width", "Thickness", "Length", "Area"],
        ScheduleKind::Framing => &["Role", "Size", "Material", "Qty", "Cut Length", "Floor"],
        ScheduleKind::Shapes => &["Label", "Layer", "Area", "Perimeter", "Floor"],
        ScheduleKind::Notes => &["No.", "Note", "Floor"],
    }
}

pub fn title(kind: ScheduleKind) -> &'static str {
    match kind {
        ScheduleKind::Doors => "Door Schedule",
        ScheduleKind::Windows => "Window Schedule",
        ScheduleKind::Rooms => "Room Finish Schedule",
        ScheduleKind::Walls => "Wall Schedule",
        ScheduleKind::Fixtures => "Electrical, Plumbing and HVAC Schedule",
        ScheduleKind::Stairs => "Stair Schedule",
        ScheduleKind::RoofPlanes => "Roof Plane Schedule",
        ScheduleKind::Cabinets => "Cabinet Schedule",
        ScheduleKind::Plants => "Plant Schedule",
        ScheduleKind::Hardscape => "Hardscape Schedule",
        ScheduleKind::Framing => "Framing Cut List",
        ScheduleKind::Shapes => "Shape Schedule",
        ScheduleKind::Notes => "Note Schedule",
    }
}

/// Numbered notes' numbers: in the order they were made, from 1.
pub fn note_numbers(doc: &Document) -> BTreeMap<Id, u32> {
    doc.iter::<Callout>().filter(|c| c.note).enumerate().map(|(i, c)| (c.id, i as u32 + 1)).collect()
}

/// A full row: column name → value, plus the objects it covers.
type RawRow = (Vec<Id>, BTreeMap<&'static str, String>);

/// Hidden grouping key stored with each grouped row.
const KEY: &str = "\u{1f}key";

/// Add `row` for object `id`, or count it into an existing row with the same `key`.
fn group(raw: &mut Vec<RawRow>, key: Vec<String>, id: Id, row: BTreeMap<&'static str, String>) {
    let k = key.join("\u{1f}");
    match raw
        .iter_mut()
        .find(|(_, r)| r.get(KEY).is_some_and(|x| *x == k))
    {
        Some((ids, r)) => {
            ids.push(id);
            r.insert("Qty", ids.len().to_string());
        }
        None => {
            let mut row = row;
            row.insert(KEY, k);
            row.insert("Qty", "1".into());
            raw.push((vec![id], row));
        }
    }
}

/// Build a schedule of `kind` (and `opts.also`) from `doc`.
pub fn schedule(doc: &Document, kind: ScheduleKind, opts: &ScheduleOptions) -> ScheduleTable {
    let mut kinds = vec![kind];
    kinds.extend(opts.also.iter().copied().filter(|k| *k != kind));
    let mixed = kinds.len() > 1;
    // Columns: the kinds' columns in order, each once ("Kind" first when mixed).
    let mut all: Vec<&'static str> = if mixed { vec!["Kind"] } else { Vec::new() };
    let mut raw: Vec<RawRow> = Vec::new();
    for k in &kinds {
        for c in available_columns(*k) {
            if !all.contains(c) {
                all.push(c);
            }
        }
        let mut rows = kind_rows(doc, *k, opts);
        if mixed {
            for (_, r) in rows.iter_mut() {
                r.insert("Kind", title(*k).trim_end_matches(" Schedule").to_string());
            }
        }
        raw.extend(rows);
    }
    if let Some(phase) = opts.renovation {
        let want = serde_json::to_value(phase).ok();
        raw.retain(|(ids, _)| {
            ids.iter().any(|id| {
                let v = doc.object(*id).and_then(|o| serde_json::to_value(o).ok());
                // Objects without a renovation status count as new.
                let status = v.as_ref().and_then(|v| v.get("renovation")).cloned().or_else(|| serde_json::to_value(Renovation::default()).ok());
                status == want
            })
        });
    }
    if !opts.order.is_empty() {
        let rank = |ids: &[Id]| ids.iter().filter_map(|id| opts.order.iter().position(|o| o == id)).min().unwrap_or(usize::MAX);
        raw.sort_by_key(|(ids, _)| rank(ids));
    }
    let columns: Vec<&str> = if opts.columns.is_empty() {
        all.clone()
    } else {
        opts.columns
            .iter()
            .filter_map(|c| all.iter().find(|a| a.eq_ignore_ascii_case(c)).copied())
            .collect()
    };
    ScheduleTable {
        title: if mixed { "Schedule".to_string() } else { title(kind).to_string() },
        columns: columns.iter().map(|c| c.to_string()).collect(),
        rows: raw
            .into_iter()
            .map(|(ids, row)| ScheduleRow {
                ids,
                cells: columns
                    .iter()
                    .map(|c| row.get(c).cloned().unwrap_or_default())
                    .collect(),
            })
            .collect(),
    }
}

/// The rows of one kind of schedule, every column filled.
fn kind_rows(doc: &Document, kind: ScheduleKind, opts: &ScheduleOptions) -> Vec<RawRow> {
    let unit = doc.settings.display_unit;
    let fmt = |l: Length| l.format(unit);
    let area = |mm2: f64| format_area(mm2, unit);
    let floor_name = |id: Id| {
        doc.get::<Floor>(id)
            .map_or(String::new(), |f| f.name.clone())
    };
    let on_floor = |f: Id| opts.floor.is_none_or(|o| o == f);
    let mut raw: Vec<RawRow> = Vec::new();

    match kind {
        ScheduleKind::Doors | ScheduleKind::Windows => {
            let doors = kind == ScheduleKind::Doors;
            for o in doc.iter::<Opening>() {
                if o.opening_kind.is_door() != doors {
                    continue;
                }
                let Some(floor) = doc.get::<Wall>(o.wall).map(|w| w.floor) else {
                    continue;
                };
                if !on_floor(floor) {
                    continue;
                }
                let type_name = match o.opening_kind {
                    OpeningKind::Door(s) => s.name(),
                    OpeningKind::Window(s) => s.name(),
                };
                let mut row = BTreeMap::new();
                row.insert("Type", type_name.to_string());
                row.insert("Width", fmt(o.width));
                row.insert("Height", fmt(o.height));
                row.insert("Sill", fmt(o.sill));
                row.insert("Swing", if o.swing_in { "In" } else { "Out" }.to_string());
                let d = Some(&o.details);
                // Glass: a window's sash less a 2 1/2" frame; a door's lite.
                let frame = 63.5;
                let glass = match o.opening_kind {
                    OpeningKind::Window(_) => (o.width.mm() - 2.0 * frame).max(0.0) * (o.height.mm() - 2.0 * frame).max(0.0),
                    OpeningKind::Door(_) if d.is_some_and(|d| d.glass.is_some()) => 0.4 * o.width.mm() * o.height.mm(),
                    OpeningKind::Door(_) => 0.0,
                };
                row.insert("Glass Area", if glass > 0.0 { area(glass) } else { String::new() });
                row.insert("U-Factor", d.and_then(|d| d.u_factor).map_or(String::new(), |u| format!("{u:.2}")));
                row.insert("SHGC", d.and_then(|d| d.shgc).map_or(String::new(), |g| format!("{g:.2}")));
                row.insert("Floor", floor_name(floor));
                row.insert("Renovation", o.renovation.name().to_string());
                let key = ["Type", "Width", "Height", "Sill", "U-Factor", "SHGC", "Floor", "Renovation"]
                    .iter()
                    .map(|c| row[c].clone())
                    .collect();
                group(&mut raw, key, o.id, row);
            }
            // Label in order of size so D01 is the widest door type.
            raw.sort_by(|a, b| {
                let w = |r: &RawRow| doc.get::<Opening>(r.0[0]).map_or(0.0, |o| o.width.mm());
                w(b).total_cmp(&w(a))
                    .then_with(|| a.1["Type"].cmp(&b.1["Type"]))
            });
            let prefix = if doors { "D" } else { "W" };
            let start = opts.start.unwrap_or(1) as usize;
            for (i, (_, row)) in raw.iter_mut().enumerate() {
                row.insert("Label", format!("{prefix}{:02}", i + start));
            }
        }
        ScheduleKind::Rooms => {
            for floor in doc.floors_by_elevation() {
                // Crawlspaces, slabs and piers enclose no rooms worth listing.
                let unfinished = floor
                    .foundation
                    .as_ref()
                    .is_some_and(|f| f.kind != FoundationKind::Basement);
                if !on_floor(floor.id) || unfinished {
                    continue;
                }
                let areas: BTreeMap<Id, f64> = crate::query::room_areas(doc, floor.id)
                    .into_iter()
                    .collect();
                for room in doc.rooms_on(floor.id) {
                    let rt = doc.get::<RoomType>(room.room_type);
                    let mut row = BTreeMap::new();
                    row.insert("Name", doc.room_name(room));
                    row.insert("Type", rt.map_or(String::new(), |t| t.name.clone()));
                    row.insert("Floor", floor.name.clone());
                    row.insert(
                        "Area",
                        areas.get(&room.id).map_or(String::new(), |a| area(*a)),
                    );
                    row.insert(
                        "Ceiling",
                        doc.room_ceiling_height(room).map_or(String::new(), fmt),
                    );
                    row.insert(
                        "Floor Finish",
                        rt.map_or(String::new(), |t| t.floor_finish.clone()),
                    );
                    row.insert(
                        "Wall Finish",
                        rt.map_or(String::new(), |t| t.wall_finish.clone()),
                    );
                    row.insert(
                        "Living",
                        if rt.is_some_and(|t| t.living_area) {
                            "Yes"
                        } else {
                            "No"
                        }
                        .into(),
                    );
                    raw.push((vec![room.id], row));
                }
            }
        }
        ScheduleKind::Walls => {
            // One row per wall type and floor, totalling length and area.
            // (wall type, floor) -> (walls, total length, total face area, tallest).
            type Totals = (Vec<Id>, f64, f64, f64);
            let mut totals: BTreeMap<(Id, Id), Totals> = BTreeMap::new();
            for w in doc.iter::<Wall>().filter(|w| on_floor(w.floor)) {
                let len = w.start.distance(w.end).mm();
                let h = doc.wall_height(w).map_or(0.0, |h| h.mm());
                let e = totals.entry((w.wall_type, w.floor)).or_default();
                e.0.push(w.id);
                e.1 += len;
                e.2 += len * h;
                e.3 = e.3.max(h);
            }
            for ((wt, floor), (ids, len, a, h)) in totals {
                let t = doc.get::<WallType>(wt);
                let mut row = BTreeMap::new();
                row.insert("Type", t.map_or(String::new(), |t| t.name.clone()));
                row.insert("Qty", ids.len().to_string());
                row.insert("Thickness", t.map_or(String::new(), |t| fmt(t.thickness())));
                row.insert("Length", fmt(Length::from_mm(len)));
                row.insert("Height", fmt(Length::from_mm(h)));
                row.insert("Area", area(a));
                row.insert("Floor", floor_name(floor));
                raw.push((ids, row));
            }
        }
        ScheduleKind::Fixtures => {
            let labels = doc.switch_labels();
            for e in doc.iter::<Electrical>().filter(|e| on_floor(e.floor)) {
                let item = match labels.get(&e.id) {
                    Some(l) => format!("{} ({l})", e.display_name()),
                    None => e.display_name().to_string(),
                };
                let mut row = BTreeMap::new();
                row.insert("Trade", e.electrical_kind.trade().to_string());
                row.insert("Item", item);
                row.insert("Floor", floor_name(e.floor));
                let key = ["Trade", "Item", "Floor"]
                    .iter()
                    .map(|c| row[c].clone())
                    .collect();
                group(&mut raw, key, e.id, row);
            }
            raw.sort_by(|a, b| (&a.1["Trade"], &a.1["Item"]).cmp(&(&b.1["Trade"], &b.1["Item"])));
        }
        ScheduleKind::Cabinets => {
            // Door and drawer faces counted by size: "2 @ 11 3/4" x 23"".
            let sizes = |list: Vec<(f64, f64)>| -> String {
                let mut counts: Vec<(String, usize)> = Vec::new();
                for (w, h) in list {
                    let k = format!("{} x {}", fmt(Length::from_mm(w)), fmt(Length::from_mm(h)));
                    match counts.iter_mut().find(|(s, _)| *s == k) {
                        Some((_, n)) => *n += 1,
                        None => counts.push((k, 1)),
                    }
                }
                counts
                    .iter()
                    .map(|(k, n)| format!("{n} @ {k}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            for c in doc.iter::<Cabinet>().filter(|c| on_floor(c.floor)) {
                let (doors, drawers) = housecraft_geom::cabinets::front_sizes(c);
                let mut row = BTreeMap::new();
                row.insert("Code", c.code());
                row.insert("Type", c.cabinet_kind.name().into());
                row.insert("Width", fmt(c.width));
                row.insert("Depth", fmt(c.depth));
                row.insert("Height", fmt(c.height));
                row.insert("Doors", sizes(doors));
                row.insert("Drawers", sizes(drawers));
                row.insert("Floor", floor_name(c.floor));
                let key = ["Code", "Width", "Depth", "Height", "Doors", "Drawers", "Floor"]
                    .iter()
                    .map(|k| row[k].clone())
                    .collect();
                group(&mut raw, key, c.id, row);
            }
            raw.sort_by(|a, b| a.1["Code"].cmp(&b.1["Code"]));
        }
        ScheduleKind::Stairs => {
            for s in doc.iter::<Stair>().filter(|s| on_floor(s.floor)) {
                let g = housecraft_geom::stairs::geometry(doc, s);
                let mut row = BTreeMap::new();
                row.insert(
                    "Shape",
                    match s.shape {
                        StairShape::Straight => "Straight",
                        StairShape::L { .. } => "L-shaped",
                        StairShape::U { .. } => "U-shaped",
                        StairShape::Curved { .. } => "Curved",
                        StairShape::Sections => "Sectioned",
                    }
                    .into(),
                );
                row.insert("Width", fmt(s.width));
                row.insert("Risers", g.risers.to_string());
                row.insert("Riser", fmt(Length::from_mm(g.riser_height)));
                row.insert("Tread", fmt(s.tread_depth));
                row.insert("Total Rise", fmt(doc.stair_rise(s)));
                row.insert("Floor", floor_name(s.floor));
                raw.push((vec![s.id], row));
            }
        }
        ScheduleKind::Framing => {
            for m in doc.iter::<FramingMember>().filter(|m| on_floor(m.floor)) {
                // Cut lengths to the nearest 1/8".
                let len = Length::from_inches((m.length().inches() * 8.0).round() / 8.0);
                let mut row = BTreeMap::new();
                row.insert("Role", m.role.name().to_string());
                row.insert("Size", m.size_name());
                row.insert("Material", m.material.name().to_string());
                row.insert("Cut Length", fmt(len));
                row.insert("Floor", floor_name(m.floor));
                let key = ["Role", "Size", "Material", "Cut Length", "Floor"].iter().map(|k| row[k].clone()).collect();
                group(&mut raw, key, m.id, row);
            }
            raw.sort_by(|a, b| (&a.1["Floor"], &a.1["Role"], &a.1["Size"]).cmp(&(&b.1["Floor"], &b.1["Role"], &b.1["Size"])));
        }
        ScheduleKind::Hardscape => {
            for f in doc.iter::<SiteFeature>().filter(|f| on_floor(f.floor)) {
                let SiteKind::Road { style, width, thickness, label, .. } = &f.feature else { continue };
                let len: f64 = f.points.windows(2).map(|w| housecraft_geom::v2(w[0]).distance(housecraft_geom::v2(w[1]))).sum();
                let mut row = BTreeMap::new();
                row.insert("Type", style.name().to_string());
                row.insert("Name", label.clone());
                row.insert("Width", fmt(*width));
                row.insert("Thickness", fmt(*thickness));
                row.insert("Length", fmt(Length::from_mm(len)));
                row.insert("Area", area(len * width.mm()));
                raw.push((vec![f.id], row));
            }
            raw.sort_by(|a, b| (&a.1["Type"], &a.1["Name"]).cmp(&(&b.1["Type"], &b.1["Name"])));
        }
        ScheduleKind::Plants => {
            for p in doc.iter::<Plant>().filter(|p| on_floor(p.floor)) {
                let mut row = BTreeMap::new();
                row.insert("Plant", p.name.clone());
                row.insert("Form", p.form.name().to_string());
                row.insert("Mature Height", fmt(p.height));
                row.insert("Spread", fmt(p.spread));
                let key = ["Plant", "Form", "Mature Height", "Spread"].iter().map(|k| row[k].clone()).collect();
                group(&mut raw, key, p.id, row);
            }
            raw.sort_by(|a, b| a.1["Plant"].cmp(&b.1["Plant"]));
        }
        ScheduleKind::Shapes => {
            for c in doc.iter::<Cad>().filter(|c| on_floor(c.floor)) {
                let polys = housecraft_geom::math::shape_polylines(&c.shape);
                let closed: Vec<&Vec<housecraft_geom::DVec2>> = polys.iter().filter(|(_, closed)| *closed).map(|(p, _)| p).collect();
                if closed.is_empty() {
                    continue;
                }
                let a: f64 = closed.iter().map(|p| housecraft_geom::math::signed_area(p).abs()).sum();
                let per: f64 = closed
                    .iter()
                    .map(|p| (0..p.len()).map(|i| p[i].distance(p[(i + 1) % p.len()])).sum::<f64>())
                    .sum();
                let label = doc
                    .settings
                    .labels
                    .get(&c.id)
                    .cloned()
                    .or_else(|| doc.settings.object_info.get(&c.id).map(|i| i.description.clone()))
                    .unwrap_or_default();
                let mut row = BTreeMap::new();
                row.insert("Label", label);
                row.insert("Layer", doc.get::<Layer>(c.layer).map_or(String::new(), |l| l.name.clone()));
                row.insert("Area", area(a));
                row.insert("Perimeter", fmt(Length::from_mm(per)));
                row.insert("Floor", doc.get::<Floor>(c.floor).map_or_else(|| "(detail)".to_string(), |f| f.name.clone()));
                raw.push((vec![c.id], row));
            }
        }
        ScheduleKind::Notes => {
            let numbers = note_numbers(doc);
            for c in doc.iter::<Callout>().filter(|c| c.note && on_floor(c.floor)) {
                let mut row = BTreeMap::new();
                row.insert("No.", numbers[&c.id].to_string());
                row.insert("Note", c.text.clone());
                row.insert("Floor", doc.get::<Floor>(c.floor).map_or_else(|| "(sheet)".to_string(), |f| f.name.clone()));
                raw.push((vec![c.id], row));
            }
        }
        ScheduleKind::RoofPlanes => {
            for r in doc
                .iter::<RoofPlane>()
                .filter(|r| !r.gable && on_floor(r.floor))
            {
                let pts: Vec<_> = r
                    .outline
                    .iter()
                    .map(|p| {
                        let [x, y, z] = p.mm();
                        housecraft_geom::DVec3::new(x, y, z)
                    })
                    .collect();
                let a = housecraft_geom::roof::newell(&pts).length() / 2.0;
                let rise = r.pitch.to_radians().tan() * 12.0;
                let mut row = BTreeMap::new();
                row.insert("Pitch", format!("{:.1}/12", rise));
                row.insert("Area", area(a));
                row.insert("Floor", floor_name(r.floor));
                raw.push((vec![r.id], row));
            }
        }
    }

    raw
}

/// Export formats for [`ScheduleTable::export`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TableFormat {
    Csv,
    Txt,
    Xml,
    Html,
}

impl TableFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "csv" => TableFormat::Csv,
            "txt" => TableFormat::Txt,
            "xml" => TableFormat::Xml,
            "html" | "htm" => TableFormat::Html,
            _ => return None,
        })
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl ScheduleTable {
    pub fn export(&self, format: TableFormat) -> String {
        match format {
            TableFormat::Csv => {
                let cell = |s: &str| {
                    if s.contains([',', '"', '\n']) {
                        format!("\"{}\"", s.replace('"', "\"\""))
                    } else {
                        s.to_string()
                    }
                };
                let mut out = String::new();
                for line in std::iter::once(&self.columns).chain(self.rows.iter().map(|r| &r.cells))
                {
                    out.push_str(&line.iter().map(|c| cell(c)).collect::<Vec<_>>().join(","));
                    out.push_str("\r\n");
                }
                out
            }
            TableFormat::Txt => {
                let widths: Vec<usize> = (0..self.columns.len())
                    .map(|i| {
                        self.rows
                            .iter()
                            .map(|r| r.cells[i].chars().count())
                            .chain(std::iter::once(self.columns[i].chars().count()))
                            .max()
                            .unwrap_or(0)
                    })
                    .collect();
                let line = |cells: &[String]| {
                    cells
                        .iter()
                        .zip(&widths)
                        .map(|(c, w)| format!("{c:<w$}"))
                        .collect::<Vec<_>>()
                        .join("  ")
                        .trim_end()
                        .to_string()
                };
                let mut out = format!("{}\n\n{}\n", self.title, line(&self.columns));
                out.push_str(
                    &widths
                        .iter()
                        .map(|w| "-".repeat(*w))
                        .collect::<Vec<_>>()
                        .join("  "),
                );
                out.push('\n');
                for r in &self.rows {
                    out.push_str(&line(&r.cells));
                    out.push('\n');
                }
                out
            }
            TableFormat::Xml => {
                let tag = |c: &str| {
                    c.chars()
                        .map(|ch| {
                            if ch.is_ascii_alphanumeric() {
                                ch.to_ascii_lowercase()
                            } else {
                                '_'
                            }
                        })
                        .collect::<String>()
                };
                let mut out = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<schedule title=\"{}\">\n",
                    xml_escape(&self.title)
                );
                for r in &self.rows {
                    out.push_str("  <row>\n");
                    for (c, v) in self.columns.iter().zip(&r.cells) {
                        let t = tag(c);
                        out.push_str(&format!("    <{t}>{}</{t}>\n", xml_escape(v)));
                    }
                    out.push_str("  </row>\n");
                }
                out.push_str("</schedule>\n");
                out
            }
            TableFormat::Html => {
                let mut out = format!(
                    "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{t}</title>\
                     <style>body{{font-family:sans-serif}}table{{border-collapse:collapse}}\
                     th,td{{border:1px solid #888;padding:4px 8px;text-align:left}}th{{background:#eee}}</style>\
                     </head><body><h1>{t}</h1><table>\n<tr>",
                    t = xml_escape(&self.title)
                );
                for c in &self.columns {
                    out.push_str(&format!("<th>{}</th>", xml_escape(c)));
                }
                out.push_str("</tr>\n");
                for r in &self.rows {
                    out.push_str("<tr>");
                    for v in &r.cells {
                        out.push_str(&format!("<td>{}</td>", xml_escape(v)));
                    }
                    out.push_str("</tr>\n");
                }
                out.push_str("</table></body></html>\n");
                out
            }
        }
    }
}
