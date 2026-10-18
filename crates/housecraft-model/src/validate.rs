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
                for l in &f.platform {
                    c.positive(id, "platform layer thickness", l.thickness);
                }
                if let Some(fd) = &f.foundation {
                    c.positive(id, "foundation.wall_height", fd.wall_height);
                    c.positive(id, "foundation.wall_thickness", fd.wall_thickness);
                    c.non_negative(id, "foundation.footing_width", fd.footing_width);
                    c.non_negative(id, "foundation.footing_depth", fd.footing_depth);
                    c.positive(id, "foundation.slab_thickness", fd.slab_thickness);
                }
            }
            Object::WallType(t) => {
                if let Some(r) = t.railing {
                    c.positive(id, "railing.height", r.height);
                    c.positive(id, "railing.baluster_spacing", r.baluster_spacing);
                }
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
                if let Some(p) = w.roof.pitch
                    && !(p > 0.0 && p < 90.0)
                {
                    c.issue(id, "roof.pitch must be between 0 and 90 degrees");
                }
                if let Some(f) = w.footing {
                    c.positive(id, "footing.width", f.width);
                    c.positive(id, "footing.depth", f.depth);
                }
            }
            Object::Opening(o) => {
                if !(0.0..=180.0).contains(&o.plan_swing) {
                    c.issue(id, "plan_swing must be between 0 and 180 degrees");
                }
                if !(0.0..=100.0).contains(&o.open_percent) {
                    c.issue(id, "open_percent must be between 0 and 100");
                }
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
                match r.ceiling {
                    CeilingStyle::Tray { inset, rise } => {
                        c.positive(id, "ceiling.inset", inset);
                        c.positive(id, "ceiling.rise", rise);
                    }
                    CeilingStyle::Coffered {
                        spacing,
                        beam_width,
                        depth,
                    } => {
                        c.positive(id, "ceiling.spacing", spacing);
                        c.positive(id, "ceiling.beam_width", beam_width);
                        c.positive(id, "ceiling.depth", depth);
                        if beam_width >= spacing {
                            c.issue(id, "coffer beams must be narrower than their spacing");
                        }
                    }
                    CeilingStyle::Barrel { rise } => c.positive(id, "ceiling.rise", rise),
                    _ => {}
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
                if !(d.roof.pitch > 0.0 && d.roof.pitch < 90.0) {
                    c.issue(id, "roof.pitch must be between 0 and 90 degrees");
                }
                if let Some(l) = d.layers.roofs {
                    c.reference(id, "layers.roofs", l, K::Layer);
                }
                if let Some(l) = d.layers.stairs {
                    c.reference(id, "layers.stairs", l, K::Layer);
                }
                if let Some(l) = d.layers.electrical {
                    c.reference(id, "layers.electrical", l, K::Layer);
                }
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
                for f in &v.references {
                    c.reference(id, "references", *f, K::Floor);
                }
                c.reference(id, "default_set", v.default_set, K::DefaultSet);
                if let Some(z) = &v.zoom
                    && !(z.mm_per_point.is_finite() && z.mm_per_point > 0.0)
                {
                    c.issue(id, "zoom.mm_per_point must be positive");
                }
            }
            Object::RoofPlane(r) => {
                c.reference(id, "floor", r.floor, K::Floor);
                c.reference(id, "layer", r.layer, K::Layer);
                if r.outline.len() < 3 {
                    c.issue(id, "a roof plane needs at least three points");
                }
                let flat: Vec<f64> = r.outline.iter().flat_map(|p| p.mm()).collect();
                c.finite(id, "outline", &flat);
                if !(r.pitch > 0.0 && r.pitch <= 90.0) {
                    c.issue(id, "pitch must be between 0 and 90 degrees");
                }
                c.non_negative(id, "thickness", r.thickness);
            }
            Object::Schedule(s) => {
                c.reference(id, "floor", s.floor, K::Floor);
                c.reference(id, "layer", s.layer, K::Layer);
                c.finite(id, "position", &s.position.mm());
                if let Some(h) = s.text_height {
                    c.positive(id, "text_height", h);
                }
            }
            Object::Electrical(e) => {
                c.reference(id, "floor", e.floor, K::Floor);
                c.reference(id, "layer", e.layer, K::Layer);
                c.finite(
                    id,
                    "position",
                    &[e.position.mm()[0], e.position.mm()[1], e.rotation],
                );
                if let Some(h) = e.height {
                    c.non_negative(id, "height", h);
                }
                if !e.controls.is_empty() && !e.electrical_kind.is_switch() {
                    c.issue(id, "only switches control other fixtures");
                }
                for t in &e.controls {
                    match c.doc.get::<Electrical>(*t) {
                        None => c.issue(
                            id,
                            format!("controls {t}, which is not an electrical object"),
                        ),
                        Some(x) if x.electrical_kind.is_switch() => {
                            c.issue(id, format!("controls {t}, which is a switch"))
                        }
                        Some(_) => {}
                    }
                }
            }
            Object::Stair(st) => {
                c.reference(id, "floor", st.floor, K::Floor);
                c.reference(id, "layer", st.layer, K::Layer);
                c.positive(id, "width", st.width);
                c.positive(id, "max_riser", st.max_riser);
                c.positive(id, "tread_depth", st.tread_depth);
                c.positive(id, "railing_height", st.railing_height);
                c.non_negative(id, "tread_overhang", st.tread_overhang);
                if let Some(r) = st.rise {
                    c.positive(id, "rise", r);
                }
                c.finite(
                    id,
                    "start",
                    &[st.start.mm()[0], st.start.mm()[1], st.direction],
                );
                match st.shape {
                    StairShape::Straight => {}
                    StairShape::L { first_treads, .. } | StairShape::U { first_treads, .. } => {
                        if first_treads == 0 {
                            c.issue(id, "the first flight needs at least one tread");
                        }
                    }
                }
            }
            Object::SectionView(v) => {
                c.reference(id, "floor", v.floor, K::Floor);
                c.reference(id, "layer", v.layer, K::Layer);
                let (a, b) = (v.a.mm(), v.b.mm());
                c.finite(id, "a/b", &[a[0], a[1], b[0], b[1]]);
                if (a[0] - b[0]).hypot(a[1] - b[1]) < 1.0 {
                    c.issue(id, "a section line needs two distinct points");
                }
                if let Some(d) = v.depth {
                    c.positive(id, "depth", d);
                }
            }
            Object::Layout(l) => {
                c.positive(id, "paper_width", l.paper_width);
                c.positive(id, "paper_height", l.paper_height);
                if l.pages.is_empty() {
                    c.issue(id, "a layout needs at least one page");
                }
            }
            Object::LayoutBox(b) => {
                c.reference(id, "layout", b.layout, K::Layout);
                if let Some(l) = c.doc.get::<Layout>(b.layout)
                    && b.page as usize >= l.pages.len()
                {
                    c.issue(
                        id,
                        format!(
                            "page {} is past the layout's {} pages",
                            b.page + 1,
                            l.pages.len()
                        ),
                    );
                }
                c.positive(id, "width", b.width);
                c.positive(id, "height", b.height);
                c.finite(id, "x/y/scale", &[b.x.mm(), b.y.mm(), b.scale]);
                if b.scale < 0.0 {
                    c.issue(id, "scale must not be negative");
                }
                match &b.source {
                    LayoutSource::Plan {
                        floor, layer_set, ..
                    } => {
                        c.reference(id, "source.floor", *floor, K::Floor);
                        if let Some(s) = layer_set {
                            c.reference(id, "source.layer_set", *s, K::LayerSet);
                        }
                    }
                    LayoutSource::View { view } => {
                        c.reference(id, "source.view", *view, K::SectionView)
                    }
                    LayoutSource::Schedule { floor: Some(f), .. } => {
                        c.reference(id, "source.floor", *f, K::Floor)
                    }
                    _ => {}
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
