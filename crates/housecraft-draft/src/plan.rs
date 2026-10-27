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
    fill: Option<FillStyle>,
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
                    .dash(&self.doc.settings),
            },
            text_style: l.and_then(|l| l.text_style),
            fill: l.and_then(|l| l.fill),
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
    pictures(&ctx, opts.floor, &mut out);

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

    // Room dividers: a dashed line where they run.
    for w in doc.walls_and_dividers_on(opts.floor).filter(|w| w.invisible) {
        let look = ctx.look(w.layer);
        if look.visible && opts.phase.shows(w.renovation) {
            let stroke = Stroke { dash: &[2.0, 1.2], ..look.stroke };
            out.push(Some(w.id), Prim::Polyline { points: vec![v2(w.start), v2(w.end)], closed: false, stroke });
        }
    }
    let geoms = wall_geoms(doc, opts.floor);
    for g in &geoms {
        let Some(wall) = doc.get::<Wall>(g.id) else {
            continue;
        };
        // Openings of a wall built back to back cut this one too.
        let shared: Vec<(f64, f64)> = housecraft_geom::walls::shared_openings(doc, &geoms, g)
            .into_iter()
            .map(|s| (s.0, s.1))
            .collect();
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
            for piece in g.pieces_with(&openings, &shared) {
                match look.fill {
                    // The layer's fill replaces poché (not for new work in proposed plans).
                    Some(fs) if fill == Fill::Poche => crate::fills::fill_area(&mut out, Some(g.id), &piece, &fs, look.ink, &doc.settings.hatch_patterns),
                    _ => out.push(
                        Some(g.id),
                        Prim::Polygon {
                            points: piece,
                            fill,
                        },
                    ),
                }
            }
        }
        // The wall's own hatch (shear walls…) over its pieces.
        if let Some(h) = wall.shape.hatch {
            for piece in g.pieces_with(&openings, &shared) {
                crate::fills::fill_area(&mut out, Some(g.id), &piece, &h, look.ink, &doc.settings.hatch_patterns);
            }
        }
        if let Some(f) = wall.effective_footing() {
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
        for [a, b] in g.outline_segments_with(&openings, &shared) {
            out.push(
                Some(g.id),
                Prim::Polyline {
                    points: vec![a, b],
                    closed: false,
                    stroke: look.stroke,
                },
            );
        }
        let edge = Stroke { width: look.stroke.width * 0.5, dash: &[2.0, 1.2], ..look.stroke };
        // A pony wall's lower part shows as an edge line where it is wider.
        if let Some(p) = &wall.shape.pony
            && let Some(lower) = doc.get::<WallType>(p.lower_type)
        {
            let t = lower.thickness().mm();
            if (t - 2.0 * g.half).abs() > 1.0 {
                out.push(
                    Some(g.id),
                    Prim::Polyline { points: vec![g.left_at(0.0) - g.left * t, g.left_at(g.length) - g.left * t], closed: false, stroke: edge },
                );
            }
        }
        // Niches: the recess outlined in the face it is in.
        for n in &wall.shape.niches {
            let (a, b) = ((n.position.mm() - n.width.mm() / 2.0).max(0.0), (n.position.mm() + n.width.mm() / 2.0).min(g.length));
            let d = n.depth.mm();
            let (f0, f1, inward) = if n.interior { (g.right_at(a), g.right_at(b), g.left) } else { (g.left_at(a), g.left_at(b), -g.left) };
            out.push(Some(g.id), Prim::Polyline { points: vec![f0, f0 + inward * d, f1 + inward * d, f1], closed: false, stroke: edge });
        }
        if let Some(r) = railing {
            for u in housecraft_geom::mesh::newel_positions(g.length, r.newel_spacing.map(|s| s.mm()), &wall.shape.newels) {
                let c = g.start + g.dir * u;
                let s = 45.0;
                out.push(
                    Some(g.id),
                    Prim::Polygon { points: vec![c + DVec2::new(-s, -s), c + DVec2::new(s, -s), c + DVec2::new(s, s), c + DVec2::new(-s, s)], fill: Fill::Ink(look.ink) },
                );
            }
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
                opening_symbol(doc, g, o, stroke, &mut out);
            }
            // Shutters on a layer of their own.
            if let Some(layer) = o.details.shutters.as_ref().and_then(|s| s.layer)
                && wall_shown
                && opts.phase.shows(o.renovation)
            {
                let sl = ctx.look(layer);
                if sl.visible
                    && let Some(&span) = opening_spans(g, &[o]).first()
                {
                    let id = o.id;
                    shutters_plan(g, o, span, Stroke { width: sl.stroke.width * 0.6, ..sl.stroke }, |points, closed, stroke| {
                        out.push(Some(id), Prim::Polyline { points, closed, stroke });
                    });
                }
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
        let rl = ctx.defaults.room_labels;
        let mut lines: Vec<String> = Vec::new();
        if let Some(i) = regions.room_at(at) {
            let region = &regions.rooms[i];
            if rl.show_area {
                lines.push(format_area(region.area, doc.settings.display_unit));
            }
            if rl.size != RoomSizeFormat::Off
                && let Some((lo, hi)) = housecraft_geom::math::bounds(region.polygon.iter().copied())
            {
                let (w, d) = (hi.x - lo.x, hi.y - lo.y);
                lines.push(room_size(doc, rl.size, w, d));
            }
        }
        if rl.show_ceiling_height {
            let height = room
                .ceiling_height
                .or_else(|| doc.get::<RoomType>(room.room_type).and_then(|t| t.ceiling_height))
                .or_else(|| doc.get::<Floor>(room.floor).map(|f| f.ceiling_height));
            if let Some(height) = height {
                lines.push(format!("Clg. {}", height.format(doc.settings.display_unit)));
            }
        }
        for (k, line) in lines.into_iter().enumerate() {
            out.push(
                Some(room.id),
                Prim::Text {
                    at: at - DVec2::Y * h * (1.1 + k as f64 * 0.95),
                    text: line,
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

    cad_shapes(&ctx, opts.floor, &mut out);

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
        let below = !stair.extras.to_grade
            && doc
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
    for t in doc.iter::<Countertop>().filter(|t| t.floor == opts.floor) {
        let look = ctx.look(t.layer);
        if look.visible {
            crate::cabinets::countertop(t, look.stroke, &mut out);
        }
    }
    for c in doc.iter::<Cabinet>().filter(|c| c.floor == opts.floor) {
        let look = ctx.look(c.layer);
        if look.visible && opts.phase.shows(c.renovation) {
            let label = ctx.defaults.cabinets.show_codes.then(|| {
                let style = ctx.text_style(None, &look);
                (style.height.mm() * 0.6, look.ink)
            });
            crate::cabinets::cabinet(c, look.stroke, label, &mut out);
        }
    }
    for r in doc.iter::<MaterialRegion>().filter(|r| r.floor == opts.floor) {
        let look = ctx.look(r.layer);
        if look.visible {
            crate::cabinets::material_region(doc, r, look.stroke, &mut out);
        }
    }
    for m in doc.iter::<MoldingPolyline>().filter(|m| m.floor == opts.floor) {
        let look = ctx.look(m.layer);
        if look.visible {
            crate::cabinets::molding_polyline(m, look.stroke, &mut out);
        }
    }
    for t in doc.iter::<Terrain>().filter(|t| t.floor == opts.floor) {
        let look = ctx.look(t.layer);
        if look.visible {
            crate::site::terrain(doc, t, look.stroke, look.ink, &mut out);
        }
    }
    for m in doc.iter::<FramingMember>().filter(|m| m.floor == opts.floor) {
        let look = ctx.look(m.layer);
        if look.visible {
            let outline = housecraft_geom::framing::plan_outline(m);
            if outline.len() >= 3 {
                out.push(Some(m.id), Prim::Polyline { points: outline, closed: true, stroke: Stroke { width: look.stroke.width * 0.5, ..look.stroke } });
            }
        }
    }
    // Dormers and skylights, drawn dashed like the roof above.
    for d in doc.iter::<Dormer>().filter(|d| d.floor == opts.floor) {
        let look = ctx.look(d.layer);
        if !look.visible {
            continue;
        }
        let dashed = Stroke { dash: &[2.0, 1.0], ..look.stroke };
        if let Some(p) = housecraft_geom::dormer::plan(doc, d) {
            out.push(Some(d.id), Prim::Polyline { points: p.outline, closed: true, stroke: dashed });
            if let Some((a, b)) = p.ridge {
                out.push(Some(d.id), Prim::Polyline { points: vec![a, b], closed: false, stroke: dashed });
            }
        }
    }
    for r in doc.iter::<RoofPlane>().filter(|r| r.floor == opts.floor && !r.skylights.is_empty()) {
        let look = ctx.look(r.layer);
        if !look.visible {
            continue;
        }
        let pts: Vec<housecraft_geom::DVec3> = r.outline.iter().map(|q| { let [x, y, z] = q.mm(); housecraft_geom::DVec3::new(x, y, z) }).collect();
        let down = housecraft_geom::roof_extras::downslope(&pts);
        let across = DVec2::new(-down.y, down.x);
        for s in &r.skylights {
            let c = v2(s.center);
            let (hl, hw) = (s.length.mm() / 2.0, s.width.mm() / 2.0);
            let pts = vec![c - down * hl - across * hw, c - down * hl + across * hw, c + down * hl + across * hw, c + down * hl - across * hw];
            out.push(Some(r.id), Prim::Polyline { points: pts.clone(), closed: true, stroke: Stroke { dash: &[2.0, 1.0], ..look.stroke } });
            out.push(Some(r.id), Prim::Polyline { points: vec![pts[0], pts[2]], closed: false, stroke: Stroke { dash: &[2.0, 1.0], ..look.stroke } });
        }
    }
    for dk in doc.iter::<Deck>().filter(|d| d.floor == opts.floor) {
        let look = ctx.look(dk.layer);
        if look.visible
            && let Some(parts) = housecraft_geom::deck::parts(doc, dk)
        {
            let thin = Stroke { width: look.stroke.width * 0.4, ..look.stroke };
            for p in parts.planks.iter().chain(&parts.border) {
                out.push(Some(dk.id), Prim::Polyline { points: p.clone(), closed: true, stroke: thin });
            }
            out.push(Some(dk.id), Prim::Polyline { points: parts.outline.clone(), closed: true, stroke: look.stroke });
            for (a, b) in &parts.rails {
                let n = perp((*b - *a).normalize_or_zero()) * 60.0;
                out.push(Some(dk.id), Prim::Polyline { points: vec![*a + n, *b + n], closed: false, stroke: look.stroke });
            }
            for m in parts.members.iter().filter(|m| m.role == FramingRole::Post) {
                let c = m.a.truncate();
                let h = m.width / 2.0;
                out.push(
                    Some(dk.id),
                    Prim::Polyline { points: vec![c + DVec2::new(-h, -h), c + DVec2::new(h, -h), c + DVec2::new(h, h), c + DVec2::new(-h, h)], closed: true, stroke: look.stroke },
                );
            }
        }
    }
    for j in doc.iter::<JoistDirection>().filter(|j| j.floor == opts.floor) {
        let look = ctx.look(j.layer);
        if look.visible {
            // A double-headed arrow along the joists.
            let c = housecraft_geom::v2(j.at);
            let d = DVec2::from_angle(j.angle.to_radians());
            let n = perp(d);
            let (a, b) = (c - d * 900.0, c + d * 900.0);
            let dash: &'static [f64] = if j.ceiling { &[2.0, 1.0] } else { &[] };
            let s = Stroke { dash, ..look.stroke };
            out.push(Some(j.id), Prim::Polyline { points: vec![a, b], closed: false, stroke: s });
            for (tip, back) in [(a, d), (b, -d)] {
                out.push(Some(j.id), Prim::Polyline { points: vec![tip + back * 200.0 + n * 100.0, tip, tip + back * 200.0 - n * 100.0], closed: false, stroke: look.stroke });
            }
        }
    }
    for f in doc.iter::<SiteFeature>().filter(|f| f.floor == opts.floor) {
        let look = ctx.look(f.layer);
        if look.visible {
            crate::site::feature(f, look.stroke, &mut out);
        }
    }
    for p in doc.iter::<Plant>().filter(|p| p.floor == opts.floor) {
        let look = ctx.look(p.layer);
        if look.visible {
            crate::site::plant(p, look.stroke, look.ink, &mut out);
        }
    }
    for e in doc.iter::<ElevationData>().filter(|e| e.floor == opts.floor) {
        let look = ctx.look(e.layer);
        if look.visible {
            crate::site::elevation_data(doc, e, look.stroke, look.ink, &mut out);
        }
    }
    for x in doc.iter::<Solid>().filter(|x| x.floor == opts.floor) {
        let look = ctx.look(x.layer);
        if look.visible {
            crate::cabinets::solid(x, look.stroke, &mut out);
        }
    }
    for b in doc.iter::<Backsplash>().filter(|b| b.floor == opts.floor) {
        let look = ctx.look(b.layer);
        if look.visible {
            crate::cabinets::backsplash(b, look.stroke, &mut out);
        }
    }
    for s in doc.iter::<Symbol>().filter(|s| s.floor == opts.floor) {
        let look = ctx.look(s.layer);
        if look.visible && opts.phase.shows(s.renovation) {
            let outline = housecraft_geom::cabinets::symbol_outline(doc, s);
            if outline.len() >= 3 {
                out.push(
                    Some(s.id),
                    Prim::Polyline {
                        points: outline,
                        closed: true,
                        stroke: look.stroke,
                    },
                );
            }
        }
    }
    for a in doc.iter::<Appliance>().filter(|a| a.floor == opts.floor) {
        let look = ctx.look(a.layer);
        if look.visible && opts.phase.shows(a.renovation) {
            let style = ctx.text_style(None, &look);
            crate::cabinets::appliance(a, look.stroke, style.height.mm() * 0.7, look.ink, &mut out);
        }
    }
    if let Some(at) = doc.settings.north_pointer {
        north_pointer(v2(at), doc.settings.north, &mut out);
    }

    for v in doc.iter::<SectionView>().filter(|v| v.floor == opts.floor) {
        let look = ctx.look(v.layer);
        if look.visible {
            let style = ctx.text_style(None, &look);
            section_callout(doc, v, look.stroke, style.height.mm(), &mut out);
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

    notes(&ctx, opts.floor, &mut out);
    object_labels(&ctx, opts.floor, &mut out);
    out
}

/// Pictures placed on `sheet`, under everything else.
fn pictures(ctx: &Ctx, sheet: Id, out: &mut Drawing) {
    for p in ctx.doc.iter::<Picture>().filter(|p| p.floor == sheet) {
        if !ctx.look(p.layer).visible {
            continue;
        }
        let o = v2(p.position);
        let a = DVec2::from_angle(p.rotation.to_radians());
        let (w, h) = (a * p.width.mm(), perp(a) * p.height.mm());
        out.push(
            Some(p.id),
            Prim::Image {
                corners: [o, o + w, o + w + h, o + h],
                path: p.path.clone(),
                opacity: p.opacity,
                transparent: p.transparent,
            },
        );
    }
}

/// CAD shapes and placed blocks drawn on `sheet`.
fn cad_shapes(ctx: &Ctx, sheet: Id, out: &mut Drawing) {
    let doc = ctx.doc;
    for cad in doc.iter::<Cad>().filter(|c| c.floor == sheet) {
        let look = ctx.look(cad.layer);
        if look.visible {
            let fs = cad.fill_style.or(look.fill);
            cad_shape(cad.id, &cad.shape, cad.color, cad.fill, fs, look, &doc.settings.hatch_patterns, out);
        }
    }
    for b in doc.iter::<BlockRef>().filter(|b| b.floor == sheet) {
        let look = ctx.look(b.layer);
        if !look.visible {
            continue;
        }
        let t = housecraft_api::blocks::placement(b);
        for cad in doc.iter::<Cad>().filter(|c| c.floor == b.block) {
            let fs = cad.fill_style.or(look.fill);
            cad_shape(b.id, &t.apply_shape(&cad.shape), cad.color, cad.fill, fs, look, &doc.settings.hatch_patterns, out);
        }
    }
}

/// One CAD shape, tagged with `id` for selection.
#[allow(clippy::too_many_arguments)]
fn cad_shape(
    id: Id,
    shape: &Shape,
    color: Option<Color>,
    fill: Option<Color>,
    fill_style: Option<FillStyle>,
    look: Look,
    patterns: &[HatchPattern],
    out: &mut Drawing,
) {
    let ink = color.map_or(look.ink, Ink::Color);
    let stroke = Stroke { ink, ..look.stroke };
    for (points, closed) in shape_polylines(shape) {
        if let (Some(fs), true) = (fill_style, closed) {
            crate::fills::fill_area(out, Some(id), &points, &fs, ink, patterns);
        }
        if let (Some(fill), true) = (fill, closed) {
            out.push(
                Some(id),
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
                    Some(id),
                    Prim::Polyline {
                        points: vec![p - d, p + d],
                        closed: false,
                        stroke,
                    },
                );
            }
        } else {
            out.push(
                Some(id),
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
    } = shape
    {
        let (a, b) = (v2(*a), v2(*b));
        let size = 150.0;
        for (on, tip, from) in [(*arrow_start, a, b), (*arrow_end, b, a)] {
            if on {
                let dir = (from - tip).normalize_or_zero();
                let back = tip + dir * size;
                let side = perp(dir) * size * 0.35;
                out.push(
                    Some(id),
                    Prim::Polygon {
                        points: vec![tip, back + side, back - side],
                        fill: Fill::Ink(ink),
                    },
                );
            }
        }
    }
}

/// Callouts, revision clouds, dimensions and text drawn on `sheet`.
/// A room's interior width x depth for its label.
fn room_size(doc: &Document, format: RoomSizeFormat, w: f64, d: f64) -> String {
    let unit = doc.settings.display_unit;
    let metric = matches!(unit, housecraft_units::DisplayUnit::Millimeters | housecraft_units::DisplayUnit::Meters { .. });
    match format {
        RoomSizeFormat::Rounded if metric => format!("{:.1} x {:.1} m", w / 1000.0, d / 1000.0),
        RoomSizeFormat::Rounded => format!("{:.0}' x {:.0}'", w / 304.8, d / 304.8),
        _ => format!(
            "{} x {}",
            housecraft_units::Length::from_mm(w).format(unit),
            housecraft_units::Length::from_mm(d).format(unit)
        ),
    }
}

/// Labels on doors, windows, cabinets and fixtures: each object's own
/// label, else its kind's template from the default set.
fn object_labels(ctx: &Ctx, floor: Id, out: &mut Drawing) {
    // Thumbnails show objects, not their labels.
    if THUMBNAIL.with(|f| f.get()) {
        return;
    }
    let doc = ctx.doc;
    let l = &ctx.defaults.labels;
    if l.doors.is_empty() && l.windows.is_empty() && l.cabinets.is_empty() && l.fixtures.is_empty() && doc.settings.labels.is_empty() {
        return;
    }
    let no_template = String::new();
    let in_front = |pos: housecraft_units::Point2, facing: f64, depth: housecraft_units::Length| v2(pos) + DVec2::from_angle(facing.to_radians()) * depth.mm() / 2.0;
    for (id, object) in &doc.objects {
        let (template, at, layer) = match object {
            Object::Opening(o) => {
                let Some(w) = doc.get::<Wall>(o.wall).filter(|w| w.floor == floor) else {
                    continue;
                };
                let (a, b) = (v2(w.start), v2(w.end));
                let dir = (b - a).normalize_or_zero();
                // Outside the wall (the exterior is on the left of start→end).
                let at = a + dir * o.position.mm() + perp(dir) * 450.0;
                let t = if o.opening_kind.is_door() { &l.doors } else { &l.windows };
                (t, at, o.layer)
            }
            Object::Cabinet(c) if c.floor == floor => (&l.cabinets, in_front(c.position, c.facing, c.depth), c.layer),
            Object::Appliance(a) if a.floor == floor => (&l.fixtures, in_front(a.position, a.facing, a.depth), a.layer),
            Object::Symbol(s) if s.floor == floor => (&l.fixtures, v2(s.position), s.layer),
            // Walls are labelled one by one (shear wall marks and the like).
            Object::Wall(w) if w.floor == floor && doc.settings.labels.contains_key(id) => {
                let (a, b) = (v2(w.start), v2(w.end));
                let side = perp((b - a).normalize_or_zero()) * 450.0;
                (&no_template, (a + b) / 2.0 + side, w.layer)
            }
            _ => continue,
        };
        let text = doc.settings.labels.get(id).unwrap_or(template);
        if text.trim().is_empty() || text.trim() == "-" {
            continue;
        }
        let look = ctx.look(layer);
        if !look.visible {
            continue;
        }
        let style = ctx.text_style(None, &look);
        let ink = style.color.map_or(look.ink, Ink::Color);
        let text = crate::macros::expand_object(doc, *id, text);
        let height = style.height.mm() * 0.75;
        crate::rich::text_block(
            out,
            Some(*id),
            &crate::rich::TextBlock {
                at,
                text: &text,
                height,
                rotation: 0.0,
                justify: Justify::Center,
                middle: true,
                bold: false,
                ink,
                stroke: Stroke { ink, ..look.stroke },
                border: l.border && l.shape.is_none(),
                wrap: l.wrap.map(|w| w.mm()),
                leaders: &[],
            },
        );
        // A schedule-style marker round the label.
        if let Some(shape) = l.shape {
            let lines = text.lines().count().max(1) as f64;
            let w = text.lines().map(|t| crate::approx_text_width(t, height) * 1.3).fold(0.0, f64::max);
            let r = (w / 2.0).max(lines * height * 0.6) + height * 0.5;
            out.push(
                Some(*id),
                Prim::Polyline {
                    points: marker_outline(shape, at, r),
                    closed: true,
                    stroke: Stroke { ink, ..look.stroke },
                },
            );
        }
    }
}

fn notes(ctx: &Ctx, sheet: Id, out: &mut Drawing) {
    let doc = ctx.doc;
    for c in doc.iter::<Callout>().filter(|c| c.floor == sheet) {
        let look = ctx.look(c.layer);
        if look.visible {
            let style = ctx.text_style(None, &look);
            let h = c.height.unwrap_or(style.height).mm();
            callout(doc, c, h, look.stroke, out);
        }
    }
    for rc in doc
        .iter::<RevisionCloud>()
        .filter(|c| c.floor == sheet)
    {
        let look = ctx.look(rc.layer);
        if look.visible {
            let style = ctx.text_style(None, &look);
            revision_cloud(rc, style.height.mm(), look.stroke, out);
        }
    }

    for dim in doc.iter::<Dimension>().filter(|d| d.floor == sheet) {
        let look = ctx.look(dim.layer);
        if !look.visible {
            continue;
        }
        // A named dimension style overrides the default set's dimension defaults.
        let styled;
        let defaults = match dim.style.as_ref().and_then(|n| doc.settings.dimension_styles.iter().find(|s| &s.name == n)) {
            Some(st) => {
                let mut d = ctx.defaults.clone();
                d.dimensions = st.defaults.clone();
                styled = d;
                &styled
            }
            None => ctx.defaults,
        };
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
            block: match defaults.dimensions.arrow {
                ArrowStyle::Block(b) => crate::dims::arrow_block(doc, b),
                _ => Vec::new(),
            },
        };
        draw_dimension(doc, defaults, dim, &style, out);
    }

    for text in doc.iter::<Text>().filter(|t| t.floor == sheet) {
        let look = ctx.look(text.layer);
        if !look.visible {
            continue;
        }
        let style = ctx.text_style(text.style, &look);
        let leaders: Vec<DVec2> = text.leaders.iter().map(|p| v2(*p)).collect();
        let ink = text.color.or(style.color).map_or(look.ink, Ink::Color);
        crate::rich::text_block(
            out,
            Some(text.id),
            &crate::rich::TextBlock {
                at: v2(text.position),
                text: &crate::macros::expand(doc, &text.text),
                height: text.height.unwrap_or(style.height).mm(),
                rotation: text.rotation,
                justify: text.justify,
                middle: false,
                bold: text.bold.unwrap_or(style.bold),
                ink,
                stroke: Stroke { ink, ..look.stroke },
                border: text.border,
                wrap: text.wrap.map(|w| w.mm()),
                leaders: &leaders,
            },
        );
    }
}

/// The drafting on a 2D view or CAD detail `sheet` (its lines, blocks,
/// dimensions, text and callouts), as `layer_set` shows it (`None`: the
/// first layer set), using the sheet's camera default set.
pub fn drafting(doc: &Document, sheet: Id, layer_set: Option<Id>) -> Drawing {
    let ctx = Ctx {
        doc,
        set: layer_set
            .and_then(|s| doc.get::<LayerSet>(s))
            .or_else(|| doc.iter::<LayerSet>().next()),
        defaults: doc.defaults_for(sheet),
    };
    let mut out = Drawing::default();
    pictures(&ctx, sheet, &mut out);
    cad_shapes(&ctx, sheet, &mut out);
    notes(&ctx, sheet, &mut out);
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
    // A thumbnail's own plan drawing leaves schedules out (no recursion).
    if THUMBNAIL.with(|f| f.get()) {
        return;
    }
    let table = housecraft_api::schedule::schedule(
        doc,
        s.schedule_kind,
        &housecraft_api::schedule::ScheduleOptions {
            floor: (!s.all_floors).then_some(s.floor),
            columns: s.columns.clone(),
            also: s.also.clone(),
            renovation: s.renovation,
            start: s.start_number,
        },
    );
    let thumbs = if s.thumbnails { thumbnails(doc, &table) } else { Vec::new() };
    table_drawing(&table, v2(s.position), h, stroke, ink, Some(s.id), &thumbs, out);
}

thread_local! {
    static THUMBNAIL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Each row's first object as its plan shows it.
pub(crate) fn thumbnails(doc: &Document, table: &housecraft_api::schedule::ScheduleTable) -> Vec<Drawing> {
    let mut plans: std::collections::BTreeMap<Id, Drawing> = std::collections::BTreeMap::new();
    THUMBNAIL.with(|f| f.set(true));
    let thumbs = table
        .rows
        .iter()
        .map(|r| {
            let mut d = Drawing::default();
            let Some(id) = r.ids.first().copied() else { return d };
            let Some(floor) = doc.floor_of(id).filter(|f| doc.get::<Floor>(*f).is_some()) else {
                return d;
            };
            let plan_d = plans.entry(floor).or_insert_with(|| {
                let mut opts = PlanOptions::first(doc).expect("a floor exists");
                opts.floor = floor;
                plan(doc, &opts)
            });
            d.items = plan_d.items.iter().filter(|i| i.object == Some(id) && !matches!(i.prim, Prim::Text { .. })).cloned().collect();
            d
        })
        .collect();
    THUMBNAIL.with(|f| f.set(false));
    thumbs
}

/// Draw `table` as a ruled table hanging down-right from `o` (its top-left).
/// With `thumbs` (one per row), a first column shows each row's picture.
#[allow(clippy::too_many_arguments)]
pub(crate) fn table_drawing(
    table: &housecraft_api::schedule::ScheduleTable,
    o: DVec2,
    h: f64,
    stroke: Stroke,
    ink: Ink,
    id: Option<Id>,
    thumbs: &[Drawing],
    out: &mut Drawing,
) {
    let pad = h * 0.6;
    let row_h = h * 2.0;
    let pictures = !thumbs.is_empty() && thumbs.len() == table.rows.len();
    // Rows with pictures are taller.
    let data_h = if pictures { h * 5.0 } else { row_h };
    let thumb_w = if pictures { data_h * 1.4 } else { 0.0 };
    // The shared width estimate is tuned for labels; table text, and the bold
    // header especially, renders wider, so allow a margin.
    let text_w =
        |t: &str, bold: bool| crate::approx_text_width(t, h) * if bold { 1.4 } else { 1.3 };
    let mut widths: Vec<f64> = (0..table.columns.len())
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
    if pictures {
        widths.insert(0, thumb_w);
    }
    let width: f64 = widths
        .iter()
        .sum::<f64>()
        .max(crate::approx_text_width(&table.title, h * 1.3) + 2.0 * pad);
    // Top of row r (0: title, 1: header, then the data rows).
    let top = |r: usize| if r <= 2 { r as f64 * row_h } else { 2.0 * row_h + (r - 2) as f64 * data_h };
    let rows = table.rows.len() + 2; // title and header
    let height = top(rows);
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
            at(0.0, top(r)),
            at(width, top(r)),
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
    let first = if pictures { thumb_w } else { 0.0 };
    let cells = std::iter::once(&table.columns).chain(table.rows.iter().map(|r| &r.cells));
    for (r, row) in cells.enumerate() {
        let y = (top(r + 1) + top(r + 2)) / 2.0;
        let mut x = first;
        for (c, cell) in row.iter().enumerate() {
            text(out, at(x + pad, y), cell, h, r == 0, Justify::Left);
            x += widths[c + usize::from(pictures)];
        }
    }
    // Pictures fitted into their cells.
    if pictures {
        for (r, d) in thumbs.iter().enumerate() {
            let Some((lo, hi)) = d.bounds() else { continue };
            let size = (hi - lo).max(DVec2::splat(1e-6));
            let room = DVec2::new(thumb_w, data_h) - DVec2::splat(2.0 * pad);
            let k = (room.x / size.x).min(room.y / size.y);
            let c = (lo + hi) / 2.0;
            let cell = at(thumb_w / 2.0, (top(r + 2) + top(r + 3)) / 2.0);
            let map = |p: DVec2| cell + (p - c) * k;
            for item in &d.items {
                let prim = match &item.prim {
                    Prim::Polyline { points, closed, stroke } => Prim::Polyline {
                        points: points.iter().map(|p| map(*p)).collect(),
                        closed: *closed,
                        stroke: Stroke { width: thin.width, ..*stroke },
                    },
                    Prim::Polygon { points, fill } => Prim::Polygon { points: points.iter().map(|p| map(*p)).collect(), fill: *fill },
                    _ => continue,
                };
                out.push(id, prim);
            }
        }
    }
}

/// Plan callout for an elevation / section view: its line (dashed for a
/// cut), a marker whose arrow points the way the view looks, and its name.
fn section_callout(doc: &Document, v: &SectionView, stroke: Stroke, text: f64, out: &mut Drawing) {
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
    let placed = layout_page_of(doc, LayoutSource::View { view: v.id });
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
        // The arrow caps the marker, leaving its middle free for the
        // layout reference.
        let side = dir * r * 0.8;
        let base = c + look * r * 0.6;
        out.push(
            id,
            Prim::Polygon {
                points: vec![c + look * r * 1.9, base + side, base - side],
                fill: Fill::Ink(stroke.ink),
            },
        );
        // Where the view is placed: its box number over the layout page.
        if let Some((number, page)) = &placed {
            let lines: Vec<(DVec2, &str)> = match number {
                Some(n) => vec![(c + DVec2::Y * r * 0.42, n.as_str()), (c - DVec2::Y * r * 0.42, page.as_str())],
                None => vec![(c, page.as_str())],
            };
            if number.is_some() {
                out.push(
                    id,
                    Prim::Polyline {
                        points: vec![c - DVec2::X * r, c + DVec2::X * r],
                        closed: false,
                        stroke,
                    },
                );
            }
            for (at, t) in lines {
                out.push(
                    id,
                    Prim::Text {
                        at,
                        text: t.to_string(),
                        height: text * 0.55,
                        rotation: 0.0,
                        justify: Justify::Center,
                        middle: true,
                        bold: true,
                        ink: stroke.ink,
                    },
                );
            }
        }
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

/// Where `source` is placed on a layout: the page's name when it is a short
/// sheet number like "A-3", else the page's number.
/// The box's number on its page comes first, when it has one.
pub fn layout_page_of(doc: &Document, source: LayoutSource) -> Option<(Option<String>, String)> {
    let b = doc.iter::<LayoutBox>().find(|b| b.source == source)?;
    let layout = doc.get::<Layout>(b.layout)?;
    let name = layout.pages.get(b.page as usize).map(|p| p.trim()).unwrap_or("");
    let page = if !name.is_empty() && !name.starts_with("Page") && name.chars().count() <= 6 {
        name.to_string()
    } else {
        format!("{}", b.page + 1)
    };
    Some((housecraft_api::layouts::box_number(doc, b).map(|n| n.to_string()), page))
}

/// A marker's outline of "radius" `r` round `at`.
fn marker_outline(m: MarkerShape, at: DVec2, r: f64) -> Vec<DVec2> {
    match m {
        MarkerShape::Circle => arc_points(at, r, 0.0, 360.0),
        MarkerShape::Hexagon => (0..6)
            .map(|k| at + DVec2::from_angle((k as f64 * 60.0).to_radians()) * r * 1.1)
            .collect(),
        MarkerShape::Square => vec![
            at + DVec2::new(-r, -r),
            at + DVec2::new(r, -r),
            at + DVec2::new(r, r),
            at + DVec2::new(-r, r),
        ],
        MarkerShape::Diamond => vec![
            at + DVec2::new(0.0, -r * 1.3),
            at + DVec2::new(r * 1.3, 0.0),
            at + DVec2::new(0.0, r * 1.3),
            at + DVec2::new(-r * 1.3, 0.0),
        ],
    }
}

/// A callout: text, optionally inside a marker shape, with an optional
/// leader arrow from the text to its target.
fn callout(doc: &Document, c: &Callout, h: f64, stroke: Stroke, out: &mut Drawing) {
    let id = Some(c.id);
    let at = v2(c.at);
    let text = if c.note {
        // A numbered note shows its number; the text is in the Note Schedule.
        housecraft_api::schedule::note_numbers(doc).get(&c.id).map_or_else(String::new, |n| n.to_string())
    } else {
        // {link}: where the linked view is placed on a layout ("2/A-1").
        let link = c
            .link
            .and_then(|l| match doc.object(l) {
                Some(Object::SectionView(_)) => Some(LayoutSource::View { view: l }),
                Some(Object::CadDetail(_)) => Some(LayoutSource::Detail { detail: l }),
                _ => None,
            })
            .and_then(|s| layout_page_of(doc, s))
            .map_or_else(String::new, |(n, p)| n.map_or(p.clone(), |n| format!("{n}/{p}")));
        crate::macros::expand(doc, &c.text.replace("{link}", &link))
    };
    let lines: Vec<&str> = text.lines().collect();
    let w = lines
        .iter()
        .map(|l| crate::approx_text_width(l, h))
        .fold(0.0, f64::max);
    let line_h = h * 1.5;
    let total_h = line_h * lines.len().max(1) as f64;
    // Text reads away from the target so the leader meets its near edge.
    let toward_left = c.target.is_some_and(|t| v2(t).x < at.x);
    let justify = match (c.marker, toward_left) {
        (Some(_), _) => Justify::Center,
        (None, true) => Justify::Left,
        (None, false) if c.target.is_some() => Justify::Right,
        _ => Justify::Left,
    };
    for (i, l) in lines.iter().enumerate() {
        out.push(
            id,
            Prim::Text {
                at: at - DVec2::Y * (i as f64 * line_h - (total_h - line_h) / 2.0),
                text: l.to_string(),
                height: h,
                rotation: 0.0,
                justify,
                middle: true,
                bold: false,
                ink: stroke.ink,
            },
        );
    }
    // Marker outline around the text.
    let r = (w / 2.0).max(total_h / 2.0) + h * 0.6;
    let outline: Option<Vec<DVec2>> = c.marker.map(|m| marker_outline(m, at, r));
    if let Some(points) = &outline {
        out.push(
            id,
            Prim::Polyline {
                points: points.clone(),
                closed: true,
                stroke,
            },
        );
    }
    if let Some(t) = c.target {
        let t = v2(t);
        // Leave from the marker's edge, or the text's near side.
        let start = match c.marker {
            Some(_) => at + (t - at).normalize_or_zero() * r,
            None if toward_left => at - DVec2::X * h * 0.4,
            None if c.target.is_some() => at + DVec2::X * h * 0.4,
            None => at,
        };
        out.push(
            id,
            Prim::Polyline {
                points: vec![start, t],
                closed: false,
                stroke,
            },
        );
        let dir = (start - t).normalize_or_zero();
        let back = t + dir * h * 1.2;
        let side = perp(dir) * h * 0.35;
        out.push(
            id,
            Prim::Polygon {
                points: vec![t, back + side, back - side],
                fill: Fill::Ink(stroke.ink),
            },
        );
    }
}

/// A revision cloud: outward scallops along a closed outline, with the
/// revision label in a triangle at its first corner.
fn revision_cloud(rc: &RevisionCloud, h: f64, stroke: Stroke, out: &mut Drawing) {
    let id = Some(rc.id);
    let mut pts: Vec<DVec2> = rc.points.iter().map(|p| v2(*p)).collect();
    if housecraft_geom::math::signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    let arc = rc.arc.mm().max(10.0);
    let mut path = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        let len = a.distance(b);
        let n = (len / arc).round().max(1.0) as usize;
        // Counter-clockwise outline: outward is to the right of each edge.
        let outward = -perp((b - a).normalize_or_zero());
        for k in 0..n {
            let (p, q) = (
                a.lerp(b, k as f64 / n as f64),
                a.lerp(b, (k + 1) as f64 / n as f64),
            );
            let mid = (p + q) / 2.0;
            let half = p.distance(q) / 2.0;
            // A semicircle bulging outward from p to q.
            let start = (p - mid).to_angle().to_degrees();
            let mut sweep = -180.0;
            if outward.dot(DVec2::from_angle((start + sweep / 2.0).to_radians())) < 0.0 {
                sweep = 180.0;
            }
            let seg = arc_points(mid, half, start, sweep);
            path.extend(seg);
        }
    }
    if path.len() > 2 {
        out.push(
            id,
            Prim::Polyline {
                points: path,
                closed: true,
                stroke,
            },
        );
    }
    if let (Some(label), Some(&corner)) = (&rc.label, pts.first()) {
        let s = h * 1.4;
        let tri = vec![
            corner + DVec2::new(-s, -s * 0.6),
            corner + DVec2::new(s, -s * 0.6),
            corner + DVec2::new(0.0, s * 1.1),
        ];
        out.push(
            id,
            Prim::Polygon {
                points: tri.clone(),
                fill: Fill::Paper,
            },
        );
        out.push(
            id,
            Prim::Polyline {
                points: tri,
                closed: true,
                stroke,
            },
        );
        out.push(
            id,
            Prim::Text {
                at: corner,
                text: label.clone(),
                height: h * 0.8,
                rotation: 0.0,
                justify: Justify::Center,
                middle: true,
                bold: true,
                ink: stroke.ink,
            },
        );
    }
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
    // A stair down to grade lies wholly below the cut: nothing is dashed.
    let cut_riser = if stair.extras.to_grade { u32::MAX } else { (STAIR_CUT / g.riser_height).ceil() as u32 };
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
    if here && !stair.extras.to_grade && let Some(step) = g.steps.iter().rfind(|s| s.index <= cut_riser && !s.landing) {
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
    // A stair down to grade reads DN from the floor it leaves.
    let up = here && !stair.extras.to_grade;
    let mut walk: Vec<DVec2> = g.walk.iter().map(|p| p.truncate()).collect();
    if !up {
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
                text: if up { "UP" } else { "DN" }.into(),
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

/// A window's shutters in plan: a board either side, outside the wall.
fn shutters_plan(g: &WallGeom, o: &Opening, (a, b): (f64, f64), stroke: Stroke, mut sink: impl FnMut(Vec<DVec2>, bool, Stroke)) {
    let Some(s) = &o.details.shutters else { return };
    let at = |u: f64, off: f64| g.center_at(u) + g.left * off;
    let sw = s.width.map_or((b - a) / 2.0, |w| w.mm());
    let (o0, o1) = (g.half + 25.0, g.half + 50.0);
    for (u0, u1) in [(a - sw, a), (b, b + sw)] {
        sink(vec![at(u0, o0), at(u1, o0), at(u1, o1), at(u0, o1)], true, stroke);
    }
}

/// Adapts a `(points, closed)` drawer to [`shutters_plan`]'s sink.
fn poly_sink(poly: &mut impl FnMut(Vec<DVec2>, bool)) -> impl FnMut(Vec<DVec2>, bool, Stroke) + '_ {
    move |p, c, _| poly(p, c)
}

/// Plan symbol for a door or window in its wall gap.
/// Shutters outside and curtains or blinds inside a window, in plan.
fn opening_details(g: &WallGeom, o: &Opening, (a, b): (f64, f64), stroke: Stroke, out: &mut Drawing) {
    let thin = Stroke {
        width: stroke.width * 0.6,
        ..stroke
    };
    let at = |u: f64, off: f64| g.center_at(u) + g.left * off;
    let mut poly = |points: Vec<DVec2>, closed: bool| {
        out.push(Some(o.id), Prim::Polyline { points, closed, stroke: thin });
    };
    if o.details.shutters.as_ref().is_some_and(|s| s.layer.is_none()) {
        shutters_plan(g, o, (a, b), thin, poly_sink(&mut poly));
    }
    if let Some(t) = &o.details.treatment {
        let room = -g.half;
        match t.kind {
            TreatmentKind::Curtains => {
                let (r0, r1) = (a - 200.0, b + 200.0);
                let pw = ((1.0 - t.open.clamp(0.0, 1.0)) * (r1 - r0) / 2.0).max(150.0);
                poly(vec![at(r0, room - 80.0), at(r1, room - 80.0)], false);
                for (u0, u1) in [(r0, r0 + pw), (r1 - pw, r1)] {
                    let n = ((u1 - u0) / 100.0).ceil().max(1.0) as usize;
                    let zig = (0..=n)
                        .map(|i| {
                            let u = u0 + (u1 - u0) * i as f64 / n as f64;
                            at(u, room - if i % 2 == 0 { 50.0 } else { 110.0 })
                        })
                        .collect();
                    poly(zig, false);
                }
            }
            TreatmentKind::Blinds | TreatmentKind::Shade => {
                poly(vec![at(a, room + 45.0), at(b, room + 45.0)], false);
            }
        }
    }
}

fn opening_symbol(doc: &Document, g: &WallGeom, o: &Opening, stroke: Stroke, out: &mut Drawing) {
    let Some(&(a, b)) = opening_spans(g, &[o]).first() else {
        return;
    };
    opening_details(g, o, (a, b), stroke, out);
    // A custom plan symbol: the block stretched over the opening.
    if let Some(block) = o.details.plan_block.and_then(|b| doc.get::<Block>(b)) {
        let lines: Vec<(Vec<DVec2>, bool)> = doc
            .iter::<Cad>()
            .filter(|c| c.floor == block.id)
            .flat_map(|c| shape_polylines(&c.shape))
            .collect();
        if let Some((lo, hi)) = housecraft_geom::math::bounds(lines.iter().flat_map(|(p, _)| p.iter().copied())) {
            let size = hi - lo;
            let map = |p: DVec2| {
                let u = a + if size.x > 1e-6 { (p.x - lo.x) / size.x } else { 0.5 } * (b - a);
                let v = if size.y > 1e-6 { (p.y - lo.y) / size.y * 2.0 - 1.0 } else { 0.0 } * g.half;
                g.center_at(u) + g.left * v
            };
            for (pts, closed) in lines {
                out.push(
                    Some(o.id),
                    Prim::Polyline {
                        points: pts.into_iter().map(map).collect(),
                        closed,
                        stroke,
                    },
                );
            }
            return;
        }
    }
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
                DoorStyle::Sliding if o.details.panels > 2 => {
                    // Several panels in staggered tracks.
                    let count = o.details.panels as usize;
                    let w = (b - a) / count as f64;
                    for k in 0..count {
                        let off = g.left * (g.half * 0.5 * ((k % 3) as f64 / 2.0 - 0.5));
                        let (u0, u1) = (a + w * k as f64 - w * 0.05, a + w * (k + 1) as f64 + w * 0.05);
                        line(g.center_at(u0) + off, g.center_at(u1) + off, stroke);
                    }
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
                    let n = (o.details.panels.max(4) as usize).div_ceil(2) * 2;
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
