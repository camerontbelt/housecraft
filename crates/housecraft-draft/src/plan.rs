//! Floor plan drawing.

use housecraft_geom::math::{arc_points, perp, shape_polylines};
use housecraft_geom::walls::{DoorSwing, WallGeom, opening_spans, wall_geoms};
use housecraft_geom::{DVec2, rooms, v2};
use housecraft_model::*;
use housecraft_units::format_area;

use crate::dims::{DimStyle, draw_dimension};
use crate::{Drawing, Fill, Ink, Prim, Stroke};

#[derive(Debug, Clone)]
pub struct PlanOptions {
    pub floor: Id,
    pub layer_set: Id,
    /// Draw reference floors, faded: `references`, or else the floor below.
    pub reference_floor: bool,
    pub references: Vec<Id>,
    /// Remodel phase to show.
    pub phase: Phase,
    /// Draw the building's sun shadows on the ground (lowest storey only).
    pub sun_shadows: bool,
    /// Default set used for dimension and text defaults; `None` = active.
    pub default_set: Option<Id>,
}

impl PlanOptions {
    /// Resolve a floor and layer set given by name or numeric id (`None` = first).
    pub fn resolve(
        doc: &Document,
        floor: Option<&str>,
        layer_set: Option<&str>,
    ) -> Result<Self, String> {
        fn find<T: ObjectData + Named>(
            doc: &Document,
            key: &str,
            what: &str,
        ) -> Result<Id, String> {
            let by_id = key
                .trim_start_matches('#')
                .parse::<u64>()
                .ok()
                .and_then(|n| doc.get::<T>(Id(n)));
            by_id
                .or_else(|| doc.find_named::<T>(key))
                .map(ObjectData::id)
                .ok_or_else(|| {
                    let names: Vec<&str> = doc.iter::<T>().map(Named::name).collect();
                    format!("no {what} {key:?} (have: {})", names.join(", "))
                })
        }
        let mut opts = Self::first(doc).ok_or("the document has no floors or layer sets")?;
        if let Some(f) = floor {
            opts.floor = find::<Floor>(doc, f, "floor")?;
        }
        if let Some(s) = layer_set {
            opts.layer_set = find::<LayerSet>(doc, s, "layer set")?;
        }
        Ok(opts)
    }

    /// The first floor with the first layer set and the active default set.
    pub fn first(doc: &Document) -> Option<Self> {
        Some(PlanOptions {
            floor: doc.floors_by_elevation().first()?.id,
            layer_set: doc.iter::<LayerSet>().next()?.id,
            reference_floor: false,
            references: Vec::new(),
            phase: Phase::All,
            sun_shadows: false,
            default_set: None,
        })
    }

    /// The reference floors to draw (none when reference display is off).
    pub fn reference_floor_ids(&self, doc: &Document) -> Vec<Id> {
        if !self.reference_floor {
            return Vec::new();
        }
        let chosen: Vec<Id> = self
            .references
            .iter()
            .copied()
            .filter(|r| *r != self.floor && doc.get::<Floor>(*r).is_some())
            .collect();
        if chosen.is_empty() {
            doc.floor_below(self.floor)
                .map(|f| f.id)
                .into_iter()
                .collect()
        } else {
            chosen
        }
    }
}

#[derive(Clone, Copy)]
struct Look {
    visible: bool,
    ink: Ink,
    stroke: Stroke,
    text_style: Option<Id>,
}

struct Ctx<'a> {
    doc: &'a Document,
    set: Option<&'a LayerSet>,
    defaults: &'a DefaultSet,
}

impl Ctx<'_> {
    fn look(&self, layer: Id) -> Look {
        let l = self.doc.get::<Layer>(layer);
        let d = self.set.map(|s| s.display(layer)).unwrap_or_default();
        let color = d.color.or(l.and_then(|l| l.color));
        let ink = color.map_or(Ink::Default, Ink::Color);
        Look {
            visible: d.visible,
            ink,
            stroke: Stroke {
                ink,
                width: d.line_weight.or(l.map(|l| l.line_weight)).unwrap_or(0.25),
                dash: d
                    .line_style
                    .or(l.map(|l| l.line_style))
                    .unwrap_or_default()
                    .pattern(),
            },
            text_style: l.and_then(|l| l.text_style),
        }
    }

    fn text_style(&self, explicit: Option<Id>, look: &Look) -> TextStyle {
        explicit
            .or(look.text_style)
            .and_then(|s| self.doc.get::<TextStyle>(s))
            .or_else(|| self.doc.get::<TextStyle>(self.defaults.text_style))
            .cloned()
            .unwrap_or(TextStyle {
                id: Id(0),
                name: String::new(),
                height: housecraft_units::Length::from_inches(6.0),
                font: "Sans".into(),
                bold: false,
                italic: false,
                color: None,
            })
    }
}

pub fn plan(doc: &Document, opts: &PlanOptions) -> Drawing {
    let defaults = opts
        .default_set
        .and_then(|d| doc.get::<DefaultSet>(d))
        .unwrap_or_else(|| doc.active_defaults());
    let ctx = Ctx {
        doc,
        set: doc.get::<LayerSet>(opts.layer_set),
        defaults,
    };
    let mut out = Drawing::default();

    if opts.sun_shadows {
        sun_shadows(doc, opts.floor, &mut out);
    }

    for reference in opts.reference_floor_ids(doc) {
        for g in wall_geoms(doc, reference) {
            out.push(
                None,
                Prim::Polyline {
                    points: g.outline(),
                    closed: true,
                    stroke: Stroke {
                        ink: Ink::Faded,
                        width: 0.18,
                        dash: &[],
                    },
                },
            );
        }
    }

    let geoms = wall_geoms(doc, opts.floor);
    for g in &geoms {
        let Some(wall) = doc.get::<Wall>(g.id) else {
            continue;
        };
        let look = ctx.look(wall.layer);
        if !look.visible || !opts.phase.shows(wall.renovation) {
            continue;
        }
        // Openings outside the phase are filled in with wall.
        let openings: Vec<&Opening> = doc
            .openings_in(g.id)
            .filter(|o| opts.phase.shows(o.renovation))
            .collect();
        let demolish = wall.renovation == Renovation::Demolish;
        let look = if demolish {
            Look {
                stroke: Stroke {
                    dash: &[1.5, 1.0],
                    ..look.stroke
                },
                ..look
            }
        } else {
            look
        };
        // Proposed plans fill new walls solid so new work stands out from
        // the existing (grey) walls, as remodel drawings do.
        let fill = if demolish {
            None
        } else if opts.phase == Phase::Proposed && wall.renovation == Renovation::New {
            Some(Fill::Ink(look.ink))
        } else {
            Some(Fill::Poche)
        };
        let railing = doc.get::<WallType>(wall.wall_type).and_then(|t| t.railing);
        if let Some(r) = railing {
            // Railings: outline only, with a dot at each baluster.
            let len = (g.end - g.start).length();
            let spacing = r.baluster_spacing.mm().max(25.0);
            let n = (len / spacing).floor() as usize;
            let off = (len - n as f64 * spacing) / 2.0;
            for i in 0..=n {
                let c = g.start + g.dir * (off + i as f64 * spacing);
                out.push(
                    Some(g.id),
                    Prim::Polygon {
                        points: arc_points(c, 12.0, 0.0, 360.0),
                        fill: Fill::Ink(look.ink),
                    },
                );
            }
        }
        if let (None, Some(fill)) = (railing, fill) {
            for piece in g.pieces(&openings) {
                out.push(
                    Some(g.id),
                    Prim::Polygon {
                        points: piece,
                        fill,
                    },
                );
            }
        }
        if let Some(f) = wall.footing {
            let hw = f.width.mm() / 2.0;
            let (a, b) = (g.start - g.dir * hw, g.end + g.dir * hw);
            let n = g.left * hw;
            out.push(
                Some(g.id),
                Prim::Polyline {
                    points: vec![a - n, b - n, b + n, a + n],
                    closed: true,
                    stroke: Stroke {
                        width: look.stroke.width * 0.5,
                        dash: &[2.0, 1.2],
                        ..look.stroke
                    },
                },
            );
        }
        for [a, b] in g.outline_segments(&openings) {
            out.push(
                Some(g.id),
                Prim::Polyline {
                    points: vec![a, b],
                    closed: false,
                    stroke: look.stroke,
                },
            );
        }
    }

    for g in &geoms {
        let wall_shown = doc
            .get::<Wall>(g.id)
            .is_some_and(|w| opts.phase.shows(w.renovation));
        for o in doc.openings_in(g.id) {
            let look = ctx.look(o.layer);
            if look.visible && wall_shown && opts.phase.shows(o.renovation) {
                let stroke = if o.renovation == Renovation::Demolish {
                    Stroke {
                        dash: &[1.5, 1.0],
                        ..look.stroke
                    }
                } else {
                    look.stroke
                };
                opening_symbol(g, o, stroke, &mut out);
            }
        }
    }

    if let Some(floor) = doc.get::<Floor>(opts.floor)
        && let Some(l) = housecraft_geom::piers::layout(doc, floor)
    {
        let look = ctx.look(ctx.defaults.layers.walls);
        let dashed = Stroke {
            dash: &[2.0, 1.2],
            width: look.stroke.width * 0.5,
            ..look.stroke
        };
        for [a, b] in &l.beams {
            let n =
                perp((*b - *a).normalize_or_zero()) * (housecraft_geom::piers::BEAM_WIDTH / 2.0);
            out.push(
                None,
                Prim::Polyline {
                    points: vec![*a - n, *b - n, *b + n, *a + n],
                    closed: true,
                    stroke: dashed,
                },
            );
        }
        for p in &l.piers {
            out.push(
                None,
                Prim::Polygon {
                    points: arc_points(*p, l.diameter / 2.0, 0.0, 360.0),
                    fill: Fill::Poche,
                },
            );
            let h = l.footing / 2.0;
            out.push(
                None,
                Prim::Polyline {
                    points: vec![
                        *p + DVec2::new(-h, -h),
                        *p + DVec2::new(h, -h),
                        *p + DVec2::new(h, h),
                        *p + DVec2::new(-h, h),
                    ],
                    closed: true,
                    stroke: dashed,
                },
            );
        }
    }

    let regions = rooms::detect(doc, opts.floor);
    for room in doc.rooms_on(opts.floor) {
        let look = ctx.look(room.layer);
        if !look.visible {
            continue;
        }
        let style = ctx.text_style(None, &look);
        let h = style.height.mm();
        let at = v2(room.anchor);
        let ink = style.color.map_or(look.ink, Ink::Color);
        out.push(
            Some(room.id),
            Prim::Text {
                at: at + DVec2::Y * h * 0.15,
                text: doc.room_name(room),
                height: h,
                rotation: 0.0,
                justify: Justify::Center,
                middle: false,
                bold: style.bold,
                ink,
            },
        );
        if let Some(i) = regions.room_at(at) {
            out.push(
                Some(room.id),
                Prim::Text {
                    at: at - DVec2::Y * h * 1.1,
                    text: format_area(regions.rooms[i].area, doc.settings.display_unit),
                    height: h * 0.7,
                    rotation: 0.0,
                    justify: Justify::Center,
                    middle: false,
                    bold: false,
                    ink,
                },
            );
        }
    }

    for cad in doc.iter::<Cad>().filter(|c| c.floor == opts.floor) {
        let look = ctx.look(cad.layer);
        if !look.visible {
            continue;
        }
        let ink = cad.color.map_or(look.ink, Ink::Color);
        let stroke = Stroke { ink, ..look.stroke };
        for (points, closed) in shape_polylines(&cad.shape) {
            if let (Some(fill), true) = (cad.fill, closed) {
                out.push(
                    Some(cad.id),
                    Prim::Polygon {
                        points: points.clone(),
                        fill: Fill::Ink(Ink::Color(fill)),
                    },
                );
            }
            if points.len() == 1 {
                // A point: small cross.
                let p = points[0];
                let s = 75.0;
                for d in [DVec2::new(s, s), DVec2::new(s, -s)] {
                    out.push(
                        Some(cad.id),
                        Prim::Polyline {
                            points: vec![p - d, p + d],
                            closed: false,
                            stroke,
                        },
                    );
                }
            } else {
                out.push(
                    Some(cad.id),
                    Prim::Polyline {
                        points,
                        closed,
                        stroke,
                    },
                );
            }
        }
        if let Shape::Line {
            a,
            b,
            arrow_start,
            arrow_end,
        } = &cad.shape
        {
            let (a, b) = (v2(*a), v2(*b));
            let size = 150.0;
            for (on, tip, from) in [(*arrow_start, a, b), (*arrow_end, b, a)] {
                if on {
                    let dir = (from - tip).normalize_or_zero();
                    let back = tip + dir * size;
                    let side = perp(dir) * size * 0.35;
                    out.push(
                        Some(cad.id),
                        Prim::Polygon {
                            points: vec![tip, back + side, back - side],
                            fill: Fill::Ink(ink),
                        },
                    );
                }
            }
        }
    }

    for plane in doc.iter::<RoofPlane>().filter(|r| r.floor == opts.floor) {
        let look = ctx.look(plane.layer);
        if !look.visible {
            continue;
        }
        let points: Vec<DVec2> = plane
            .outline
            .iter()
            .map(|p| {
                let [x, y, _] = p.mm();
                DVec2::new(x, y)
            })
            .collect();
        if points.len() < 3 {
            continue;
        }
        let area = housecraft_geom::math::signed_area(&points).abs();
        if area < 1.0 {
            continue; // vertical gable infill has no plan footprint
        }
        out.push(
            Some(plane.id),
            Prim::Polyline {
                points: points.clone(),
                closed: true,
                stroke: look.stroke,
            },
        );
        let rise = plane.pitch.to_radians().tan() * 12.0;
        let label = if (rise - rise.round()).abs() < 0.05 {
            format!("{}/12", rise.round())
        } else {
            format!("{rise:.1}/12")
        };
        let style = ctx.text_style(None, &look);
        out.push(
            Some(plane.id),
            Prim::Text {
                at: housecraft_geom::math::interior_point(&points),
                text: label,
                height: style.height.mm() * 0.8,
                rotation: 0.0,
                justify: Justify::Center,
                middle: true,
                bold: false,
                ink: look.ink,
            },
        );
    }

    for stair in doc.iter::<Stair>() {
        let here = stair.floor == opts.floor;
        let below = doc
            .floor_above(stair.floor)
            .is_some_and(|f| f.id == opts.floor);
        if !here && !below {
            continue;
        }
        let look = ctx.look(stair.layer);
        if look.visible {
            let style = ctx.text_style(None, &look);
            stair_symbol(doc, stair, here, look.stroke, style.height.mm(), &mut out);
        }
    }
    let switch_labels = doc.switch_labels();
    for e in doc.iter::<Electrical>().filter(|e| e.floor == opts.floor) {
        let look = ctx.look(e.layer);
        if !look.visible || !opts.phase.shows(e.renovation) {
            continue;
        }
        let label = switch_labels.get(&e.id).map(String::as_str);
        electrical_symbol(e, label, look.stroke, &mut out);
        // Switch legs: a dashed curve to each fixture it controls.
        for target in e.controls.iter().filter_map(|t| doc.get::<Electrical>(*t)) {
            if target.floor != opts.floor {
                continue;
            }
            let (a, b) = (v2(e.position), v2(target.position));
            let mid = (a + b) / 2.0 + perp((b - a).normalize_or_zero()) * a.distance(b) * 0.2;
            let points: Vec<DVec2> = (0..=16)
                .map(|i| {
                    let t = i as f64 / 16.0;
                    a * (1.0 - t) * (1.0 - t) + mid * 2.0 * t * (1.0 - t) + b * t * t
                })
                .collect();
            out.push(
                Some(e.id),
                Prim::Polyline {
                    points,
                    closed: false,
                    stroke: Stroke {
                        width: look.stroke.width * 0.6,
                        dash: &[2.0, 1.2],
                        ..look.stroke
                    },
                },
            );
        }
    }
    if let Some(at) = doc.settings.north_pointer {
        north_pointer(v2(at), doc.settings.north, &mut out);
    }

    for v in doc.iter::<SectionView>().filter(|v| v.floor == opts.floor) {
        let look = ctx.look(v.layer);
        if look.visible {
            let style = ctx.text_style(None, &look);
            section_callout(v, look.stroke, style.height.mm(), &mut out);
        }
    }

    for s in doc.iter::<Schedule>().filter(|s| s.floor == opts.floor) {
        let look = ctx.look(s.layer);
        if look.visible {
            let style = ctx.text_style(None, &look);
            let h = s.text_height.unwrap_or(style.height).mm();
            let ink = style.color.map_or(look.ink, Ink::Color);
            schedule_table(doc, s, h, look.stroke, ink, &mut out);
        }
    }

    for dim in doc.iter::<Dimension>().filter(|d| d.floor == opts.floor) {
        let look = ctx.look(dim.layer);
        if !look.visible {
            continue;
        }
        let ts = ctx
            .doc
            .get::<TextStyle>(defaults.dimensions.text_style)
            .cloned()
            .unwrap_or_else(|| ctx.text_style(None, &look));
        let style = DimStyle {
            stroke: look.stroke,
            ink: ts.color.map_or(look.ink, Ink::Color),
            text_height: ts.height.mm(),
            arrow: defaults.dimensions.arrow,
            gap: defaults.dimensions.extension_gap.mm(),
            overshoot: defaults.dimensions.extension_overshoot.mm(),
        };
        draw_dimension(doc, defaults, dim, &style, &mut out);
    }

    for text in doc.iter::<Text>().filter(|t| t.floor == opts.floor) {
        let look = ctx.look(text.layer);
        if !look.visible {
            continue;
        }
        let style = ctx.text_style(text.style, &look);
        out.push(
            Some(text.id),
            Prim::Text {
                at: v2(text.position),
                text: text.text.clone(),
                height: text.height.unwrap_or(style.height).mm(),
                rotation: text.rotation,
                justify: text.justify,
                middle: false,
                bold: text.bold.unwrap_or(style.bold),
                ink: text.color.or(style.color).map_or(look.ink, Ink::Color),
            },
        );
    }
    out
}

/// Shadows the walls and roof cast on the ground at the document's sun
/// position, drawn under the plan of the lowest storey and masked by its
/// footprint (the building's own shade inside is not shown).
fn sun_shadows(doc: &Document, floor: Id, out: &mut Drawing) {
    use housecraft_geom::mesh::Category;
    let lowest = doc
        .floors_by_elevation()
        .into_iter()
        .find(|f| f.foundation.is_none());
    if lowest.map(|f| f.id) != Some(floor) {
        return;
    }
    let sun = housecraft_geom::sun::direction(doc);
    let Some(ground) = housecraft_geom::sun::grade(doc) else {
        return;
    };
    // Very low sun throws shadows off the page; 5° keeps them bounded.
    if sun.z < 5f64.to_radians().sin() || !doc.settings.sun.shadows {
        return;
    }
    let mesh = housecraft_geom::mesh::build(doc, None);
    let project = |i: u32| {
        let [x, y, z] = mesh.positions[i as usize];
        let p = glam::DVec3::new(x as f64, y as f64, z as f64) * 1000.0;
        let h = (p.z - ground).max(0.0);
        (p.truncate() - sun.truncate() * (h / sun.z), h)
    };
    for part in mesh.parts.iter().filter(|p| {
        matches!(
            p.category,
            Category::Wall | Category::Roof | Category::Foundation
        )
    }) {
        let range = part.first_index as usize..(part.first_index + part.index_count) as usize;
        for t in mesh.indices[range].chunks(3) {
            let pts = [project(t[0]), project(t[1]), project(t[2])];
            if pts.iter().all(|(_, h)| *h < 1.0) {
                continue;
            }
            let poly: Vec<DVec2> = pts.iter().map(|(p, _)| *p).collect();
            if housecraft_geom::math::signed_area(&poly).abs() < 1.0 {
                continue;
            }
            out.push(
                None,
                Prim::Polygon {
                    points: poly,
                    fill: Fill::Shadow,
                },
            );
        }
    }
    for fp in rooms::detect(doc, floor).footprints {
        out.push(
            None,
            Prim::Polygon {
                points: fp,
                fill: Fill::Paper,
            },
        );
    }
}

/// Draw a schedule as a ruled table hanging down-right from its position.
fn schedule_table(
    doc: &Document,
    s: &Schedule,
    h: f64,
    stroke: Stroke,
    ink: Ink,
    out: &mut Drawing,
) {
    let table = housecraft_api::schedule::schedule(
        doc,
        s.schedule_kind,
        &housecraft_api::schedule::ScheduleOptions {
            floor: (!s.all_floors).then_some(s.floor),
            columns: s.columns.clone(),
        },
    );
    let id = Some(s.id);
    let pad = h * 0.6;
    let row_h = h * 2.0;
    // The shared width estimate is tuned for labels; table text, and the bold
    // header especially, renders wider, so allow a margin.
    let text_w =
        |t: &str, bold: bool| crate::approx_text_width(t, h) * if bold { 1.4 } else { 1.3 };
    let widths: Vec<f64> = (0..table.columns.len())
        .map(|i| {
            table
                .rows
                .iter()
                .map(|r| text_w(&r.cells[i], false))
                .chain(std::iter::once(text_w(&table.columns[i], true)))
                .fold(0.0, f64::max)
                + 2.0 * pad
        })
        .collect();
    let width: f64 = widths
        .iter()
        .sum::<f64>()
        .max(crate::approx_text_width(&table.title, h * 1.3) + 2.0 * pad);
    let rows = table.rows.len() + 2; // title and header
    let height = rows as f64 * row_h;
    let o = v2(s.position);
    let at = |x: f64, y: f64| o + DVec2::new(x, -y);
    let line = |out: &mut Drawing, a: DVec2, b: DVec2, stroke: Stroke| {
        out.push(
            id,
            Prim::Polyline {
                points: vec![a, b],
                closed: false,
                stroke,
            },
        );
    };
    out.push(
        id,
        Prim::Polyline {
            points: vec![
                at(0.0, 0.0),
                at(width, 0.0),
                at(width, height),
                at(0.0, height),
            ],
            closed: true,
            stroke,
        },
    );
    let thin = Stroke {
        width: stroke.width * 0.6,
        ..stroke
    };
    for r in 1..rows {
        line(
            out,
            at(0.0, r as f64 * row_h),
            at(width, r as f64 * row_h),
            if r <= 2 { stroke } else { thin },
        );
    }
    let mut x = 0.0;
    for w in &widths[..widths.len().saturating_sub(1)] {
        x += w;
        line(out, at(x, row_h), at(x, height), thin);
    }
    let text = |out: &mut Drawing, p: DVec2, t: &str, height: f64, bold: bool, justify: Justify| {
        out.push(
            id,
            Prim::Text {
                at: p,
                text: t.to_string(),
                height,
                rotation: 0.0,
                justify,
                middle: true,
                bold,
                ink,
            },
        );
    };
    text(
        out,
        at(width / 2.0, row_h / 2.0),
        &table.title,
        h * 1.3,
        true,
        Justify::Center,
    );
    let cells = std::iter::once(&table.columns).chain(table.rows.iter().map(|r| &r.cells));
    for (r, row) in cells.enumerate() {
        let y = (r as f64 + 1.5) * row_h;
        let mut x = 0.0;
        for (c, cell) in row.iter().enumerate() {
            text(out, at(x + pad, y), cell, h, r == 0, Justify::Left);
            x += widths[c];
        }
    }
}

/// Plan callout for an elevation / section view: its line (dashed for a
/// cut), a marker whose arrow points the way the view looks, and its name.
fn section_callout(v: &SectionView, stroke: Stroke, text: f64, out: &mut Drawing) {
    let id = Some(v.id);
    let (a, b) = (v2(v.a), v2(v.b));
    let dir = (b - a).normalize_or_zero();
    let look = perp(dir);
    let line = Stroke {
        dash: if v.section_kind == SectionKind::Section {
            &[3.0, 1.0, 0.6, 1.0]
        } else {
            &[]
        },
        ..stroke
    };
    out.push(
        id,
        Prim::Polyline {
            points: vec![a, b],
            closed: false,
            stroke: line,
        },
    );
    let r = text * 1.6;
    // Markers at both ends for a section, the middle for an elevation.
    let marks = if v.section_kind == SectionKind::Section {
        vec![a, b]
    } else {
        vec![(a + b) / 2.0]
    };
    for c in marks {
        out.push(
            id,
            Prim::Polyline {
                points: arc_points(c, r, 0.0, 360.0),
                closed: true,
                stroke,
            },
        );
        let side = dir * r * 0.9;
        out.push(
            id,
            Prim::Polygon {
                points: vec![c + look * r * 1.9, c + side, c - side],
                fill: Fill::Ink(stroke.ink),
            },
        );
    }
    let mid = (a + b) / 2.0;
    out.push(
        id,
        Prim::Text {
            at: mid - look * r * 2.0,
            text: v.name.clone(),
            height: text * 0.8,
            rotation: 0.0,
            justify: Justify::Center,
            middle: true,
            bold: false,
            ink: stroke.ink,
        },
    );
}

/// A north pointer: a circle with a half-filled arrow and an "N" past its tip.
fn north_pointer(c: DVec2, north: f64, out: &mut Drawing) {
    let r = 450.0;
    let stroke = Stroke {
        ink: Ink::Default,
        width: 0.25,
        dash: &[],
    };
    let d = DVec2::from_angle(north.to_radians());
    let s = perp(d);
    let (tip, tail) = (c + d * r * 1.15, c - d * r * 0.8);
    out.push(
        None,
        Prim::Polyline {
            points: arc_points(c, r, 0.0, 360.0),
            closed: true,
            stroke,
        },
    );
    out.push(
        None,
        Prim::Polygon {
            points: vec![tip, tail, tail + s * r * 0.45],
            fill: Fill::Ink(Ink::Default),
        },
    );
    out.push(
        None,
        Prim::Polyline {
            points: vec![tip, tail - s * r * 0.45, tail, tail + s * r * 0.45, tip],
            closed: true,
            stroke,
        },
    );
    out.push(
        None,
        Prim::Text {
            at: c + d * r * 1.55,
            text: "N".into(),
            height: r * 0.5,
            rotation: 0.0,
            justify: Justify::Center,
            middle: true,
            bold: true,
            ink: Ink::Default,
        },
    );
}
/// Plan symbol for an electrical device, drawn about its position. Wall
/// devices are drawn just inside the room, facing `rotation`.
fn electrical_symbol(
    e: &Electrical,
    switch_label: Option<&str>,
    stroke: Stroke,
    out: &mut Drawing,
) {
    use ElectricalKind as K;
    let id = Some(e.id);
    let p = v2(e.position);
    let n = DVec2::from_angle(e.rotation.to_radians());
    let t = perp(n);
    let line = |out: &mut Drawing, a: DVec2, b: DVec2| {
        out.push(
            id,
            Prim::Polyline {
                points: vec![a, b],
                closed: false,
                stroke,
            },
        );
    };
    let circle = |out: &mut Drawing, c: DVec2, r: f64| {
        out.push(
            id,
            Prim::Polyline {
                points: arc_points(c, r, 0.0, 360.0),
                closed: true,
                stroke,
            },
        );
    };
    let text = |out: &mut Drawing, at: DVec2, s: &str, h: f64| {
        out.push(
            id,
            Prim::Text {
                at,
                text: s.into(),
                height: h,
                rotation: 0.0,
                justify: Justify::Center,
                middle: true,
                bold: false,
                ink: stroke.ink,
            },
        );
    };
    let r = 100.0;
    match e.electrical_kind {
        K::Outlet | K::GfciOutlet | K::Outlet240 => {
            let c = p + n * r;
            circle(out, c, r);
            // Prongs: two lines parallel to the wall (three for 240 V).
            let prongs: &[f64] = if e.electrical_kind == K::Outlet240 {
                &[-0.5, 0.0, 0.5]
            } else {
                &[-0.35, 0.35]
            };
            for k in prongs {
                let o = n * (r * k);
                line(out, c + o - t * r * 1.5, c + o + t * r * 1.5);
            }
            if e.electrical_kind == K::GfciOutlet {
                text(out, c + n * r * 2.2, "GFI", r * 1.1);
            }
        }
        K::Switch | K::Dimmer => {
            let c = p + n * r * 2.0;
            line(out, p, p + n * r);
            text(out, c, switch_label.unwrap_or("S"), r * 1.6);
        }
        K::WallLight => {
            let c = p + n * r * 0.9;
            circle(out, c, r * 0.9);
            line(out, p - t * r * 1.2, p + t * r * 1.2);
            line(
                out,
                c - t * r * 0.6 - n * r * 0.6,
                c + t * r * 0.6 + n * r * 0.6,
            );
            line(
                out,
                c + t * r * 0.6 - n * r * 0.6,
                c - t * r * 0.6 + n * r * 0.6,
            );
        }
        K::CeilingLight => {
            circle(out, p, r * 1.5);
            for d in [DVec2::new(1.0, 1.0), DVec2::new(1.0, -1.0)] {
                let d = d.normalize() * r * 1.5;
                line(out, p - d, p + d);
            }
        }
        K::RecessedLight => {
            circle(out, p, r * 1.2);
            circle(out, p, r * 0.6);
        }
        K::CeilingFan => {
            circle(out, p, r * 0.8);
            for i in 0..4 {
                let d = DVec2::from_angle(
                    e.rotation.to_radians() + i as f64 * std::f64::consts::FRAC_PI_2,
                );
                let q = perp(d) * r * 0.6;
                let (a, b) = (p + d * r * 0.8, p + d * r * 4.5);
                out.push(
                    id,
                    Prim::Polyline {
                        points: vec![a - q * 0.5, b - q, b + q, a + q * 0.5],
                        closed: true,
                        stroke,
                    },
                );
            }
        }
        K::SmokeDetector => {
            circle(out, p, r * 1.4);
            text(out, p, "SD", r * 1.1);
        }
        K::JunctionBox => {
            circle(out, p, r * 1.2);
            text(out, p, "J", r * 1.4);
        }
        K::HoseBibb | K::GasOutlet | K::Thermostat => {
            let c = p + n * r;
            circle(out, c, r);
            let label = match e.electrical_kind {
                K::HoseBibb => "HB",
                K::GasOutlet => "G",
                _ => "T",
            };
            text(out, c + n * r * 2.0, label, r * 1.1);
        }
        K::FloorDrain => {
            circle(out, p, r * 1.0);
            for k in [-0.5, 0.0, 0.5] {
                let o = DVec2::Y * r * k;
                let w = r * (1.0 - k * k).sqrt();
                line(out, p + o - DVec2::X * w, p + o + DVec2::X * w);
            }
            text(out, p - DVec2::Y * r * 2.0, "FD", r * 1.0);
        }
        K::SupplyRegister | K::ExhaustFan => {
            let (w, d) = if e.electrical_kind == K::ExhaustFan {
                (r * 2.4, r * 2.4)
            } else {
                (r * 2.6, r * 1.1)
            };
            let (u, v) = (n * (w / 2.0), t * (d / 2.0));
            let pts = vec![p - u - v, p + u - v, p + u + v, p - u + v];
            out.push(
                id,
                Prim::Polyline {
                    points: pts.clone(),
                    closed: true,
                    stroke,
                },
            );
            line(out, pts[0], pts[2]);
            line(out, pts[1], pts[3]);
            if e.electrical_kind == K::ExhaustFan {
                text(out, p + n * r * 2.2, "EF", r * 1.0);
            }
        }
        K::ReturnGrille => {
            let (u, v) = (t * r * 2.5, n * r * 0.5);
            let c = p + n * r * 0.6;
            out.push(
                id,
                Prim::Polyline {
                    points: vec![c - u - v, c + u - v, c + u + v, c - u + v],
                    closed: true,
                    stroke,
                },
            );
            text(out, c + n * r * 1.8, "RA", r * 1.0);
        }
    }
}

/// Height above the floor where the plan cut line crosses a stair.
const STAIR_CUT: f64 = 48.0 * 25.4;

/// A stair in plan: treads, the cut line (on its own floor, treads above
/// the cut dashed), and the walk line with an UP or DN arrow.
fn stair_symbol(
    doc: &Document,
    stair: &Stair,
    here: bool,
    stroke: Stroke,
    text: f64,
    out: &mut Drawing,
) {
    let g = housecraft_geom::stairs::geometry(doc, stair);
    let dashed = Stroke {
        dash: &[1.5, 1.0],
        ..stroke
    };
    let thin = Stroke {
        width: stroke.width * 0.6,
        ..stroke
    };
    let cut_riser = (STAIR_CUT / g.riser_height).ceil() as u32;
    for step in &g.steps {
        let beyond = here && step.index > cut_riser;
        out.push(
            Some(stair.id),
            Prim::Polyline {
                points: step.poly.clone(),
                closed: true,
                stroke: if beyond { dashed } else { thin },
            },
        );
        // Overhang dash: the nosing's front edge ahead of the riser line.
        if let [p0, _, _, p3] = step.nosing[..] {
            out.push(
                Some(stair.id),
                Prim::Polyline {
                    points: vec![p0, p3],
                    closed: false,
                    stroke: Stroke {
                        width: thin.width * 0.7,
                        dash: &[0.8, 0.6],
                        ..thin
                    },
                },
            );
        }
    }
    // Break line: a zig-zag across the tread at the cut.
    if here && let Some(step) = g.steps.iter().rfind(|s| s.index <= cut_riser && !s.landing) {
        let p = &step.poly;
        // Corner to corner across the tread, so the slant reads at any scale.
        let (a, b) = (p[0], p[2]);
        let n = perp((b - a).normalize_or_zero()) * 90.0;
        out.push(
            Some(stair.id),
            Prim::Polyline {
                points: vec![a, a.lerp(b, 0.45) + n, a.lerp(b, 0.55) - n, b],
                closed: false,
                stroke,
            },
        );
    }
    // Walk line and arrow.
    // From the floor above the arrow points down the stair.
    let mut walk: Vec<DVec2> = g.walk.iter().map(|p| p.truncate()).collect();
    if !here {
        walk.reverse();
    }
    if walk.len() >= 2 {
        let (tip, from) = (walk[walk.len() - 1], walk[walk.len() - 2]);
        let dir = (tip - from).normalize_or_zero();
        let tip = tip - dir * 100.0;
        let mut line = walk.clone();
        let last = line.len() - 1;
        line[last] = tip;
        out.push(
            Some(stair.id),
            Prim::Polyline {
                points: line,
                closed: false,
                stroke: thin,
            },
        );
        let back = tip - dir * 180.0;
        let side = perp(dir) * 70.0;
        out.push(
            Some(stair.id),
            Prim::Polygon {
                points: vec![tip, back + side, back - side],
                fill: Fill::Ink(stroke.ink),
            },
        );
        let dir0 = (walk[1] - walk[0]).normalize_or_zero();
        out.push(
            Some(stair.id),
            Prim::Text {
                at: walk[0] - dir0 * text * 1.4,
                text: if here { "UP" } else { "DN" }.into(),
                height: text * 0.8,
                rotation: 0.0,
                justify: Justify::Center,
                middle: true,
                bold: true,
                ink: stroke.ink,
            },
        );
    }
}

/// Plan symbol for a door or window in its wall gap.
/// Plan symbol for a door or window in its wall gap.
fn opening_symbol(g: &WallGeom, o: &Opening, stroke: Stroke, out: &mut Drawing) {
    let Some(&(a, b)) = opening_spans(g, &[o]).first() else {
        return;
    };
    let id = Some(o.id);
    let thin = Stroke {
        width: stroke.width * 0.7,
        ..stroke
    };
    let mut line = |p: DVec2, q: DVec2, s: Stroke| {
        out.push(
            id,
            Prim::Polyline {
                points: vec![p, q],
                closed: false,
                stroke: s,
            },
        );
    };
    match o.opening_kind {
        OpeningKind::Window(_) => {
            // Sill lines on both faces and a double glass line in the middle.
            line(g.left_at(a), g.left_at(b), thin);
            line(g.right_at(a), g.right_at(b), thin);
            let off = g.left * (g.half * 0.15).min(12.0);
            line(g.center_at(a) + off, g.center_at(b) + off, stroke);
            line(g.center_at(a) - off, g.center_at(b) - off, stroke);
        }
        OpeningKind::Door(style) => {
            let swing = DoorSwing::new(g, o, a, b);
            let s = swing.side;
            let face = |u: f64| swing.face(g, u);
            let (hinge_u, free_u) = (swing.hinge_u, swing.free_u);
            let swing_angle = o.plan_swing.to_radians();
            let leaf = |out_line: &mut dyn FnMut(DVec2, DVec2, Stroke), hu: f64, fu: f64| {
                let h = face(hu);
                let width = (fu - hu).abs();
                let closed = (face(fu) - h).normalize_or_zero();
                let tip = h + (closed * swing_angle.cos() + s * swing_angle.sin()) * width;
                out_line(h, tip, stroke);
                let start = (tip - h).to_angle().to_degrees();
                let end = (face(fu) - h).to_angle().to_degrees();
                let mut sweep = end - start;
                if sweep > 180.0 {
                    sweep -= 360.0;
                } else if sweep < -180.0 {
                    sweep += 360.0;
                }
                let arc = arc_points(h, width, start, sweep);
                for w in arc.windows(2) {
                    out_line(
                        w[0],
                        w[1],
                        Stroke {
                            dash: &[1.0, 0.8],
                            ..thin
                        },
                    );
                }
            };
            match style {
                DoorStyle::Hinged | DoorStyle::Shower => leaf(&mut line, hinge_u, free_u),
                DoorStyle::Atrium => {
                    // The active leaf swings from the hinge jamb; the other half is fixed.
                    let mid = (a + b) / 2.0;
                    leaf(&mut line, hinge_u, mid);
                    let fixed = if hinge_u == a { b } else { a };
                    line(face(fixed), face(mid), stroke);
                }
                DoorStyle::Fixed => {
                    let off = g.left * (g.half * 0.15).min(12.0);
                    line(g.center_at(a) + off, g.center_at(b) + off, stroke);
                    line(g.center_at(a) - off, g.center_at(b) - off, stroke);
                }
                DoorStyle::Double => {
                    let mid = (a + b) / 2.0;
                    leaf(&mut line, a, mid);
                    leaf(&mut line, b, mid);
                }
                DoorStyle::Sliding | DoorStyle::Pocket => {
                    let t = g.left * (g.half * 0.25);
                    let mid = (a + b) / 2.0;
                    let overlap = (b - a) * 0.05;
                    line(g.center_at(a) + t, g.center_at(mid + overlap) + t, stroke);
                    if style == DoorStyle::Pocket {
                        let dashed = Stroke {
                            dash: &[1.5, 1.0],
                            ..thin
                        };
                        line(g.center_at(a), g.center_at(a - (b - a) / 2.0), dashed);
                        line(g.center_at(mid - overlap) - t, g.center_at(b) - t, stroke);
                    } else {
                        line(g.center_at(mid - overlap) - t, g.center_at(b) - t, stroke);
                    }
                }
                DoorStyle::Bifold => {
                    let n = 4;
                    let w = (b - a) / n as f64;
                    let depth = w * 0.6;
                    let mut pts = Vec::new();
                    for i in 0..=n {
                        let u = a + w * i as f64;
                        let out_by = if i % 2 == 1 { depth } else { 0.0 };
                        pts.push(face(u) + s * out_by);
                    }
                    for p in pts.windows(2) {
                        line(p[0], p[1], stroke);
                    }
                }
                DoorStyle::Garage => {
                    // Closed sections across the opening, and the open door's
                    // overhead track shown dashed inside.
                    let t = g.left * (g.half * 0.2);
                    line(g.center_at(a) + t, g.center_at(b) + t, stroke);
                    line(g.center_at(a) - t, g.center_at(b) - t, stroke);
                    let dashed = Stroke {
                        dash: &[1.5, 1.0],
                        ..thin
                    };
                    let depth = o.height.mm();
                    line(face(a), face(a) + s * depth, dashed);
                    line(face(b), face(b) + s * depth, dashed);
                }
                DoorStyle::Barn => {
                    // A slab hung on the face, overlapping the jambs, plus the track.
                    let lap = 50.0;
                    let dir = if b > a { 1.0 } else { -1.0 };
                    let (u0, u1) = (a - lap * dir, b + lap * dir);
                    let p = [
                        face(u0) + s * 15.0,
                        face(u1) + s * 15.0,
                        face(u1) + s * 55.0,
                        face(u0) + s * 55.0,
                    ];
                    for i in 0..4 {
                        line(p[i], p[(i + 1) % 4], stroke);
                    }
                    let track_end = b + (b - a) + lap * dir;
                    line(face(u0) + s * 5.0, face(track_end) + s * 5.0, thin);
                }
                DoorStyle::Doorway => {}
            }
        }
    }
}
