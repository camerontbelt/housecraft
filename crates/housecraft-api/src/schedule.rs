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
    }
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

/// Build a schedule of `kind` from `doc`.
pub fn schedule(doc: &Document, kind: ScheduleKind, opts: &ScheduleOptions) -> ScheduleTable {
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
                row.insert("Floor", floor_name(floor));
                row.insert("Renovation", o.renovation.name().to_string());
                let key = ["Type", "Width", "Height", "Sill", "Floor", "Renovation"]
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
            for (i, (_, row)) in raw.iter_mut().enumerate() {
                row.insert("Label", format!("{prefix}{:02}", i + 1));
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
                    Some(l) => format!("{} ({l})", e.electrical_kind.name()),
                    None => e.electrical_kind.name().to_string(),
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

    let all = available_columns(kind);
    let columns: Vec<&str> = if opts.columns.is_empty() {
        all.to_vec()
    } else {
        opts.columns
            .iter()
            .filter_map(|c| all.iter().find(|a| a.eq_ignore_ascii_case(c)).copied())
            .collect()
    };
    ScheduleTable {
        title: title(kind).to_string(),
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
