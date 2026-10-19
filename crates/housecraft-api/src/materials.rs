//! Materials list: quantities taken off the live model, priced from the
//! document's price list, exportable like a schedule.

use std::collections::BTreeMap;

use housecraft_geom::{DVec2, DVec3, rooms, v2};
use housecraft_model::*;
use housecraft_units::{DisplayUnit, Length};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::schedule::{ScheduleRow, ScheduleTable};

/// Which part of the model to take off.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaterialsOptions {
    /// Only this floor; every floor when `None`.
    #[serde(default)]
    pub floor: Option<Id>,
    /// Only objects on layers this layer set shows.
    #[serde(default)]
    pub layer_set: Option<Id>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Measure {
    Count,
    Length,
    Area,
    Volume,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaterialLine {
    pub category: String,
    pub item: String,
    pub quantity: f64,
    pub unit: String,
    /// Price per unit from the document's price list (0 if unpriced).
    pub unit_price: f64,
    pub total: f64,
    pub ids: Vec<Id>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaterialsList {
    pub lines: Vec<MaterialLine>,
    pub subtotal: f64,
    pub markup_percent: f64,
    pub total: f64,
}

/// Accumulates quantities keyed by (category, item, measure).
#[derive(Default)]
struct Takeoff {
    map: BTreeMap<(String, String, Measure), (f64, Vec<Id>)>,
}

impl Takeoff {
    fn add(&mut self, category: &str, item: impl Into<String>, measure: Measure, qty: f64, id: Id) {
        if qty <= 0.0 {
            return;
        }
        let e = self
            .map
            .entry((category.to_string(), item.into(), measure))
            .or_default();
        e.0 += qty;
        if !e.1.contains(&id) {
            e.1.push(id);
        }
    }
}

fn imperial(unit: DisplayUnit) -> bool {
    matches!(
        unit,
        DisplayUnit::FeetInches { .. } | DisplayUnit::Inches { .. }
    )
}

/// Convert a quantity in mm / mm² / mm³ to the display system's unit.
fn convert(measure: Measure, v: f64, imperial: bool) -> (f64, &'static str) {
    match (measure, imperial) {
        (Measure::Count, _) => (v, "ea"),
        (Measure::Length, true) => (v / 304.8, "lin ft"),
        (Measure::Length, false) => (v / 1000.0, "m"),
        (Measure::Area, true) => (v / (304.8 * 304.8), "sq ft"),
        (Measure::Area, false) => (v / 1e6, "m²"),
        (Measure::Volume, true) => (v / (914.4f64.powi(3)), "cu yd"),
        (Measure::Volume, false) => (v / 1e9, "m³"),
    }
}

fn perimeter(poly: &[DVec2]) -> f64 {
    (0..poly.len())
        .map(|i| poly[i].distance(poly[(i + 1) % poly.len()]))
        .sum()
}

/// The price list key for an item.
pub fn price_key(category: &str, item: &str) -> String {
    format!("{category}: {item}")
}

/// Take off materials from `doc`.
pub fn materials(doc: &Document, opts: &MaterialsOptions) -> MaterialsList {
    let set = opts.layer_set.and_then(|s| doc.get::<LayerSet>(s));
    let shown = |id: Id| match (set, doc.layer_of(id)) {
        (Some(set), Some(layer)) => set.display(layer).visible,
        _ => true,
    };
    let on_floor = |f: Id| opts.floor.is_none_or(|o| o == f);
    let unit = doc.settings.display_unit;
    let fmt = |l: Length| l.format(unit);
    let mut t = Takeoff::default();

    // Walls: each layer's face area (net of openings), studs and plates for
    // framing layers, concrete for foundation walls.
    for w in doc.iter::<Wall>() {
        if !on_floor(w.floor) || !shown(w.id) || w.renovation == Renovation::Demolish {
            continue;
        }
        let Some(wt) = doc.get::<WallType>(w.wall_type) else {
            continue;
        };
        let len = w.start.distance(w.end).mm();
        let h = doc.wall_height(w).map_or(0.0, |h| h.mm());
        if let Some(r) = wt.railing {
            t.add(
                "Railings",
                format!("Railing {} high", fmt(r.height)),
                Measure::Length,
                len,
                w.id,
            );
            continue;
        }
        let openings: Vec<&Opening> = doc
            .openings_in(w.id)
            .filter(|o| o.renovation != Renovation::Demolish)
            .collect();
        let holes: f64 = openings.iter().map(|o| o.width.mm() * o.height.mm()).sum();
        let foundation = doc
            .get::<Floor>(w.floor)
            .is_some_and(|f| f.foundation.is_some());
        if foundation {
            t.add(
                "Foundation",
                "Concrete, foundation walls",
                Measure::Volume,
                len * h * wt.thickness().mm(),
                w.id,
            );
        } else {
            for layer in &wt.layers {
                let name = if layer.material.is_empty() {
                    &layer.name
                } else {
                    &layer.material
                };
                let framing = name.to_ascii_lowercase().contains("stud");
                if framing {
                    let spacing = Length::from_inches(16.0).mm();
                    let studs = (len / spacing).ceil() + 1.0 + 2.0 * openings.len() as f64;
                    t.add(
                        "Framing",
                        format!("{name}, {}", fmt(Length::from_mm(h))),
                        Measure::Count,
                        studs,
                        w.id,
                    );
                    t.add(
                        "Framing",
                        format!("{name} plates (3 per wall)"),
                        Measure::Length,
                        3.0 * len,
                        w.id,
                    );
                } else {
                    t.add(
                        "Wall Surfaces",
                        name.clone(),
                        Measure::Area,
                        (len * h - holes).max(0.0),
                        w.id,
                    );
                }
            }
        }
        if let Some(f) = w.footing {
            t.add(
                "Foundation",
                "Concrete, footings",
                Measure::Volume,
                len * f.width.mm() * f.depth.mm(),
                w.id,
            );
        }
    }

    // Doors and windows by type and size.
    for o in doc.iter::<Opening>() {
        let Some(floor) = doc.get::<Wall>(o.wall).map(|w| w.floor) else {
            continue;
        };
        if !on_floor(floor) || !shown(o.id) || o.renovation == Renovation::Demolish {
            continue;
        }
        let (cat, name) = match o.opening_kind {
            OpeningKind::Door(s) => ("Doors", s.name()),
            OpeningKind::Window(s) => ("Windows", s.name()),
        };
        t.add(
            cat,
            format!("{name} {} x {}", fmt(o.width), fmt(o.height)),
            Measure::Count,
            1.0,
            o.id,
        );
    }

    // Rooms: floor finish, ceiling board, base molding, platform layers.
    for floor in doc.floors_by_elevation() {
        if !on_floor(floor.id) {
            continue;
        }
        let regions = rooms::detect(doc, floor.id);
        for room in doc.rooms_on(floor.id) {
            if !shown(room.id) {
                continue;
            }
            let Some(i) = regions.room_at(v2(room.anchor)) else {
                continue;
            };
            let region = &regions.rooms[i];
            let rt = doc.get::<RoomType>(room.room_type);
            if let Some(f) = rt
                .map(|t| t.floor_finish.as_str())
                .filter(|f| !f.is_empty())
            {
                t.add("Flooring", f, Measure::Area, region.area, room.id);
            }
            if room.ceiling != CeilingStyle::None && floor.foundation.is_none() {
                t.add(
                    "Ceilings",
                    "Ceiling drywall",
                    Measure::Area,
                    region.area,
                    room.id,
                );
            }
            if floor.foundation.is_none() {
                let doors: f64 = region
                    .walls
                    .iter()
                    .flat_map(|w| doc.openings_in(*w))
                    .filter(|o| o.opening_kind.is_door())
                    .map(|o| o.width.mm())
                    .sum();
                t.add(
                    "Trim",
                    "Base molding",
                    Measure::Length,
                    (perimeter(&region.polygon) - doors).max(0.0),
                    room.id,
                );
            }
            for layer in &floor.platform {
                let name = if layer.material.is_empty() {
                    &layer.name
                } else {
                    &layer.material
                };
                t.add(
                    "Floor Structure",
                    name.clone(),
                    Measure::Area,
                    region.area,
                    room.id,
                );
            }
            if let Some(fd) = &floor.foundation
                && fd.kind != FoundationKind::Crawlspace
                && fd.kind != FoundationKind::Piers
            {
                t.add(
                    "Foundation",
                    "Concrete, slab",
                    Measure::Volume,
                    region.area * fd.slab_thickness.mm(),
                    room.id,
                );
            }
        }
        if let Some(l) = housecraft_geom::piers::layout(doc, floor) {
            for _ in &l.piers {
                t.add(
                    "Foundation",
                    format!("Concrete pier {} dia", fmt(Length::from_mm(l.diameter))),
                    Measure::Count,
                    1.0,
                    floor.id,
                );
            }
            let beams: f64 = l.beams.iter().map(|[a, b]| a.distance(*b)).sum();
            t.add("Foundation", "Beam 6x10", Measure::Length, beams, floor.id);
        }
    }

    // Roofs: sloped area, sheathing, fascia along eaves.
    for r in doc.iter::<RoofPlane>() {
        if r.gable || !on_floor(r.floor) || !shown(r.id) {
            continue;
        }
        let pts: Vec<DVec3> = r
            .outline
            .iter()
            .map(|p| {
                let [x, y, z] = p.mm();
                DVec3::new(x, y, z)
            })
            .collect();
        let area = housecraft_geom::roof::newell(&pts).length() / 2.0;
        t.add("Roofing", "Roofing", Measure::Area, area, r.id);
        t.add("Roofing", "Roof sheathing", Measure::Area, area, r.id);
        let low = pts.iter().map(|p| p.z).fold(f64::MAX, f64::min);
        let eaves: f64 = (0..pts.len())
            .map(|i| (pts[i], pts[(i + 1) % pts.len()]))
            .filter(|(a, b)| (a.z - low).abs() < 1.0 && (b.z - low).abs() < 1.0)
            .map(|(a, b)| a.distance(b))
            .sum();
        t.add("Roofing", "Fascia", Measure::Length, eaves, r.id);
    }

    // Stairs and fixtures.
    for s in doc
        .iter::<Stair>()
        .filter(|s| on_floor(s.floor) && shown(s.id))
    {
        let g = housecraft_geom::stairs::geometry(doc, s);
        t.add(
            "Stairs",
            format!("Stair, {} risers, {} wide", g.risers, fmt(s.width)),
            Measure::Count,
            1.0,
            s.id,
        );
    }
    for e in doc.iter::<Electrical>() {
        if on_floor(e.floor) && shown(e.id) && e.renovation != Renovation::Demolish {
            t.add(
                e.electrical_kind.trade(),
                e.electrical_kind.name(),
                Measure::Count,
                1.0,
                e.id,
            );
        }
    }

    // Site work: paving area and volume, fence and wall runs, plants.
    for f in doc.iter::<SiteFeature>() {
        if !on_floor(f.floor) || !shown(f.id) {
            continue;
        }
        let pts: Vec<DVec2> = f.points.iter().map(|p| v2(*p)).collect();
        let run: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        match &f.feature {
            SiteKind::Road { style, width, thickness, .. } => {
                let area = run * width.mm();
                t.add("Site", format!("{} paving", style.name()), Measure::Area, area, f.id);
                t.add("Site", format!("{} base, {} thick", style.name(), fmt(*thickness)), Measure::Volume, area * thickness.mm(), f.id);
            }
            SiteKind::Fence { style, height, .. } => {
                t.add("Site", format!("{} fence {} high", style.name(), fmt(*height)), Measure::Length, run, f.id);
            }
            SiteKind::RetainingWall { thickness, .. } => {
                t.add("Site", format!("Retaining wall {} thick", fmt(*thickness)), Measure::Length, run, f.id);
            }
            SiteKind::GardenBed { height } => {
                t.add("Site", format!("Garden bed edging {} high", fmt(*height)), Measure::Length, perimeter(&pts), f.id);
            }
            SiteKind::SteppingStones { spacing, size } => {
                let n = (run / spacing.mm().max(1.0)).floor() + 1.0;
                t.add("Site", format!("Stepping stone {}", fmt(*size)), Measure::Count, n, f.id);
            }
            SiteKind::Stream { .. } => {}
        }
    }
    for p in doc.iter::<Plant>().filter(|p| on_floor(p.floor) && shown(p.id)) {
        t.add("Plants", p.name.clone(), Measure::Count, 1.0, p.id);
    }

    let imperial = imperial(unit);
    let mut lines = Vec::new();
    let mut subtotal = 0.0;
    for ((category, item, measure), (qty, ids)) in t.map {
        let (quantity, unit) = convert(measure, qty, imperial);
        // Counts are whole; measured quantities keep one decimal.
        let quantity = if measure == Measure::Count {
            quantity.round()
        } else {
            (quantity * 10.0).round() / 10.0
        };
        let unit_price = doc
            .settings
            .prices
            .get(&price_key(&category, &item))
            .copied()
            .unwrap_or(0.0);
        let total = quantity * unit_price;
        subtotal += total;
        lines.push(MaterialLine {
            category,
            item,
            quantity,
            unit: unit.into(),
            unit_price,
            total,
            ids,
        });
    }
    let markup_percent = doc.settings.markup_percent;
    MaterialsList {
        lines,
        subtotal,
        markup_percent,
        total: subtotal * (1.0 + markup_percent / 100.0),
    }
}

impl MaterialsList {
    /// As a table for display and export (with subtotal and total rows when priced).
    pub fn table(&self) -> ScheduleTable {
        let money = |v: f64| format!("{v:.2}");
        let mut rows: Vec<ScheduleRow> = self
            .lines
            .iter()
            .map(|l| ScheduleRow {
                ids: l.ids.clone(),
                cells: vec![
                    l.category.clone(),
                    l.item.clone(),
                    if l.unit == "ea" {
                        format!("{}", l.quantity)
                    } else {
                        format!("{:.1}", l.quantity)
                    },
                    l.unit.clone(),
                    money(l.unit_price),
                    money(l.total),
                ],
            })
            .collect();
        if self.subtotal > 0.0 {
            let row = |label: &str, value: f64| ScheduleRow {
                ids: Vec::new(),
                cells: vec![
                    String::new(),
                    label.into(),
                    String::new(),
                    String::new(),
                    String::new(),
                    money(value),
                ],
            };
            rows.push(row("Subtotal", self.subtotal));
            rows.push(row(
                &format!("Markup {}%", self.markup_percent),
                self.total - self.subtotal,
            ));
            rows.push(row("Total", self.total));
        }
        ScheduleTable {
            title: "Materials List".into(),
            columns: ["Category", "Item", "Qty", "Unit", "Unit Price", "Total"]
                .iter()
                .map(|c| c.to_string())
                .collect(),
            rows,
        }
    }
}
