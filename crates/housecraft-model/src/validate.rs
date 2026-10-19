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

    /// A drafting object's `floor`: a floor plan, elevation/section view or
    /// CAD detail, or (for shapes inside a block) a block.
    fn sheet(&mut self, from: Id, to: Id, block: bool) {
        let ok = self.doc.is_sheet(to)
            || (block && self.doc.kind_of(to) == Some(ObjectKind::Block));
        if !ok {
            match self.doc.kind_of(to) {
                Some(k) => self.issue(
                    from,
                    format!("floor {to} is a {k}, expected a floor, view or CAD detail"),
                ),
                None => self.issue(from, format!("floor references missing sheet {to}")),
            }
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
    for (field, set) in [
        ("elevation", doc.settings.view_defaults.elevation),
        ("section", doc.settings.view_defaults.section),
        ("detail", doc.settings.view_defaults.detail),
    ] {
        if let Some(set) = set {
            c.reference(Id(0), &format!("settings.view_defaults.{field}"), set, K::DefaultSet);
        }
    }
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
                let dt = &o.details;
                if let Some(b) = dt.plan_block {
                    c.reference(id, "plan_block", b, K::Block);
                }
                if let Some(m) = &dt.muntins {
                    if m.across == 0 || m.up == 0 {
                        c.issue(id, "muntins need at least one pane across and up");
                    }
                    c.positive(id, "muntin width", m.width);
                    if m.custom.iter().flatten().flatten().any(|v| !v.is_finite()) {
                        c.issue(id, "muntin points must be finite");
                    }
                }
                if let Some(w) = dt.shutters.as_ref().and_then(|s| s.width) {
                    c.positive(id, "shutter width", w);
                }
                if let Some(t) = &dt.treatment
                    && !(0.0..=1.0).contains(&t.open)
                {
                    c.issue(id, "a window treatment opens from 0 to 1");
                }
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
                if let Some(l) = d.layers.cabinets {
                    c.reference(id, "layers.cabinets", l, K::Layer);
                }
                for (surface, m) in &d.materials {
                    c.reference(id, &format!("materials.{}", surface.name()), *m, K::Material);
                }
                c.positive(id, "window.width", d.window.width);
                c.positive(id, "window.height", d.window.height);
                if d.window.head_height.mm() < d.window.height.mm() {
                    c.issue(id, "window.head_height is below window.height");
                }
            }
            Object::Cad(cad) => {
                c.sheet(id, cad.floor, true);
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
                    Shape::CrossBox { a, b, width }
                    | Shape::BlockingBox { a, b, width }
                    | Shape::Insulation { a, b, width } => {
                        c.positive(id, "width", *width);
                        c.finite(id, "a/b", &[a.mm()[0], a.mm()[1], b.mm()[0], b.mm()[1]]);
                        if a.distance(*b).mm() < 1.0 {
                            c.issue(id, "a detail shape needs two distinct points");
                        }
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
                c.sheet(id, d.floor, false);
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
                c.sheet(id, t.floor, false);
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
                if let Some(l) = &e.light {
                    if !(l.lumens.is_finite() && l.lumens >= 0.0) {
                        c.issue(id, "light lumens must not be negative");
                    }
                    if !(0.0..=1.0).contains(&l.dimmer) {
                        c.issue(id, "light dimmer must be between 0 and 1");
                    }
                    if !(1000.0..=40000.0).contains(&l.kelvin) {
                        c.issue(id, "light colour temperature must be 1000–40000 K");
                    }
                    if !e.electrical_kind.emits_light() {
                        c.issue(id, "only light fixtures have light settings");
                    }
                }
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
                for s in [st.baluster, st.newel].into_iter().flatten() {
                    c.reference(id, "symbol", s, K::SymbolDef);
                }
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
                    LayoutSource::Detail { detail } => {
                        c.reference(id, "source.detail", *detail, K::CadDetail)
                    }
                    LayoutSource::Schedule { floor: Some(f), .. } => {
                        c.reference(id, "source.floor", *f, K::Floor)
                    }
                    _ => {}
                }
            }
            Object::Callout(co) => {
                c.sheet(id, co.floor, false);
                c.reference(id, "layer", co.layer, K::Layer);
                c.finite(id, "at", &co.at.mm());
                if let Some(t) = co.target {
                    c.finite(id, "target", &t.mm());
                }
                if let Some(h) = co.height {
                    c.positive(id, "height", h);
                }
            }
            Object::RevisionCloud(rc) => {
                c.sheet(id, rc.floor, false);
                c.reference(id, "layer", rc.layer, K::Layer);
                c.positive(id, "arc", rc.arc);
                if rc.points.len() < 3 {
                    c.issue(id, "a revision cloud needs at least three points");
                }
                let flat: Vec<f64> = rc.points.iter().flat_map(|p| p.mm()).collect();
                c.finite(id, "points", &flat);
            }
            Object::CadDetail(_) | Object::Block(_) => {}
            Object::Material(m) => {
                for (field, v) in [
                    ("roughness", m.roughness),
                    ("metallic", m.metallic),
                    ("opacity", m.opacity),
                ] {
                    if !(0.0..=1.0).contains(&v) {
                        c.issue(id, format!("{field} must be between 0 and 1"));
                    }
                }
                if !(m.emissive.is_finite() && m.emissive >= 0.0) {
                    c.issue(id, "emissive must not be negative");
                }
                if !(-1.0..=1.0).contains(&m.anisotropy) {
                    c.issue(id, "anisotropy must be between -1 and 1");
                }
                for t in [m.texture_width, m.texture_height].into_iter().flatten() {
                    c.positive(id, "texture size", t);
                }
            }
            Object::MaterialAssignment(a) => {
                if doc.object(a.object).is_none() {
                    c.issue(id, format!("object references missing object {}", a.object));
                }
                c.reference(id, "material", a.material, K::Material);
            }
            Object::Cabinet(cab) => {
                c.reference(id, "floor", cab.floor, K::Floor);
                c.reference(id, "layer", cab.layer, K::Layer);
                c.positive(id, "width", cab.width);
                c.positive(id, "depth", cab.depth);
                c.positive(id, "height", cab.height);
                c.non_negative(id, "elevation", cab.elevation);
                c.non_negative(id, "filler_left", cab.filler_left);
                c.non_negative(id, "filler_right", cab.filler_right);
                if !(0.0..=1.0).contains(&cab.open) {
                    c.issue(id, "open must be between 0 and 1");
                }
                c.positive(id, "build.side_thickness", cab.build.side_thickness);
                c.positive(id, "build.back_thickness", cab.build.back_thickness);
                for m in &cab.moldings {
                    c.positive(id, "molding height", m.height);
                    c.positive(id, "molding projection", m.projection);
                }
                c.finite(id, "position", &cab.position.mm());
                if cab.face.iter().any(|r| r.items.is_empty()) {
                    c.issue(id, "every face row needs at least one door, drawer or opening");
                }
                if let Some((h, d)) = cab.toe_kick {
                    c.non_negative(id, "toe_kick height", h);
                    c.non_negative(id, "toe_kick depth", d);
                }
                if let Some(t) = &cab.countertop {
                    c.positive(id, "countertop.thickness", t.thickness);
                    c.non_negative(id, "countertop.overhang", t.overhang);
                    c.non_negative(id, "countertop.backsplash", t.backsplash);
                }
                match cab.shape {
                    CabinetShape::Angled { left, right } => {
                        c.non_negative(id, "shape.left", left);
                        c.non_negative(id, "shape.right", right);
                        if (left + right).mm() >= cab.width.mm() || left.mm().max(right.mm()) >= cab.depth.mm() {
                            c.issue(id, "angled corners must fit within the cabinet");
                        }
                    }
                    CabinetShape::Bow { depth } => c.positive(id, "shape.depth", depth),
                    CabinetShape::Curved { radius } => {
                        if radius.mm().abs() <= cab.depth.mm() {
                            c.issue(id, "a curved cabinet's radius must exceed its depth");
                        }
                    }
                    CabinetShape::Standard => {}
                }
            }
            Object::MaterialRegion(r) => {
                c.reference(id, "floor", r.floor, K::Floor);
                c.reference(id, "layer", r.layer, K::Layer);
                c.reference(id, "material", r.material, K::Material);
                match &r.target {
                    RegionTarget::Floor { points } => {
                        if points.len() < 3 {
                            c.issue(id, "a floor region needs at least three points");
                        }
                        let flat: Vec<f64> = points.iter().flat_map(|p| p.mm()).collect();
                        c.finite(id, "points", &flat);
                    }
                    RegionTarget::Wall { wall, u0, u1, z0, z1, .. } => {
                        c.reference(id, "wall", *wall, K::Wall);
                        if u1.mm() <= u0.mm() || z1.mm() <= z0.mm() {
                            c.issue(id, "a wall region needs a positive width and height");
                        }
                    }
                }
            }
            Object::StylePalette(st) => {
                for m in st.materials.values() {
                    c.reference(id, "materials", *m, K::Material);
                }
            }
            Object::MoldingProfile(p) => {
                if p.points.len() < 3 {
                    c.issue(id, "a molding profile needs at least three points");
                }
                if p.points.iter().flatten().any(|v| !v.is_finite()) {
                    c.issue(id, "profile points must be finite");
                }
            }
            Object::MoldingPolyline(m) => {
                c.reference(id, "floor", m.floor, K::Floor);
                c.reference(id, "layer", m.layer, K::Layer);
                if m.points.len() < 2 {
                    c.issue(id, "a molding needs at least two points");
                }
                if m.stack.is_empty() {
                    c.issue(id, "a molding needs at least one profile");
                }
                for s in &m.stack {
                    c.positive(id, "molding height", s.height);
                    c.positive(id, "molding depth", s.depth);
                    if let ProfileRef::Custom { profile } = s.profile {
                        c.reference(id, "profile", profile, K::MoldingProfile);
                    }
                }
            }
            Object::FramingMember(m) => {
                c.reference(id, "floor", m.floor, K::Floor);
                c.reference(id, "layer", m.layer, K::Layer);
                c.positive(id, "width", m.width);
                c.positive(id, "depth", m.depth);
                let flat: Vec<f64> = m.a.mm().into_iter().chain(m.b.mm()).chain([m.rotation]).collect();
                c.finite(id, "points", &flat);
                if m.length().mm() < 1.0 {
                    c.issue(id, "a framing member needs some length");
                }
            }
            Object::Walkthrough(w) => {
                if w.keyframes.is_empty() {
                    c.issue(id, "a walkthrough needs at least one keyframe");
                }
                if !(w.fps > 0.0 && w.fps <= 120.0) {
                    c.issue(id, "frames per second must be between 0 and 120");
                }
                let flat: Vec<f64> = w
                    .keyframes
                    .iter()
                    .flat_map(|k| k.eye.mm().into_iter().chain(k.target.mm()).chain([k.seconds, k.pause]))
                    .collect();
                c.finite(id, "keyframes", &flat);
                if w.keyframes.iter().any(|k| k.seconds < 0.0 || k.pause < 0.0) {
                    c.issue(id, "keyframe times can't be negative");
                }
            }
            Object::Deck(dk) => {
                c.reference(id, "floor", dk.floor, K::Floor);
                c.reference(id, "layer", dk.layer, K::Layer);
                if dk.outline.len() < 3 {
                    c.issue(id, "a deck outline needs at least three points");
                }
                for (field, v) in [
                    ("plank_width", dk.plank_width),
                    ("plank_thickness", dk.plank_thickness),
                    ("joist_spacing", dk.joist_spacing),
                    ("joist_depth", dk.joist_depth),
                    ("max_span", dk.max_span),
                    ("post_spacing", dk.post_spacing),
                    ("railing_height", dk.railing_height),
                ] {
                    c.positive(id, field, v);
                }
                c.non_negative(id, "plank_gap", dk.plank_gap);
                let flat: Vec<f64> = dk.outline.iter().flat_map(|p| p.mm()).collect();
                c.finite(id, "outline", &flat);
            }
            Object::JoistDirection(j) => {
                c.reference(id, "floor", j.floor, K::Floor);
                c.reference(id, "layer", j.layer, K::Layer);
                c.finite(id, "at", &j.at.mm().into_iter().chain([j.angle]).collect::<Vec<_>>());
            }
            Object::SiteFeature(f) => {
                c.reference(id, "floor", f.floor, K::Floor);
                c.reference(id, "layer", f.layer, K::Layer);
                let min = if f.feature.closed() { 3 } else { 2 };
                if f.points.len() < min {
                    c.issue(id, format!("a {} needs at least {min} points", f.feature.name().to_lowercase()));
                }
                let flat: Vec<f64> = f.points.iter().flat_map(|p| p.mm()).collect();
                c.finite(id, "points", &flat);
                match &f.feature {
                    SiteKind::RetainingWall { thickness, .. } => c.positive(id, "thickness", *thickness),
                    SiteKind::Road { width, thickness, .. } => {
                        c.positive(id, "width", *width);
                        c.positive(id, "thickness", *thickness);
                    }
                    SiteKind::Fence { height, post_spacing, gates, .. } => {
                        c.positive(id, "height", *height);
                        c.positive(id, "post_spacing", *post_spacing);
                        for g in gates {
                            c.positive(id, "gate width", g.width);
                            c.non_negative(id, "gate position", g.at);
                        }
                    }
                    SiteKind::GardenBed { height } => c.positive(id, "height", *height),
                    SiteKind::SteppingStones { spacing, size } => {
                        c.positive(id, "spacing", *spacing);
                        c.positive(id, "size", *size);
                    }
                    SiteKind::Stream { width } => c.positive(id, "width", *width),
                }
            }
            Object::Plant(p) => {
                c.reference(id, "floor", p.floor, K::Floor);
                c.reference(id, "layer", p.layer, K::Layer);
                c.positive(id, "height", p.height);
                c.positive(id, "spread", p.spread);
                c.finite(id, "at", &p.at.mm());
                if p.age < 0.0 || p.years_to_mature < 0.0 {
                    c.issue(id, "age and years to mature can't be negative");
                }
            }
            Object::Terrain(t) => {
                c.reference(id, "floor", t.floor, K::Floor);
                c.reference(id, "layer", t.layer, K::Layer);
                if t.perimeter.len() < 3 {
                    c.issue(id, "a terrain outline needs at least three points");
                }
                c.non_negative(id, "skirt", t.skirt);
                c.non_negative(id, "contour_interval", t.contour_interval);
                let flat: Vec<f64> = t.perimeter.iter().flat_map(|p| p.mm()).collect();
                c.finite(id, "perimeter", &flat);
            }
            Object::ElevationData(e) => {
                c.reference(id, "floor", e.floor, K::Floor);
                c.reference(id, "layer", e.layer, K::Layer);
                let points: Vec<housecraft_units::Point2> = match &e.shape {
                    ElevationShape::Point { at, .. } => vec![*at],
                    ElevationShape::Line { points, .. } => {
                        if points.len() < 2 {
                            c.issue(id, "an elevation line needs at least two points");
                        }
                        points.clone()
                    }
                    ElevationShape::Region { points, .. } => {
                        if points.len() < 3 {
                            c.issue(id, "a flat region needs at least three points");
                        }
                        points.clone()
                    }
                    ElevationShape::Hill { center, radius, .. } | ElevationShape::Valley { center, radius, .. } => {
                        c.positive(id, "radius", *radius);
                        vec![*center]
                    }
                };
                let flat: Vec<f64> = points.iter().flat_map(|p| p.mm()).collect();
                c.finite(id, "points", &flat);
            }
            Object::Solid(x) => {
                c.reference(id, "floor", x.floor, K::Floor);
                c.reference(id, "layer", x.layer, K::Layer);
                match &x.shape {
                    SolidShape::Extrude { points, height } => {
                        c.positive(id, "height", *height);
                        if points.len() < 3 {
                            c.issue(id, "an extruded solid needs at least three points");
                        }
                        let flat: Vec<f64> = points.iter().flat_map(|p| p.mm()).collect();
                        c.finite(id, "points", &flat);
                    }
                    SolidShape::Revolve { center, profile, sweep } => {
                        if profile.len() < 2 {
                            c.issue(id, "a revolved solid needs at least two profile points");
                        }
                        if profile.iter().any(|p| p[0].mm() < 0.0) {
                            c.issue(id, "profile radii can't be negative");
                        }
                        if !(*sweep > 0.0 && *sweep <= 360.0) {
                            c.issue(id, "sweep must be more than 0 and at most 360 degrees");
                        }
                        let flat: Vec<f64> =
                            profile.iter().flatten().map(|l| l.mm()).chain(center.mm()).collect();
                        c.finite(id, "profile", &flat);
                    }
                }
            }
            Object::Backsplash(b) => {
                c.reference(id, "floor", b.floor, K::Floor);
                c.reference(id, "layer", b.layer, K::Layer);
                c.positive(id, "height", b.height);
                c.positive(id, "thickness", b.thickness);
                c.non_negative(id, "bottom", b.bottom);
                if b.points.len() < 2 {
                    c.issue(id, "a backsplash needs at least two points");
                }
                let flat: Vec<f64> = b.points.iter().flat_map(|p| p.mm()).collect();
                c.finite(id, "points", &flat);
            }
            Object::Countertop(t) => {
                c.reference(id, "floor", t.floor, K::Floor);
                c.reference(id, "layer", t.layer, K::Layer);
                c.positive(id, "thickness", t.thickness);
                c.non_negative(id, "height", t.height);
                if t.points.len() < 3 {
                    c.issue(id, "a countertop needs at least three points");
                }
                let flat: Vec<f64> = t.points.iter().flat_map(|p| p.mm()).collect();
                c.finite(id, "points", &flat);
                for h in &t.holes {
                    c.positive(id, "hole width", h.width);
                    if h.hole_kind != HoleKind::Round {
                        c.positive(id, "hole depth", h.depth);
                    }
                }
            }
            Object::Appliance(a) => {
                c.reference(id, "floor", a.floor, K::Floor);
                c.reference(id, "layer", a.layer, K::Layer);
                c.positive(id, "width", a.width);
                c.positive(id, "depth", a.depth);
                c.positive(id, "height", a.height);
                c.non_negative(id, "elevation", a.elevation);
                c.finite(id, "position", &a.position.mm());
            }
            Object::Picture(p) => {
                c.sheet(id, p.floor, false);
                c.reference(id, "layer", p.layer, K::Layer);
                c.positive(id, "width", p.width);
                c.positive(id, "height", p.height);
                c.finite(id, "position", &p.position.mm());
                if !(0.0..=1.0).contains(&p.opacity) {
                    c.issue(id, "opacity must be between 0 and 1");
                }
                if p.path.trim().is_empty() {
                    c.issue(id, "a picture needs an image file");
                }
            }
            Object::SymbolDef(s) => {
                let n = s.positions.len() as u32;
                if s.triangles.iter().any(|t| t.iter().any(|i| *i >= n)) {
                    c.issue(id, "a triangle refers to a missing point");
                }
                if s.positions.iter().flatten().any(|v| !v.is_finite()) {
                    c.issue(id, "points must be finite");
                }
                let t = s.triangles.len() as u64;
                if s.groups.iter().any(|g| g.first as u64 + g.count as u64 > t) {
                    c.issue(id, "a colour group runs past the last triangle");
                }
            }
            Object::Symbol(s) => {
                c.reference(id, "floor", s.floor, K::Floor);
                c.reference(id, "layer", s.layer, K::Layer);
                c.reference(id, "def", s.def, K::SymbolDef);
                c.positive(id, "width", s.width);
                c.positive(id, "depth", s.depth);
                c.positive(id, "height", s.height);
                c.non_negative(id, "elevation", s.elevation);
                c.finite(id, "position", &s.position.mm());
            }
            Object::BlockRef(b) => {
                c.sheet(id, b.floor, false);
                c.reference(id, "layer", b.layer, K::Layer);
                c.reference(id, "block", b.block, K::Block);
                c.finite(id, "at", &b.at.mm());
                if !(b.scale.is_finite() && b.scale > 0.0) {
                    c.issue(id, "scale must be positive");
                }
                if !b.rotation.is_finite() {
                    c.issue(id, "rotation must be finite");
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
