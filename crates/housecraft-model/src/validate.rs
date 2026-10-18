use std::fmt;

use housecraft_units::Length;
use serde::Serialize;

use crate::*;

/// A consistency problem found by [`Document::validate`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue {
    pub object: Id,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.object, self.message)
    }
}

struct Checker<'a> {
    doc: &'a Document,
    issues: Vec<Issue>,
}

impl Checker<'_> {
    fn issue(&mut self, object: Id, message: impl Into<String>) {
        self.issues.push(Issue {
            object,
            message: message.into(),
        });
    }

    fn reference(&mut self, from: Id, field: &str, to: Id, kind: ObjectKind) {
        match self.doc.kind_of(to) {
            Some(k) if k == kind => {}
            Some(k) => self.issue(from, format!("{field} {to} is a {k}, expected a {kind}")),
            None => self.issue(from, format!("{field} references missing {kind} {to}")),
        }
    }

    fn positive(&mut self, from: Id, field: &str, len: Length) {
        if !(len.is_finite() && len.mm() > 0.0) {
            self.issue(from, format!("{field} must be positive"));
        }
    }

    fn non_negative(&mut self, from: Id, field: &str, len: Length) {
        if !(len.is_finite() && len.mm() >= 0.0) {
            self.issue(from, format!("{field} must not be negative"));
        }
    }

    fn finite(&mut self, from: Id, field: &str, values: &[f64]) {
        if values.iter().any(|v| !v.is_finite()) {
            self.issue(from, format!("{field} must be finite"));
        }
    }
}

pub(crate) fn validate(doc: &Document) -> Vec<Issue> {
    use ObjectKind as K;
    let mut c = Checker {
        doc,
        issues: Vec::new(),
    };

    if let Some((max, _)) = doc.objects.last_key_value()
        && max.0 >= doc.next_id
    {
        c.issue(
            *max,
            format!("next_id {} is not above the largest id", doc.next_id),
        );
    }
    c.reference(
        Id(0),
        "settings.active_default_set",
        doc.settings.active_default_set,
        K::DefaultSet,
    );
    if doc.count::<Floor>() == 0 {
        c.issue(Id(0), "document has no floors");
    }

    for (key, object) in &doc.objects {
        let id = object.id();
        if *key != id {
            c.issue(*key, format!("stored under key {key} but has id {id}"));
        }
        match object {
            Object::Floor(f) => {
                c.positive(id, "ceiling_height", f.ceiling_height);
                c.non_negative(id, "floor_structure", f.floor_structure);
            }
            Object::WallType(t) => {
                if t.layers.is_empty() {
                    c.issue(id, "wall type has no layers");
                }
                for l in &t.layers {
                    c.positive(id, "layer thickness", l.thickness);
                }
            }
            Object::Wall(w) => {
                c.reference(id, "floor", w.floor, K::Floor);
                c.reference(id, "wall_type", w.wall_type, K::WallType);
                c.reference(id, "layer", w.layer, K::Layer);
                c.finite(id, "endpoints", &[w.start.mm(), w.end.mm()].concat());
                if w.length().mm() < 1.0 {
                    c.issue(id, "wall is shorter than 1 mm");
                }
                if let Some(h) = w.height {
                    c.positive(id, "height", h);
                }
            }
            Object::Opening(o) => {
                c.reference(id, "wall", o.wall, K::Wall);
                c.reference(id, "layer", o.layer, K::Layer);
                c.positive(id, "width", o.width);
                c.positive(id, "height", o.height);
                c.non_negative(id, "sill", o.sill);
                if let Some(w) = doc.get::<Wall>(o.wall) {
                    let half = o.width.mm() / 2.0;
                    let len = w.length().mm();
                    if o.position.mm() - half < -0.5 || o.position.mm() + half > len + 0.5 {
                        c.issue(id, "opening does not fit within its wall");
                    }
                    if let Some(h) = doc.wall_height(w)
                        && (o.sill + o.height).mm() > h.mm() + 0.5
                    {
                        c.issue(id, "opening is taller than its wall");
                    }
                }
            }
            Object::RoomType(t) => {
                if let Some(h) = t.ceiling_height {
                    c.positive(id, "ceiling_height", h);
                }
            }
            Object::Room(r) => {
                c.reference(id, "floor", r.floor, K::Floor);
                c.reference(id, "room_type", r.room_type, K::RoomType);
                c.reference(id, "layer", r.layer, K::Layer);
                if let Some(h) = r.ceiling_height {
                    c.positive(id, "ceiling_height", h);
                }
                if let CeilingStyle::Tray { inset, rise } = r.ceiling {
                    c.positive(id, "ceiling.inset", inset);
                    c.positive(id, "ceiling.rise", rise);
                }
            }
            Object::Layer(l) => {
                if !(l.line_weight.is_finite() && l.line_weight >= 0.0) {
                    c.issue(id, "line_weight must not be negative");
                }
                if let Some(s) = l.text_style {
                    c.reference(id, "text_style", s, K::TextStyle);
                }
            }
            Object::LayerSet(s) => {
                for layer in s.layers.keys() {
                    c.reference(id, "layers", *layer, K::Layer);
                }
            }
            Object::TextStyle(s) => c.positive(id, "height", s.height),
            Object::DefaultSet(d) => {
                c.reference(id, "exterior_wall_type", d.exterior_wall_type, K::WallType);
                c.reference(id, "interior_wall_type", d.interior_wall_type, K::WallType);
                c.reference(id, "room_type", d.room_type, K::RoomType);
                c.reference(id, "text_style", d.text_style, K::TextStyle);
                c.reference(
                    id,
                    "dimensions.text_style",
                    d.dimensions.text_style,
                    K::TextStyle,
                );
                let l = &d.layers;
                for (field, layer) in [
                    ("layers.walls", l.walls),
                    ("layers.doors", l.doors),
                    ("layers.windows", l.windows),
                    ("layers.rooms", l.rooms),
                    ("layers.cad", l.cad),
                    ("layers.dimensions", l.dimensions),
                    ("layers.text", l.text),
                ] {
                    c.reference(id, field, layer, K::Layer);
                }
                c.positive(id, "door.width", d.door.width);
                c.positive(id, "door.height", d.door.height);
                c.positive(id, "window.width", d.window.width);
                c.positive(id, "window.height", d.window.height);
                if d.window.head_height.mm() < d.window.height.mm() {
                    c.issue(id, "window.head_height is below window.height");
                }
            }
            Object::Cad(cad) => {
                c.reference(id, "floor", cad.floor, K::Floor);
                c.reference(id, "layer", cad.layer, K::Layer);
                match &cad.shape {
                    Shape::Point { at } => c.finite(id, "point", &at.mm()),
                    Shape::Line { a, b, .. } => {
                        c.finite(id, "line", &[a.mm(), b.mm()].concat());
                        if a.distance(*b).mm() < 1e-6 {
                            c.issue(id, "line has zero length");
                        }
                    }
                    Shape::Polyline { points, closed } | Shape::Spline { points, closed } => {
                        let need = if *closed { 3 } else { 2 };
                        if points.len() < need {
                            c.issue(id, format!("needs at least {need} points"));
                        }
                        let flat: Vec<f64> = points.iter().flat_map(|p| p.mm()).collect();
                        c.finite(id, "points", &flat);
                    }
                    Shape::Arc {
                        center,
                        radius,
                        start_angle,
                        sweep,
                    } => {
                        c.positive(id, "radius", *radius);
                        c.finite(
                            id,
                            "arc",
                            &[center.mm()[0], center.mm()[1], *start_angle, *sweep],
                        );
                        if sweep.abs() < 1e-9 || sweep.abs() > 360.0 {
                            c.issue(id, "arc sweep must be non-zero and at most 360°");
                        }
                    }
                    Shape::Circle { center, radius } => {
                        c.positive(id, "radius", *radius);
                        c.finite(id, "center", &center.mm());
                    }
                    Shape::Ellipse {
                        center,
                        rx,
                        ry,
                        rotation,
                    } => {
                        c.positive(id, "rx", *rx);
                        c.positive(id, "ry", *ry);
                        c.finite(id, "ellipse", &[center.mm()[0], center.mm()[1], *rotation]);
                    }
                }
            }
            Object::Dimension(d) => {
                c.reference(id, "floor", d.floor, K::Floor);
                c.reference(id, "layer", d.layer, K::Layer);
                match &d.geometry {
                    DimensionGeometry::Linear { points, offset, .. } => {
                        if points.len() < 2 {
                            c.issue(id, "a linear dimension needs at least two points");
                        }
                        let flat: Vec<f64> = points.iter().flat_map(|p| p.mm()).collect();
                        c.finite(id, "points", &flat);
                        c.finite(id, "offset", &[offset.mm()]);
                    }
                    DimensionGeometry::Angular {
                        center,
                        start,
                        end,
                        radius,
                    } => {
                        c.positive(id, "radius", *radius);
                        if center.distance(*start).mm() < 1e-6 || center.distance(*end).mm() < 1e-6
                        {
                            c.issue(id, "angular dimension arms must not be zero length");
                        }
                    }
                }
            }
            Object::Text(t) => {
                c.reference(id, "floor", t.floor, K::Floor);
                c.reference(id, "layer", t.layer, K::Layer);
                if let Some(s) = t.style {
                    c.reference(id, "style", s, K::TextStyle);
                }
                if let Some(h) = t.height {
                    c.positive(id, "height", h);
                }
                c.finite(
                    id,
                    "position",
                    &[t.position.mm()[0], t.position.mm()[1], t.rotation],
                );
            }
            Object::PlanView(v) => {
                if let Some(f) = v.floor {
                    c.reference(id, "floor", f, K::Floor);
                }
                c.reference(id, "layer_set", v.layer_set, K::LayerSet);
                c.reference(id, "default_set", v.default_set, K::DefaultSet);
                if let Some(z) = &v.zoom
                    && !(z.mm_per_point.is_finite() && z.mm_per_point > 0.0)
                {
                    c.issue(id, "zoom.mm_per_point must be positive");
                }
            }
            Object::Camera(cam) => {
                c.reference(id, "layer_set", cam.layer_set, K::LayerSet);
                match cam.projection {
                    Projection::Perspective { fov } => {
                        if !(1.0..=170.0).contains(&fov) {
                            c.issue(id, "fov must be between 1° and 170°");
                        }
                    }
                    Projection::Orthographic { height } => c.positive(id, "height", height),
                }
                let (e, t) = (cam.eye.mm(), cam.target.mm());
                c.finite(id, "eye/target", &[e, t].concat());
                if e == t {
                    c.issue(id, "eye and target must differ");
                }
            }
        }
    }
    c.issues
}
