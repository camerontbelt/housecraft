//! Floor plan drawing.

use housecraft_geom::math::{arc_points, perp, shape_polylines};
use housecraft_geom::walls::{WallGeom, opening_spans, wall_geoms};
use housecraft_geom::{DVec2, rooms, v2};
use housecraft_model::*;
use housecraft_units::format_area;

use crate::dims::{DimStyle, draw_dimension};
use crate::{Drawing, Fill, Ink, Prim, Stroke};

#[derive(Debug, Clone)]
pub struct PlanOptions {
    pub floor: Id,
    pub layer_set: Id,
    /// Draw the floor below, faded.
    pub reference_floor: bool,
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
            default_set: None,
        })
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

    if opts.reference_floor
        && let Some(below) = doc.floor_below(opts.floor)
    {
        for g in wall_geoms(doc, below.id) {
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
        if !look.visible {
            continue;
        }
        let openings: Vec<&Opening> = doc.openings_in(g.id).collect();
        for piece in g.pieces(&openings) {
            out.push(
                Some(g.id),
                Prim::Polygon {
                    points: piece,
                    fill: Fill::Poche,
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
        for o in doc.openings_in(g.id) {
            let look = ctx.look(o.layer);
            if look.visible {
                opening_symbol(g, o, look.stroke, &mut out);
            }
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
            // Swing side normal and the face the leaf hinges on.
            let s = if o.swing_in { -g.left } else { g.left };
            let face = |u: f64| g.center_at(u) + s * g.half;
            // Seen from the swing side, "left" is perp(-s).
            let viewer_left_is_plus_u = perp(-s).dot(g.dir) > 0.0;
            let hinge_at_b = matches!(o.hinge, Hinge::Left) == viewer_left_is_plus_u;
            let (hinge_u, free_u) = if hinge_at_b { (b, a) } else { (a, b) };
            let leaf = |out_line: &mut dyn FnMut(DVec2, DVec2, Stroke), hu: f64, fu: f64| {
                let h = face(hu);
                let width = (fu - hu).abs();
                let tip = h + s * width;
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
                DoorStyle::Hinged => leaf(&mut line, hinge_u, free_u),
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
                DoorStyle::Doorway => {}
            }
        }
    }
}
