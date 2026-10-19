//! Specification windows (Open Object): typed editors for each object kind.
//! Every edit becomes an `update` (or a dedicated) command, so it is undoable
//! and identical to what the CLI and MCP server would do.

use egui::Ui;
use housecraft_api::model::*;
use housecraft_api::units::Length;
use housecraft_api::{Command, LengthAnchor};
use serde_json::{Value, json};

use crate::app::App;
use crate::util::{EditBuffers, fmt, from_color32, to_color32};

fn patch(id: Id, v: Value) -> Command {
    Command::Update {
        id,
        set: v.as_object().cloned().unwrap_or_default(),
    }
}

struct Ed<'a> {
    doc: &'a Document,
    bufs: &'a mut EditBuffers,
    id: Id,
    out: Vec<Command>,
}

impl Ed<'_> {
    fn key(&self, field: &str) -> egui::Id {
        egui::Id::unique(("spec", self.id, field))
    }

    fn label(ui: &mut Ui, text: &str) {
        ui.label(text);
    }

    fn length(&mut self, ui: &mut Ui, label: &str, field: &str, value: Length) -> Option<Length> {
        Self::label(ui, label);
        let key = self.key(field);
        let r = self.bufs.length(ui, self.doc, key, value);
        ui.end_row();
        r
    }

    /// An optional length: "Default" checkbox, or a value.
    fn opt_length(
        &mut self,
        ui: &mut Ui,
        label: &str,
        field: &str,
        value: Option<Length>,
        fallback: Option<Length>,
    ) {
        Self::label(ui, label);
        ui.horizontal(|ui| {
            let mut is_default = value.is_none();
            if ui.checkbox(&mut is_default, "Default").changed() {
                let v = if is_default {
                    Value::Null
                } else {
                    json!(fallback.unwrap_or(Length::from_feet(8.0)))
                };
                self.out.push(patch(self.id, json!({ field: v })));
            }
            match value {
                Some(v) => {
                    let key = self.key(field);
                    if let Some(l) = self.bufs.length(ui, self.doc, key, v) {
                        self.out.push(patch(self.id, json!({ field: l })));
                    }
                }
                None => {
                    ui.weak(fallback.map_or_else(String::new, |f| fmt(self.doc, f)));
                }
            }
        });
        ui.end_row();
    }

    fn text(&mut self, ui: &mut Ui, label: &str, field: &str, value: &str) {
        Self::label(ui, label);
        let key = self.key(field);
        if let Some(t) = self.bufs.text(ui, key, value, 180.0) {
            self.out.push(patch(self.id, json!({ field: t })));
        }
        ui.end_row();
    }

    /// Choose one of `options` for an enum field.
    fn choice<T: PartialEq + Copy + serde::Serialize>(
        &mut self,
        ui: &mut Ui,
        label: &str,
        field: &str,
        current: T,
        options: &[(T, &str)],
    ) {
        Self::label(ui, label);
        let name = options
            .iter()
            .find(|(v, _)| *v == current)
            .map_or("?", |(_, n)| *n);
        let mut chosen = current;
        egui::ComboBox::from_id_salt(self.key(field))
            .selected_text(name)
            .show_ui(ui, |ui| {
                for (v, n) in options {
                    ui.selectable_value(&mut chosen, *v, *n);
                }
            });
        if chosen != current {
            self.out.push(patch(self.id, json!({ field: chosen })));
        }
        ui.end_row();
    }

    fn check(&mut self, ui: &mut Ui, label: &str, field: &str, text: &str, value: bool) {
        Self::label(ui, label);
        let mut v = value;
        if ui.checkbox(&mut v, text).changed() {
            self.out.push(patch(self.id, json!({ field: v })));
        }
        ui.end_row();
    }

    /// Edit a text field of another object (e.g. a placed block's block).
    fn text_of(&mut self, ui: &mut Ui, other: Id, label: &str, field: &str, value: &str) {
        Self::label(ui, label);
        let key = egui::Id::unique(("spec", other, field));
        if let Some(t) = self.bufs.text(ui, key, value, 180.0) {
            self.out.push(patch(other, json!({ field: t })));
        }
        ui.end_row();
    }

    fn number(&mut self, ui: &mut Ui, label: &str, field: &str, value: f64) -> Option<f64> {
        Self::label(ui, label);
        let key = self.key(field);
        let r = self.bufs.number(ui, key, value);
        ui.end_row();
        r
    }

    /// Pick an object of type `T` by name.
    fn pick<T: ObjectData + Named>(
        &mut self,
        ui: &mut Ui,
        label: &str,
        field: &str,
        current: Option<Id>,
        allow_none: Option<&str>,
    ) {
        Self::label(ui, label);
        let doc = self.doc;
        let name = |id: Option<Id>| {
            id.and_then(|i| doc.get::<T>(i)).map_or_else(
                || allow_none.unwrap_or("").to_string(),
                |t| t.name().to_string(),
            )
        };
        let mut chosen = current;
        egui::ComboBox::from_id_salt(self.key(field))
            .selected_text(name(current))
            .show_ui(ui, |ui| {
                if let Some(none) = allow_none {
                    ui.selectable_value(&mut chosen, None, none);
                }
                for t in doc.iter::<T>() {
                    ui.selectable_value(&mut chosen, Some(t.id()), t.name());
                }
            });
        if chosen != current {
            let v = chosen.map_or(Value::Null, |c| json!(c));
            self.out.push(patch(self.id, json!({ field: v })));
        }
        ui.end_row();
    }

    fn color(
        &mut self,
        ui: &mut Ui,
        label: &str,
        field: &str,
        value: Option<Color>,
        default_label: &str,
    ) {
        Self::label(ui, label);
        ui.horizontal(|ui| {
            let mut custom = value.is_some();
            if ui.checkbox(&mut custom, "").changed() {
                let v = if custom {
                    json!(Color::rgb(0, 0, 0))
                } else {
                    Value::Null
                };
                self.out.push(patch(self.id, json!({ field: v })));
            }
            match value {
                Some(c) => {
                    let mut c32 = to_color32(c);
                    if ui.color_edit_button_srgba(&mut c32).changed() {
                        self.out
                            .push(patch(self.id, json!({ field: from_color32(c32) })));
                    }
                }
                None => {
                    ui.weak(default_label);
                }
            }
        });
        ui.end_row();
    }

    fn point(&mut self, ui: &mut Ui, label: &str, field: &str, p: Point2) -> Option<Point2> {
        let x = self.length(ui, &format!("{label} X"), &format!("{field}.x"), p.x());
        let y = self.length(ui, &format!("{label} Y"), &format!("{field}.y"), p.y());
        match (x, y) {
            (None, None) => None,
            (x, y) => Some(Point2(x.unwrap_or(p.x()), y.unwrap_or(p.y()))),
        }
    }
}

use housecraft_api::units::Point2;

/// Show one window per object in `app.spec`.
pub fn spec_windows(app: &mut App, ctx: &egui::Context) {
    let mut commands = Vec::new();
    let mut closed = Vec::new();
    let ids = app.spec.clone();
    for id in ids {
        let doc = app.session.document();
        let Some(object) = doc.object(id).cloned() else {
            closed.push(id);
            continue;
        };
        let title = format!("{} Specification", title_case(object.kind().name()));
        let mut open = true;
        let mut ed = Ed {
            doc,
            bufs: &mut app.bufs,
            id,
            out: Vec::new(),
        };
        egui::Window::new(title)
            .id(egui::Id::unique(("spec_window", id)))
            .open(&mut open)
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                egui::Grid::new(("spec_grid", id))
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| editor(ui, &mut ed, &object));
            });
        commands.extend(ed.out);
        if !open {
            closed.push(id);
        }
    }
    app.spec.retain(|id| !closed.contains(id));
    if !commands.is_empty() {
        app.run(commands);
    }
}

pub fn title_case(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn editor(ui: &mut Ui, ed: &mut Ed<'_>, object: &Object) {
    let id = ed.id;
    let doc = ed.doc;
    match object {
        Object::Wall(w) => {
            ed.pick::<WallType>(ui, "Wall Type", "wall_type", Some(w.wall_type), None);
            if let Some(t) = doc.get::<WallType>(w.wall_type) {
                Ed::label(ui, "Thickness");
                ui.label(fmt(doc, t.thickness()));
                ui.end_row();
            }
            if let Some(l) = ed.length(ui, "Length", "length", w.length()) {
                ed.out.push(Command::SetWallLength {
                    id,
                    length: l,
                    anchor: LengthAnchor::Start,
                });
            }
            ed.opt_length(ui, "Height", "height", w.height, doc.wall_height(w));
            if let Some(p) = ed.point(ui, "Start", "start", w.start) {
                ed.out.push(Command::MoveWall {
                    id,
                    start: Some(p),
                    end: None,
                    detach: false,
                });
            }
            if let Some(p) = ed.point(ui, "End", "end", w.end) {
                ed.out.push(Command::MoveWall {
                    id,
                    start: None,
                    end: Some(p),
                    detach: false,
                });
            }
            renovation_row(ui, ed, w.renovation);
            ed.pick::<Layer>(ui, "Layer", "layer", Some(w.layer), None);
            let d = doc.active_defaults();
            Ed::label(ui, "Roof");
            let mut gable = w.roof.gable;
            if ui.checkbox(&mut gable, "Full Gable Wall").changed() {
                ed.out
                    .push(patch(id, json!({ "roof": { "gable": gable } })));
            }
            ui.end_row();
            Ed::label(ui, "Roof Pitch");
            let key = ed.key("roof.pitch");
            let shown = crate::util::fmt_pitch(w.roof.pitch.unwrap_or(d.roof.pitch));
            if let Some(text) = ed.bufs.text(ui, key, &shown, 80.0) {
                if text.trim().is_empty() {
                    ed.out.push(patch(id, json!({ "roof": { "pitch": null } })));
                } else if let Some(deg) = crate::util::parse_pitch(&text) {
                    ed.out.push(patch(id, json!({ "roof": { "pitch": deg } })));
                }
            }
            ui.end_row();
            let default_overhang = if w.roof.gable {
                d.roof.rake_overhang
            } else {
                d.roof.overhang
            };
            if let Some(l) = ed.length(
                ui,
                "Roof Overhang",
                "roof.overhang",
                w.roof.overhang.unwrap_or(default_overhang),
            ) {
                ed.out.push(patch(id, json!({ "roof": { "overhang": l } })));
            }
            Ed::label(ui, "Upper Roof");
            let mut has_upper = w.roof.upper.is_some();
            if ui
                .checkbox(&mut has_upper, "Pitch change above a break")
                .on_hover_text("For mansard, gambrel and Dutch gable roofs")
                .changed()
            {
                let v = if has_upper {
                    json!({ "break_height": Length::from_feet(4.0), "pitch": d.roof.pitch, "gable": false })
                } else {
                    serde_json::Value::Null
                };
                ed.out.push(patch(id, json!({ "roof": { "upper": v } })));
            }
            ui.end_row();
            if let Some(u) = w.roof.upper {
                if let Some(l) = ed.length(ui, "Break Height", "roof.upper.break", u.break_height) {
                    ed.out.push(patch(
                        id,
                        json!({ "roof": { "upper": { "break_height": l } } }),
                    ));
                }
                Ed::label(ui, "Upper Pitch");
                let key = ed.key("roof.upper.pitch");
                if let Some(text) = ed
                    .bufs
                    .text(ui, key, &crate::util::fmt_pitch(u.pitch), 80.0)
                    && let Some(deg) = crate::util::parse_pitch(&text)
                {
                    ed.out
                        .push(patch(id, json!({ "roof": { "upper": { "pitch": deg } } })));
                }
                ui.end_row();
                Ed::label(ui, "");
                let mut g = u.gable;
                if ui.checkbox(&mut g, "Gable above the break").changed() {
                    ed.out
                        .push(patch(id, json!({ "roof": { "upper": { "gable": g } } })));
                }
                ui.end_row();
            }
        }
        Object::SectionView(v) => {
            ed.text(ui, "Name", "name", &v.name);
            Ed::label(ui, "Type");
            let mut k = v.section_kind;
            ui.horizontal(|ui| {
                ui.radio_value(&mut k, SectionKind::Elevation, "Elevation");
                ui.radio_value(&mut k, SectionKind::Section, "Cross section");
            });
            if k != v.section_kind {
                ed.out.push(patch(id, json!({ "section_kind": k })));
            }
            ui.end_row();
            if let Some(p) = ed.point(ui, "Line start", "a", v.a) {
                ed.out.push(patch(id, json!({ "a": p })));
            }
            if let Some(p) = ed.point(ui, "Line end", "b", v.b) {
                ed.out.push(patch(id, json!({ "b": p })));
            }
            ed.opt_length(ui, "Depth", "depth", v.depth, Some(Length::from_feet(20.0)));
            Ed::label(ui, "Clip");
            let mut c = v.clip_sides;
            if ui
                .checkbox(&mut c, "Only between the line's ends")
                .changed()
            {
                ed.out.push(patch(id, json!({ "clip_sides": c })));
            }
            ui.end_row();
            ed.pick::<Layer>(ui, "Callout Layer", "layer", Some(v.layer), None);
        }
        Object::Layout(l) => {
            ed.text(ui, "Name", "name", &l.name);
            if let Some(v) = ed.length(ui, "Sheet Width", "paper_width", l.paper_width) {
                ed.out.push(patch(id, json!({ "paper_width": v })));
            }
            if let Some(v) = ed.length(ui, "Sheet Height", "paper_height", l.paper_height) {
                ed.out.push(patch(id, json!({ "paper_height": v })));
            }
            Ed::label(ui, "Title Block");
            let mut t = l.title_block;
            if ui.checkbox(&mut t, "Border and title strip").changed() {
                ed.out.push(patch(id, json!({ "title_block": t })));
            }
            ui.end_row();
            Ed::label(ui, "Pages");
            ui.vertical(|ui| {
                for (i, name) in l.pages.iter().enumerate() {
                    let key = ed.key(&format!("page_{i}"));
                    if let Some(t) = ed.bufs.text(ui, key, name, 140.0) {
                        let mut pages = l.pages.clone();
                        pages[i] = t;
                        ed.out.push(patch(id, json!({ "pages": pages })));
                    }
                }
                if ui.button("Add Page").clicked() {
                    ed.out.push(Command::AddLayoutPage {
                        layout: id,
                        name: None,
                    });
                }
            });
            ui.end_row();
        }
        Object::LayoutBox(b) => {
            Ed::label(ui, "Shows");
            ui.label(housecraft_api::layouts::source_name(doc, &b.source));
            ui.end_row();
            for (label, field, value) in [
                ("Left (paper)", "x", b.x),
                ("Bottom (paper)", "y", b.y),
                ("Width (paper)", "width", b.width),
                ("Height (paper)", "height", b.height),
            ] {
                if let Some(v) = ed.length(ui, label, field, value) {
                    ed.out.push(patch(id, json!({ field: v })));
                }
            }
            if !matches!(
                b.source,
                LayoutSource::Schedule { .. }
                    | LayoutSource::MaterialsList
                    | LayoutSource::Text { .. }
            ) {
                Ed::label(ui, "Scale");
                let imperial =
                    crate::util::input_unit(doc) == housecraft_api::units::LengthUnit::Inches;
                let scales: &[f64] = if imperial {
                    &[12.0, 16.0, 24.0, 32.0, 48.0, 64.0, 96.0, 128.0, 192.0]
                } else {
                    &[10.0, 20.0, 25.0, 50.0, 75.0, 100.0, 200.0, 250.0, 500.0]
                };
                let label = |s: f64| {
                    if s <= 0.0 {
                        "Fit (standard scale)".to_string()
                    } else {
                        housecraft_draft::pdf::scale_label(s, imperial)
                    }
                };
                let mut chosen = b.scale;
                egui::ComboBox::from_id_salt(ed.key("scale"))
                    .selected_text(label(b.scale))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut chosen, 0.0, label(0.0));
                        for s in scales {
                            ui.selectable_value(&mut chosen, *s, label(*s));
                        }
                    });
                if chosen != b.scale {
                    ed.out.push(patch(id, json!({ "scale": chosen })));
                }
                ui.end_row();
            }
            if let LayoutSource::Text { text } = &b.source {
                Ed::label(ui, "Text");
                let key = ed.key("notes");
                if let Some(t) = ed.bufs.multiline(ui, key, text, 240.0) {
                    ed.out.push(patch(id, json!({ "source": { "text": t } })));
                }
                ui.end_row();
                Ed::label(ui, "");
                ui.weak(format!(
                    "Macros: {}",
                    housecraft_draft::macros::MACROS
                        .iter()
                        .map(|(m, _)| format!("{{{m}}}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
                ui.end_row();
            }
            let caption = b.label.clone().unwrap_or_default();
            ed.text(ui, "Caption", "label", &caption);
        }
        Object::Callout(c) => {
            Ed::label(ui, "Text");
            let key = ed.key("callout_text");
            if let Some(t) = ed.bufs.multiline(ui, key, &c.text, 220.0) {
                ed.out.push(patch(id, json!({ "text": t })));
            }
            ui.end_row();
            Ed::label(ui, "Marker");
            let names = ["None", "Circle", "Hexagon", "Square", "Diamond"];
            let shapes = [
                None,
                Some(MarkerShape::Circle),
                Some(MarkerShape::Hexagon),
                Some(MarkerShape::Square),
                Some(MarkerShape::Diamond),
            ];
            let current = shapes.iter().position(|m| *m == c.marker).unwrap_or(0);
            let mut chosen = current;
            egui::ComboBox::from_id_salt(ed.key("marker"))
                .selected_text(names[current])
                .show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut chosen, i, *n);
                    }
                });
            if chosen != current {
                ed.out.push(patch(id, json!({ "marker": shapes[chosen] })));
            }
            ui.end_row();
            Ed::label(ui, "Leader");
            let mut has = c.target.is_some();
            if ui.checkbox(&mut has, "Arrow to a point").changed() {
                let v = if has {
                    json!(Point2(
                        c.at.x() - Length::from_feet(3.0),
                        c.at.y() - Length::from_feet(2.0)
                    ))
                } else {
                    Value::Null
                };
                ed.out.push(patch(id, json!({ "target": v })));
            }
            ui.end_row();
            ed.pick::<Layer>(ui, "Layer", "layer", Some(c.layer), None);
        }
        Object::Cabinet(c) => cabinet_editor(ui, ed, c),
        Object::Picture(p) => {
            Ed::label(ui, "Image File");
            ui.horizontal(|ui| {
                let name = std::path::Path::new(&p.path)
                    .file_name()
                    .map_or(p.path.clone(), |f| f.to_string_lossy().into_owned());
                ui.label(name).on_hover_text(&p.path);
                if ui.small_button("Change…").clicked()
                    && let Some(f) = pick_image()
                {
                    ed.out.push(patch(id, json!({ "path": f })));
                }
            });
            ui.end_row();
            if let Some(w) = ed.length(ui, "Width", "width", p.width)
                && w.mm() > 0.0
            {
                // Keep the proportions when the width changes.
                let h = p.height * (w.mm() / p.width.mm());
                ed.out.push(patch(id, json!({ "width": w, "height": h })));
            }
            if let Some(h) = ed.length(ui, "Height", "height", p.height)
                && h.mm() > 0.0
            {
                ed.out.push(patch(id, json!({ "height": h })));
            }
            if let Some(n) = ed.number(ui, "Rotation (°)", "rotation", p.rotation) {
                ed.out.push(patch(id, json!({ "rotation": n })));
            }
            Ed::label(ui, "Opacity");
            let mut o = p.opacity * 100.0;
            if ui.add(egui::Slider::new(&mut o, 0.0..=100.0).suffix("%")).drag_stopped() {
                ed.out.push(patch(id, json!({ "opacity": o / 100.0 })));
            }
            ui.end_row();
            ed.color(ui, "Transparent Color", "transparent", p.transparent, "None");
            ed.pick::<Layer>(ui, "Layer", "layer", Some(p.layer), None);
        }
        Object::SymbolDef(s) => {
            ed.text(ui, "Name", "name", &s.name);
            Ed::label(ui, "Model");
            let [w, d, h] = s.size();
            ui.label(format!(
                "{} triangles, {} × {} × {}",
                s.triangles.len(),
                fmt(doc, Length::from_mm(w)),
                fmt(doc, Length::from_mm(d)),
                fmt(doc, Length::from_mm(h))
            ));
            ui.end_row();
        }
        Object::Symbol(s) => {
            ed.pick::<SymbolDef>(ui, "Model", "def", Some(s.def), None);
            for (label, field, v) in [
                ("Width", "width", s.width),
                ("Depth", "depth", s.depth),
                ("Height", "height", s.height),
            ] {
                if let Some(l) = ed.length(ui, label, field, v)
                    && l.mm() > 0.0
                {
                    ed.out.push(patch(id, json!({ field: l })));
                }
            }
            if let Some(def) = doc.get::<SymbolDef>(s.def) {
                Ed::label(ui, "");
                if ui.button("Reset to the model's size").clicked() {
                    let [w, d, h] = def.size();
                    ed.out.push(patch(
                        id,
                        json!({
                            "width": Length::from_mm(w),
                            "depth": Length::from_mm(d),
                            "height": Length::from_mm(h),
                        }),
                    ));
                }
                ui.end_row();
            }
            if let Some(l) = ed.length(ui, "Off the Floor", "elevation", s.elevation) {
                ed.out.push(patch(id, json!({ "elevation": l })));
            }
            if let Some(n) = ed.number(ui, "Facing (°)", "facing", s.facing) {
                ed.out.push(patch(id, json!({ "facing": n })));
            }
            ed.pick::<Layer>(ui, "Layer", "layer", Some(s.layer), None);
        }
        Object::Countertop(t) => {
            if let Some(h) = ed.length(ui, "Top Height", "height", t.height) {
                ed.out.push(patch(id, json!({ "height": h })));
            }
            if let Some(v) = ed.length(ui, "Thickness", "thickness", t.thickness)
                && v.mm() > 0.0
            {
                ed.out.push(patch(id, json!({ "thickness": v })));
            }
            let edges: Vec<(EdgeProfile, &str)> = EdgeProfile::ALL.iter().map(|e| (*e, e.name())).collect();
            ed.choice(ui, "Edge Profile", "edge", t.edge, &edges);
            Ed::label(ui, "Holes");
            ui.vertical(|ui| {
                let mut holes = t.holes.clone();
                let mut changed = false;
                let mut remove = None;
                for (i, h) in holes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} {}",
                            match h.hole_kind {
                                HoleKind::Sink => "Sink",
                                HoleKind::Cooktop => "Cooktop",
                                HoleKind::Round => "Round",
                            },
                            fmt(doc, h.width)
                        ));
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    holes.remove(i);
                    changed = true;
                }
                // New holes go at the outline's centre.
                let pts: Vec<_> = t.points.iter().map(|p| housecraft_api::geom::v2(*p)).collect();
                let c = housecraft_api::geom::pt(housecraft_api::geom::math::centroid(&pts));
                let along = {
                    let (a, b) = (pts[0], pts[1 % pts.len()]);
                    (b - a).y.atan2((b - a).x).to_degrees()
                };
                ui.horizontal(|ui| {
                    let i = Length::from_inches;
                    for (label, kind, w, d) in [
                        ("+ Sink", HoleKind::Sink, 30.0, 18.0),
                        ("+ Cooktop", HoleKind::Cooktop, 29.0, 19.0),
                        ("+ Round", HoleKind::Round, 1.5, 1.5),
                    ] {
                        if ui.small_button(label).clicked() {
                            holes.push(CounterHole {
                                hole_kind: kind,
                                center: c,
                                width: i(w),
                                depth: i(d),
                                rotation: along,
                            });
                            changed = true;
                        }
                    }
                });
                if changed {
                    ed.out.push(patch(id, json!({ "holes": holes })));
                }
            });
            ui.end_row();
            ed.pick::<Layer>(ui, "Layer", "layer", Some(t.layer), None);
        }
        Object::Appliance(a) => {
            let kinds: Vec<(ApplianceKind, &str)> =
                ApplianceKind::ALL.iter().map(|k| (*k, k.name())).collect();
            ed.choice(ui, "Type", "appliance_kind", a.appliance_kind, &kinds);
            for (label, field, v) in [
                ("Width", "width", a.width),
                ("Depth", "depth", a.depth),
                ("Height", "height", a.height),
            ] {
                if let Some(l) = ed.length(ui, label, field, v)
                    && l.mm() > 0.0
                {
                    ed.out.push(patch(id, json!({ field: l })));
                }
            }
            if let Some(l) = ed.length(ui, "Off the Floor", "elevation", a.elevation) {
                ed.out.push(patch(id, json!({ "elevation": l })));
            }
            if let Some(n) = ed.number(ui, "Facing (°)", "facing", a.facing) {
                ed.out.push(patch(id, json!({ "facing": n })));
            }
            ed.pick::<Layer>(ui, "Layer", "layer", Some(a.layer), None);
        }
        Object::Material(m) => {
            ed.text(ui, "Name", "name", &m.name);
            ed.text(ui, "Category", "category", &m.category);
            Ed::label(ui, "Type");
            let mut class = m.class;
            egui::ComboBox::from_id_salt(ed.key("class"))
                .selected_text(class.name())
                .show_ui(ui, |ui| {
                    for c in MaterialClass::ALL {
                        ui.selectable_value(&mut class, c, c.name());
                    }
                });
            if class != m.class {
                // Switching type takes that type's typical finish.
                let fresh = Material::new(&m.name, &m.category, class, m.color);
                ed.out.push(patch(
                    id,
                    json!({
                        "class": class,
                        "roughness": fresh.roughness,
                        "metallic": fresh.metallic,
                        "opacity": fresh.opacity,
                        "emissive": fresh.emissive,
                    }),
                ));
            }
            ui.end_row();
            Ed::label(ui, "Color");
            let mut c32 = to_color32(m.color);
            if ui.color_edit_button_srgba(&mut c32).changed() {
                ed.out.push(patch(id, json!({ "color": from_color32(c32) })));
            }
            ui.end_row();
            for (label, field, value) in [
                ("Roughness (0 shiny – 1 matte)", "roughness", m.roughness),
                ("Metallic (0–1)", "metallic", m.metallic),
                ("Opacity (0 clear – 1 solid)", "opacity", m.opacity),
                ("Glow (emissive)", "emissive", m.emissive),
                ("Anisotropy (-1–1)", "anisotropy", m.anisotropy),
            ] {
                if let Some(v) = ed.number(ui, label, field, value) {
                    let v = if field == "emissive" {
                        v.max(0.0)
                    } else if field == "anisotropy" {
                        v.clamp(-1.0, 1.0)
                    } else {
                        v.clamp(0.0, 1.0)
                    };
                    ed.out.push(patch(id, json!({ field: v })));
                }
            }
            for (label, key) in [
                ("Texture Image", "color"),
                ("Normal Map", "normal"),
                ("Bump Map", "bump"),
                ("Roughness Map", "roughness"),
                ("Metallic Map", "metallic"),
                ("Ambient Occlusion", "ao"),
                ("Emissive Map", "emissive"),
                ("Opacity Map", "opacity"),
            ] {
                let maps = &m.maps;
                let current = match key {
                    "color" => &maps.color,
                    "normal" => &maps.normal,
                    "bump" => &maps.bump,
                    "roughness" => &maps.roughness,
                    "metallic" => &maps.metallic,
                    "ao" => &maps.ao,
                    "emissive" => &maps.emissive,
                    _ => &maps.opacity,
                };
                Ed::label(ui, label);
                ui.horizontal(|ui| {
                    let path = current.as_ref().map_or("", |t| t.path.as_str());
                    let shown = std::path::Path::new(path)
                        .file_name()
                        .map_or("(none)".to_string(), |f| f.to_string_lossy().into_owned());
                    ui.label(shown).on_hover_text(path);
                    if ui.small_button("Browse…").clicked()
                        && let Some(file) = rfd::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"])
                            .pick_file()
                    {
                        let map = json!({ "path": file.to_string_lossy(), "strength": 1.0 });
                        ed.out.push(patch(id, json!({ "maps": { key: map } })));
                    }
                    if current.is_some() && ui.small_button("Clear").clicked() {
                        ed.out.push(patch(id, json!({ "maps": { key: Value::Null } })));
                    }
                });
                ui.end_row();
            }
            if m.maps.roughness.is_some() {
                Ed::label(ui, "Roughness Map");
                let mut g = m.gloss_map;
                if ui.checkbox(&mut g, "Map holds gloss (invert)").changed() {
                    ed.out.push(patch(id, json!({ "gloss_map": g })));
                }
                ui.end_row();
            }
            ed.opt_length(ui, "Texture Width", "texture_width", m.texture_width, Some(Length::from_feet(4.0)));
            ed.opt_length(ui, "Texture Height", "texture_height", m.texture_height, Some(Length::from_feet(4.0)));
        }
        Object::MaterialAssignment(a) => {
            ed.pick::<Material>(ui, "Material", "material", Some(a.material), None);
            Ed::label(ui, "Surface");
            ui.label(a.surface.map_or("All surfaces", |s| s.name()));
            ui.end_row();
        }
        Object::CadDetail(d) => {
            ed.text(ui, "Name", "name", &d.name);
            Ed::label(ui, "Contents");
            let n = doc.objects.keys().filter(|o| doc.floor_of(**o) == Some(id)).count();
            ui.label(format!("{n} object{}", if n == 1 { "" } else { "s" }));
            ui.end_row();
        }
        Object::Block(b) => {
            ed.text(ui, "Name", "name", &b.name);
            Ed::label(ui, "Placed");
            let n = doc.iter::<BlockRef>().filter(|r| r.block == id).count();
            ui.label(format!("{n} time{}", if n == 1 { "" } else { "s" }));
            ui.end_row();
        }
        Object::BlockRef(r) => {
            ed.pick::<Block>(ui, "Block", "block", Some(r.block), None);
            if let Some(b) = doc.get::<Block>(r.block) {
                let block_id = b.id;
                ed.text_of(ui, block_id, "Block Name", "name", &b.name);
            }
            if let Some(p) = ed.point(ui, "Insertion Point", "at", r.at) {
                ed.out.push(patch(id, json!({ "at": p })));
            }
            if let Some(n) = ed.number(ui, "Rotation (°)", "rotation", r.rotation) {
                ed.out.push(patch(id, json!({ "rotation": n })));
            }
            if let Some(k) = ed.number(ui, "Scale", "scale", r.scale)
                && k > 0.0
            {
                ed.out.push(patch(id, json!({ "scale": k })));
            }
            Ed::label(ui, "Mirrored");
            let mut m = r.mirrored;
            if ui.checkbox(&mut m, "Flip left to right").changed() {
                ed.out.push(patch(id, json!({ "mirrored": m })));
            }
            ui.end_row();
            ed.pick::<Layer>(ui, "Layer", "layer", Some(r.layer), None);
        }
        Object::RevisionCloud(rc) => {
            let label = rc.label.clone().unwrap_or_default();
            ed.text(ui, "Revision", "label", &label);
            if let Some(a) = ed.length(ui, "Scallop Size", "arc", rc.arc)
                && a.mm() > 0.0
            {
                ed.out.push(patch(id, json!({ "arc": a })));
            }
            ed.pick::<Layer>(ui, "Layer", "layer", Some(rc.layer), None);
        }
        Object::Schedule(s) => {
            Ed::label(ui, "Schedule");
            ui.label(s.schedule_kind.name());
            ui.end_row();
            if let Some(p) = ed.point(ui, "Position", "position", s.position) {
                ed.out.push(patch(id, json!({ "position": p })));
            }
            Ed::label(ui, "Scope");
            let mut all = s.all_floors;
            if ui.checkbox(&mut all, "All floors").changed() {
                ed.out.push(patch(id, json!({ "all_floors": all })));
            }
            ui.end_row();
            Ed::label(ui, "Columns");
            ui.vertical(|ui| {
                let shown: Vec<String> = if s.columns.is_empty() {
                    housecraft_api::schedule::available_columns(s.schedule_kind)
                        .iter()
                        .map(|c| c.to_string())
                        .collect()
                } else {
                    s.columns.clone()
                };
                for c in housecraft_api::schedule::available_columns(s.schedule_kind) {
                    let mut on = shown.iter().any(|x| x.eq_ignore_ascii_case(c));
                    if ui.checkbox(&mut on, *c).changed() {
                        // Keep the catalogue's order.
                        let cols: Vec<&str> =
                            housecraft_api::schedule::available_columns(s.schedule_kind)
                                .iter()
                                .copied()
                                .filter(|k| {
                                    if k == c {
                                        on
                                    } else {
                                        shown.iter().any(|x| x.eq_ignore_ascii_case(k))
                                    }
                                })
                                .collect();
                        ed.out.push(patch(id, json!({ "columns": cols })));
                    }
                }
            });
            ui.end_row();
            ed.pick::<Layer>(ui, "Layer", "layer", Some(s.layer), None);
        }
        Object::Electrical(e) => {
            Ed::label(ui, "Type");
            let mut kind = e.electrical_kind;
            egui::ComboBox::from_id_salt(ed.key("electrical_kind"))
                .selected_text(kind.name())
                .show_ui(ui, |ui| {
                    for k in ElectricalKind::ALL {
                        // Switches and fixtures don't swap (controls would dangle).
                        if k.is_switch() == e.electrical_kind.is_switch() {
                            ui.selectable_value(&mut kind, k, k.name());
                        }
                    }
                });
            if kind != e.electrical_kind {
                // Light settings only belong on light fixtures.
                let light = if kind.emits_light() { json!(e.light) } else { Value::Null };
                ed.out.push(patch(id, json!({ "electrical_kind": kind, "light": light })));
            }
            ui.end_row();
            if let Some(p) = ed.point(ui, "Position", "position", e.position) {
                ed.out.push(patch(id, json!({ "position": p })));
            }
            if let Some(n) = ed.number(ui, "Facing (deg)", "rotation", e.rotation) {
                ed.out.push(patch(id, json!({ "rotation": n })));
            }
            match e.electrical_kind.default_height() {
                Some(h) => ed.opt_length(ui, "Height", "height", e.height, Some(h)),
                None => {
                    Ed::label(ui, "Height");
                    ui.weak("At the ceiling");
                    ui.end_row();
                }
            }
            if e.electrical_kind.emits_light() {
                let mut l = e.light.unwrap_or_else(|| LightSettings::typical(e.electrical_kind));
                let before = l;
                Ed::label(ui, "Light");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut l.on, "On");
                    ui.checkbox(&mut l.shadows, "Casts shadows");
                });
                ui.end_row();
                Ed::label(ui, "Brightness");
                ui.add(egui::DragValue::new(&mut l.lumens).range(0.0..=20000.0).speed(10.0).suffix(" lm"));
                ui.end_row();
                Ed::label(ui, "Dimmer");
                let mut pct = l.dimmer * 100.0;
                if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%")).changed() {
                    l.dimmer = pct / 100.0;
                }
                ui.end_row();
                Ed::label(ui, "Color Temperature");
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut l.kelvin, 1800.0..=6500.0).suffix(" K"));
                    let [r, g, b] = l.rgb();
                    let c = |v: f64| (v * 255.0) as u8;
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(c(r), c(g), c(b)));
                });
                ui.end_row();
                if l != before {
                    ed.out.push(patch(id, json!({ "light": l })));
                }
            }
            if e.electrical_kind.is_switch() {
                Ed::label(ui, "Switch type");
                ui.label(doc.switch_labels().remove(&id).unwrap_or_default());
                ui.end_row();
                Ed::label(ui, "Controls");
                ui.vertical(|ui| {
                    if e.controls.is_empty() {
                        ui.weak("Nothing yet. Use Connect Electrical to wire fixtures.");
                    }
                    for c in &e.controls {
                        ui.horizontal(|ui| {
                            let name = doc
                                .get::<Electrical>(*c)
                                .map_or("?", |x| x.electrical_kind.name());
                            ui.label(format!("{name} #{}", c.0));
                            if ui.small_button("Unwire").clicked() {
                                ed.out.push(Command::ConnectSwitch {
                                    switch: id,
                                    fixtures: vec![*c],
                                    disconnect: true,
                                });
                            }
                        });
                    }
                });
                ui.end_row();
            }
            renovation_row(ui, ed, e.renovation);
            ed.pick::<Layer>(ui, "Layer", "layer", Some(e.layer), None);
        }
        Object::Stair(s) => {
            let g = housecraft_api::geom::stairs::geometry(doc, s);
            Ed::label(ui, "Shape");
            let names = ["Straight", "L-shaped", "U-shaped"];
            let current = match s.shape {
                StairShape::Straight => 0,
                StairShape::L { .. } => 1,
                StairShape::U { .. } => 2,
            };
            let mut chosen = current;
            egui::ComboBox::from_id_salt(ed.key("shape"))
                .selected_text(names[current])
                .show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut chosen, i, *n);
                    }
                });
            ui.end_row();
            let half = (g.risers.saturating_sub(2) / 2).max(1);
            if chosen != current {
                let v = match chosen {
                    0 => json!({"type": "straight"}),
                    1 => json!({"type": "l", "turn": "right", "first_treads": half}),
                    _ => {
                        json!({"type": "u", "turn": "right", "first_treads": half, "gap": Length::ZERO})
                    }
                };
                ed.out.push(patch(id, json!({ "shape": v })));
            }
            if let StairShape::L { turn, first_treads }
            | StairShape::U {
                turn, first_treads, ..
            } = s.shape
            {
                Ed::label(ui, "Turn");
                let mut t = turn;
                ui.horizontal(|ui| {
                    ui.radio_value(&mut t, Turn::Left, "Left");
                    ui.radio_value(&mut t, Turn::Right, "Right");
                });
                ui.end_row();
                if t != turn {
                    ed.out.push(patch(id, json!({ "shape": { "turn": t } })));
                }
                if let Some(n) = ed.number(
                    ui,
                    "Treads before landing",
                    "first_treads",
                    first_treads as f64,
                ) && n >= 1.0
                {
                    ed.out.push(patch(
                        id,
                        json!({ "shape": { "first_treads": n.round() as u32 } }),
                    ));
                }
                if let StairShape::U { gap, .. } = s.shape
                    && let Some(l) = ed.length(ui, "Gap between flights", "gap", gap)
                {
                    ed.out.push(patch(id, json!({ "shape": { "gap": l } })));
                }
            }
            if let Some(l) = ed.length(ui, "Width", "width", s.width) {
                ed.out.push(patch(id, json!({ "width": l })));
            }
            if let Some(n) = ed.number(ui, "Direction (deg)", "direction", s.direction) {
                ed.out.push(patch(id, json!({ "direction": n })));
            }
            let auto_rise = doc.stair_rise(&Stair {
                rise: None,
                ..s.clone()
            });
            ed.opt_length(ui, "Total rise", "rise", s.rise, Some(auto_rise));
            if let Some(l) = ed.length(ui, "Maximum riser", "max_riser", s.max_riser) {
                ed.out.push(patch(id, json!({ "max_riser": l })));
            }
            if let Some(l) = ed.length(ui, "Tread depth", "tread_depth", s.tread_depth) {
                ed.out.push(patch(id, json!({ "tread_depth": l })));
            }
            if let Some(l) = ed.length(ui, "Nosing overhang", "tread_overhang", s.tread_overhang)
                && l.mm() >= 0.0
            {
                ed.out.push(patch(id, json!({ "tread_overhang": l })));
            }
            Ed::label(ui, "Stringers");
            let mut st = s.stringers;
            if ui.checkbox(&mut st, "Side stringers").changed() {
                ed.out.push(patch(id, json!({ "stringers": st })));
            }
            ui.end_row();
            Ed::label(ui, "Risers");
            ui.label(format!(
                "{} at {}",
                g.risers,
                fmt(doc, Length::from_mm(g.riser_height))
            ));
            ui.end_row();
            Ed::label(ui, "Railings");
            ui.horizontal(|ui| {
                let (mut l, mut r) = (s.railing_left, s.railing_right);
                if ui.checkbox(&mut l, "Left").changed() {
                    ed.out.push(patch(id, json!({ "railing_left": l })));
                }
                if ui.checkbox(&mut r, "Right").changed() {
                    ed.out.push(patch(id, json!({ "railing_right": r })));
                }
            });
            ui.end_row();
            if let Some(l) = ed.length(ui, "Railing height", "railing_height", s.railing_height) {
                ed.out.push(patch(id, json!({ "railing_height": l })));
            }
            Ed::label(ui, "Stairwell");
            let mut w = s.stairwell;
            if ui
                .checkbox(&mut w, "Cut an opening in the floor above")
                .changed()
            {
                ed.out.push(patch(id, json!({ "stairwell": w })));
            }
            ui.end_row();
            ed.pick::<Layer>(ui, "Layer", "layer", Some(s.layer), None);
        }
        Object::RoofPlane(r) => {
            Ed::label(ui, "Type");
            ui.label(if r.gable {
                "Gable infill"
            } else {
                "Roof plane"
            });
            ui.end_row();
            if !r.gable {
                Ed::label(ui, "Pitch");
                ui.label(format!(
                    "{} ({:.1}°)",
                    crate::util::fmt_pitch(r.pitch),
                    r.pitch
                ));
                ui.end_row();
                if let Some(l) = ed.length(ui, "Thickness", "thickness", r.thickness) {
                    ed.out.push(patch(id, json!({ "thickness": l })));
                }
            }
            if r.auto {
                Ed::label(ui, "Automatic");
                ui.vertical(|ui| {
                    ui.weak("Rebuilt from the walls whenever they change. Edit the walls' roof settings, or stop rebuilding to edit planes by hand.");
                    if ui.button("Stop automatic rebuilding").clicked() {
                        ed.out.push(patch(r.floor, json!({ "auto_roof": false })));
                    }
                });
                ui.end_row();
            }
            ed.pick::<Layer>(ui, "Layer", "layer", Some(r.layer), None);
        }
        Object::Opening(o) => {
            Ed::label(ui, "Type");
            let mut kind = o.opening_kind;
            egui::ComboBox::from_id_salt(ed.key("opening_kind"))
                .selected_text(opening_name(kind))
                .show_ui(ui, |ui| {
                    let all: Vec<OpeningKind> = if kind.is_door() {
                        DoorStyle::ALL.map(OpeningKind::Door).to_vec()
                    } else {
                        WindowStyle::ALL.map(OpeningKind::Window).to_vec()
                    };
                    for k in all {
                        ui.selectable_value(&mut kind, k, opening_name(k));
                    }
                });
            if kind != o.opening_kind {
                ed.out.push(patch(id, json!({ "opening_kind": kind })));
            }
            ui.end_row();
            for (label, field, value) in
                [("Width", "width", o.width), ("Height", "height", o.height)]
            {
                if let Some(l) = ed.length(ui, label, field, value) {
                    ed.out.push(patch(id, json!({ field: l })));
                }
            }
            if !o.opening_kind.is_door()
                && let Some(l) = ed.length(ui, "Floor to Sill", "sill", o.sill)
            {
                ed.out.push(patch(id, json!({ "sill": l })));
            }
            if let Some(l) = ed.length(ui, "Distance from Wall Start", "position", o.position) {
                ed.out.push(patch(id, json!({ "position": l })));
            }
            if o.opening_kind.is_door() {
                Ed::label(ui, "Hinge");
                let mut hinge = o.hinge;
                ui.horizontal(|ui| {
                    ui.radio_value(&mut hinge, Hinge::Left, "Left");
                    ui.radio_value(&mut hinge, Hinge::Right, "Right");
                });
                if hinge != o.hinge {
                    ed.out.push(patch(id, json!({ "hinge": hinge })));
                }
                ui.end_row();
                Ed::label(ui, "Swing");
                let mut swing_in = o.swing_in;
                ui.horizontal(|ui| {
                    ui.radio_value(&mut swing_in, true, "In");
                    ui.radio_value(&mut swing_in, false, "Out");
                });
                if swing_in != o.swing_in {
                    ed.out.push(patch(id, json!({ "swing_in": swing_in })));
                }
                ui.end_row();
                if matches!(
                    o.opening_kind,
                    OpeningKind::Door(
                        DoorStyle::Hinged
                            | DoorStyle::Double
                            | DoorStyle::Atrium
                            | DoorStyle::Shower
                    )
                ) && let Some(a) = ed.number(ui, "Plan swing (deg)", "plan_swing", o.plan_swing)
                    && (0.0..=180.0).contains(&a)
                {
                    ed.out.push(patch(id, json!({ "plan_swing": a })));
                }
                if o.opening_kind != OpeningKind::Door(DoorStyle::Doorway) {
                    Ed::label(ui, "Open in 3D");
                    let mut pct = o.open_percent;
                    let r = ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%"));
                    // Commit once per drag (or per click/keyboard change), not every frame.
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        ed.out
                            .push(patch(id, json!({ "open_percent": pct.round() })));
                    }
                    ui.end_row();
                }
            }
            renovation_row(ui, ed, o.renovation);
            ed.pick::<Layer>(ui, "Layer", "layer", Some(o.layer), None);
        }
        Object::Room(r) => {
            ed.text(ui, "Room Name", "name", &doc.room_name(r));
            ed.pick::<RoomType>(ui, "Room Type", "room_type", Some(r.room_type), None);
            let area = housecraft_api::query::room_areas(doc, r.floor)
                .into_iter()
                .find(|(i, _)| *i == id)
                .map_or(0.0, |(_, a)| a);
            Ed::label(ui, "Interior Area");
            ui.label(housecraft_api::units::format_area(
                area,
                doc.settings.display_unit,
            ));
            ui.end_row();
            let type_height = doc
                .get::<RoomType>(r.room_type)
                .and_then(|t| t.ceiling_height)
                .or_else(|| doc.get::<Floor>(r.floor).map(|f| f.ceiling_height));
            ed.opt_length(
                ui,
                "Ceiling Height",
                "ceiling_height",
                r.ceiling_height,
                type_height,
            );
            let finish = r.floor_finish.clone().unwrap_or_else(|| {
                doc.get::<RoomType>(r.room_type)
                    .map_or_else(String::new, |t| t.floor_finish.clone())
            });
            ed.text(ui, "Floor Finish", "floor_finish", &finish);
            if let Some(c) = ceiling_editor(ui, ed, r.ceiling) {
                ed.out.push(patch(id, json!({ "ceiling": c })));
            }
            ed.pick::<Layer>(ui, "Label Layer", "layer", Some(r.layer), None);
        }
        Object::Floor(f) => {
            ed.text(ui, "Name", "name", &f.name);
            for (label, field, value) in [
                ("Floor Elevation", "elevation", f.elevation),
                ("Ceiling Height", "ceiling_height", f.ceiling_height),
            ] {
                if let Some(l) = ed.length(ui, label, field, value) {
                    ed.out.push(patch(id, json!({ field: l })));
                }
            }
            platform_editor(ui, ed, f);
            if let Some(fd) = &f.foundation {
                Ed::label(ui, "Foundation");
                ui.label(format!("{:?}", fd.kind));
                ui.end_row();
                for (label, field, value) in [
                    ("Foundation Wall Height", "wall_height", fd.wall_height),
                    (
                        "Foundation Wall Thickness",
                        "wall_thickness",
                        fd.wall_thickness,
                    ),
                    ("Footing Width", "footing_width", fd.footing_width),
                    ("Footing Depth", "footing_depth", fd.footing_depth),
                    ("Slab Thickness", "slab_thickness", fd.slab_thickness),
                ] {
                    if let Some(l) = ed.length(ui, label, &format!("foundation.{field}"), value) {
                        ed.out
                            .push(patch(id, json!({ "foundation": { field: l } })));
                    }
                }
                Ed::label(ui, "Auto Rebuild");
                let mut auto = fd.auto_rebuild;
                if ui.checkbox(&mut auto, "Follow the floor above").changed() {
                    ed.out
                        .push(patch(id, json!({ "foundation": { "auto_rebuild": auto } })));
                }
                ui.end_row();
            }
        }
        Object::Cad(c) => {
            match &c.shape {
                Shape::Line {
                    a,
                    b,
                    arrow_start,
                    arrow_end,
                } => {
                    let mut shape = c.shape.clone();
                    let mut changed = false;
                    if let Some(p) = ed.point(ui, "Start", "a", *a) {
                        if let Shape::Line { a, .. } = &mut shape {
                            *a = p;
                        }
                        changed = true;
                    }
                    if let Some(p) = ed.point(ui, "End", "b", *b) {
                        if let Shape::Line { b, .. } = &mut shape {
                            *b = p;
                        }
                        changed = true;
                    }
                    Ed::label(ui, "Arrows");
                    let (mut s, mut e) = (*arrow_start, *arrow_end);
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut s, "Start");
                        ui.checkbox(&mut e, "End");
                    });
                    ui.end_row();
                    if s != *arrow_start || e != *arrow_end {
                        if let Shape::Line {
                            arrow_start,
                            arrow_end,
                            ..
                        } = &mut shape
                        {
                            *arrow_start = s;
                            *arrow_end = e;
                        }
                        changed = true;
                    }
                    if changed {
                        ed.out.push(patch(id, json!({ "shape": shape })));
                    }
                }
                Shape::Circle { center, radius } => {
                    let mut shape = c.shape.clone();
                    let p = ed.point(ui, "Center", "center", *center);
                    let r = ed.length(ui, "Radius", "radius", *radius);
                    if let Shape::Circle { center, radius } = &mut shape {
                        if let Some(p) = p {
                            *center = p;
                        }
                        if let Some(r) = r {
                            *radius = r;
                        }
                    }
                    if p.is_some() || r.is_some() {
                        ed.out.push(patch(id, json!({ "shape": shape })));
                    }
                }
                Shape::Arc {
                    radius,
                    start_angle,
                    sweep,
                    ..
                } => {
                    let mut shape = c.shape.clone();
                    let r = ed.length(ui, "Radius", "radius", *radius);
                    let s = ed.number(ui, "Start Angle (°)", "start_angle", *start_angle);
                    let w = ed.number(ui, "Sweep (°)", "sweep", *sweep);
                    if let Shape::Arc {
                        radius,
                        start_angle,
                        sweep,
                        ..
                    } = &mut shape
                    {
                        if let Some(r) = r {
                            *radius = r;
                        }
                        if let Some(s) = s {
                            *start_angle = s;
                        }
                        if let Some(w) = w {
                            *sweep = w;
                        }
                    }
                    if r.is_some() || s.is_some() || w.is_some() {
                        ed.out.push(patch(id, json!({ "shape": shape })));
                    }
                }
                Shape::CrossBox { a, b, width }
                | Shape::BlockingBox { a, b, width }
                | Shape::Insulation { a, b, width } => {
                    Ed::label(ui, "Length");
                    ui.label(fmt(doc, a.distance(*b)));
                    ui.end_row();
                    if let Some(w) = ed.length(ui, "Depth", "width", *width)
                        && w.mm() > 0.0
                    {
                        let mut shape = c.shape.clone();
                        if let Shape::CrossBox { width, .. }
                        | Shape::BlockingBox { width, .. }
                        | Shape::Insulation { width, .. } = &mut shape
                        {
                            *width = w;
                        }
                        ed.out.push(patch(id, json!({ "shape": shape })));
                    }
                }
                Shape::Polyline { points, closed } | Shape::Spline { points, closed } => {
                    Ed::label(ui, "Points");
                    ui.label(points.len().to_string());
                    ui.end_row();
                    Ed::label(ui, "Closed");
                    let mut cl = *closed;
                    if ui.checkbox(&mut cl, "").changed() {
                        let mut shape = c.shape.clone();
                        if let Shape::Polyline { closed, .. } | Shape::Spline { closed, .. } =
                            &mut shape
                        {
                            *closed = cl;
                        }
                        ed.out.push(patch(id, json!({ "shape": shape })));
                    }
                    ui.end_row();
                }
                Shape::Ellipse {
                    rx, ry, rotation, ..
                } => {
                    let mut shape = c.shape.clone();
                    let a = ed.length(ui, "Radius X", "rx", *rx);
                    let b = ed.length(ui, "Radius Y", "ry", *ry);
                    let r = ed.number(ui, "Rotation (°)", "rotation", *rotation);
                    if let Shape::Ellipse {
                        rx, ry, rotation, ..
                    } = &mut shape
                    {
                        if let Some(a) = a {
                            *rx = a;
                        }
                        if let Some(b) = b {
                            *ry = b;
                        }
                        if let Some(r) = r {
                            *rotation = r;
                        }
                    }
                    if a.is_some() || b.is_some() || r.is_some() {
                        ed.out.push(patch(id, json!({ "shape": shape })));
                    }
                }
                Shape::Point { at } => {
                    if let Some(p) = ed.point(ui, "Position", "at", *at) {
                        ed.out
                            .push(patch(id, json!({ "shape": Shape::Point { at: p } })));
                    }
                }
            }
            ed.color(ui, "Line Color", "color", c.color, "By layer");
            ed.color(ui, "Fill", "fill", c.fill, "None");
            ed.pick::<Layer>(ui, "Layer", "layer", Some(c.layer), None);
        }
        Object::Dimension(d) => {
            if let DimensionGeometry::Linear {
                orientation,
                offset,
                ..
            } = &d.geometry
            {
                Ed::label(ui, "Orientation");
                let mut o = *orientation;
                ui.horizontal(|ui| {
                    ui.radio_value(&mut o, DimOrientation::Aligned, "Aligned");
                    ui.radio_value(&mut o, DimOrientation::Horizontal, "Horizontal");
                    ui.radio_value(&mut o, DimOrientation::Vertical, "Vertical");
                });
                ui.end_row();
                let off = ed.length(ui, "Offset", "offset", *offset);
                if o != *orientation || off.is_some() {
                    let mut g = d.geometry.clone();
                    if let DimensionGeometry::Linear {
                        orientation,
                        offset,
                        ..
                    } = &mut g
                    {
                        *orientation = o;
                        if let Some(v) = off {
                            *offset = v;
                        }
                    }
                    ed.out.push(patch(id, json!({ "geometry": g })));
                }
            }
            ed.text(
                ui,
                "Text Override ({} = value)",
                "text_override",
                d.text_override.as_deref().unwrap_or(""),
            );
            ed.pick::<Layer>(ui, "Layer", "layer", Some(d.layer), None);
        }
        Object::Text(t) => {
            Ed::label(ui, "Text");
            let key = ed.key("text");
            let buf = ed.bufs.text(ui, key, &t.text, 220.0);
            if let Some(s) = buf {
                ed.out.push(patch(id, json!({ "text": s })));
            }
            ui.end_row();
            ed.pick::<TextStyle>(
                ui,
                "Text Style",
                "style",
                t.style,
                Some("By layer / default"),
            );
            let style_height = t
                .style
                .and_then(|s| doc.get::<TextStyle>(s))
                .or_else(|| doc.get::<TextStyle>(doc.active_defaults().text_style))
                .map(|s| s.height);
            ed.opt_length(ui, "Size", "height", t.height, style_height);
            Ed::label(ui, "Bold");
            let mut bold = t.bold;
            egui::ComboBox::from_id_salt(ed.key("bold"))
                .selected_text(match bold {
                    None => "By style",
                    Some(true) => "Bold",
                    Some(false) => "Regular",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut bold, None, "By style");
                    ui.selectable_value(&mut bold, Some(true), "Bold");
                    ui.selectable_value(&mut bold, Some(false), "Regular");
                });
            if bold != t.bold {
                ed.out.push(patch(id, json!({ "bold": bold })));
            }
            ui.end_row();
            Ed::label(ui, "Justification");
            let mut j = t.justify;
            ui.horizontal(|ui| {
                ui.radio_value(&mut j, Justify::Left, "Left");
                ui.radio_value(&mut j, Justify::Center, "Center");
                ui.radio_value(&mut j, Justify::Right, "Right");
            });
            if j != t.justify {
                ed.out.push(patch(id, json!({ "justify": j })));
            }
            ui.end_row();
            if let Some(r) = ed.number(ui, "Rotation (°)", "rotation", t.rotation) {
                ed.out.push(patch(id, json!({ "rotation": r })));
            }
            ed.color(ui, "Color", "color", t.color, "By style / layer");
            ed.pick::<Layer>(ui, "Layer", "layer", Some(t.layer), None);
        }
        Object::Layer(l) => {
            ed.text(ui, "Name", "name", &l.name);
            ed.color(ui, "Color", "color", l.color, "Default ink");
            if let Some(w) = ed.number(ui, "Line Weight (mm)", "line_weight", l.line_weight) {
                ed.out.push(patch(id, json!({ "line_weight": w.max(0.0) })));
            }
            Ed::label(ui, "Line Style");
            let mut s = l.line_style;
            egui::ComboBox::from_id_salt(ed.key("line_style"))
                .selected_text(format!("{s:?}"))
                .show_ui(ui, |ui| {
                    for v in [
                        LineStyle::Solid,
                        LineStyle::Dashed,
                        LineStyle::Dotted,
                        LineStyle::DashDot,
                    ] {
                        ui.selectable_value(&mut s, v, format!("{v:?}"));
                    }
                });
            if s != l.line_style {
                ed.out.push(patch(id, json!({ "line_style": s })));
            }
            ui.end_row();
            ed.pick::<TextStyle>(
                ui,
                "Text Style",
                "text_style",
                l.text_style,
                Some("Default"),
            );
            Ed::label(ui, "Materials List");
            let mut m = l.in_materials_list;
            if ui.checkbox(&mut m, "Include").changed() {
                ed.out.push(patch(id, json!({ "in_materials_list": m })));
            }
            ui.end_row();
        }
        Object::TextStyle(s) => {
            ed.text(ui, "Name", "name", &s.name);
            if let Some(h) = ed.length(ui, "Character Height", "height", s.height) {
                ed.out.push(patch(id, json!({ "height": h })));
            }
            ed.text(ui, "Font", "font", &s.font);
            Ed::label(ui, "Style");
            let (mut b, mut i) = (s.bold, s.italic);
            ui.horizontal(|ui| {
                ui.checkbox(&mut b, "Bold");
                ui.checkbox(&mut i, "Italic");
            });
            if b != s.bold || i != s.italic {
                ed.out.push(patch(id, json!({ "bold": b, "italic": i })));
            }
            ui.end_row();
            ed.color(ui, "Color", "color", s.color, "By layer");
        }
        Object::PlanView(v) => {
            ed.text(ui, "Name", "name", &v.name);
            ed.pick::<Floor>(ui, "Floor", "floor", v.floor, Some("Current floor"));
            ed.pick::<LayerSet>(ui, "Layer Set", "layer_set", Some(v.layer_set), None);
            ed.pick::<DefaultSet>(ui, "Default Set", "default_set", Some(v.default_set), None);
            Ed::label(ui, "Reference Floor");
            let mut r = v.reference_floor;
            if ui.checkbox(&mut r, "Show floor below").changed() {
                ed.out.push(patch(id, json!({ "reference_floor": r })));
            }
            ui.end_row();
        }
        Object::Camera(c) => {
            Ed::label(ui, "Backdrop");
            ui.horizontal(|ui| {
                match &c.backdrop {
                    Some(b) => {
                        let name = std::path::Path::new(b)
                            .file_name()
                            .map_or(b.clone(), |f| f.to_string_lossy().into_owned());
                        ui.label(name).on_hover_text(b);
                    }
                    None => {
                        ui.weak("Sky color");
                    }
                }
                if ui.small_button("Choose…").clicked()
                    && let Some(f) = pick_image()
                {
                    ed.out.push(patch(id, json!({ "backdrop": f })));
                }
                if c.backdrop.is_some() && ui.small_button("Clear").clicked() {
                    ed.out.push(patch(id, json!({ "backdrop": Value::Null })));
                }
            });
            ui.end_row();
            ed.text(ui, "Name", "name", &c.name);
            Ed::label(ui, "Projection");
            let persp = matches!(c.projection, Projection::Perspective { .. });
            let mut p = persp;
            ui.horizontal(|ui| {
                ui.radio_value(&mut p, true, "Perspective");
                ui.radio_value(&mut p, false, "Orthographic");
            });
            if p != persp {
                let proj = if p {
                    Projection::Perspective { fov: 45.0 }
                } else {
                    Projection::Orthographic {
                        height: Length::from_m(15.0),
                    }
                };
                ed.out.push(patch(id, json!({ "projection": proj })));
            }
            ui.end_row();
            ed.pick::<LayerSet>(ui, "Layer Set", "layer_set", Some(c.layer_set), None);
        }
        Object::RoomType(t) => {
            ed.text(ui, "Name", "name", &t.name);
            ed.opt_length(
                ui,
                "Ceiling Height",
                "ceiling_height",
                t.ceiling_height,
                None,
            );
            ed.text(ui, "Floor Finish", "floor_finish", &t.floor_finish);
            ed.text(ui, "Wall Finish", "wall_finish", &t.wall_finish);
            Ed::label(ui, "Living Area");
            let mut l = t.living_area;
            if ui.checkbox(&mut l, "Count in living area").changed() {
                ed.out.push(patch(id, json!({ "living_area": l })));
            }
            ui.end_row();
        }
        Object::WallType(t) => {
            ed.text(ui, "Name", "name", &t.name);
            Ed::label(ui, "Layers");
            ui.vertical(|ui| {
                for l in &t.layers {
                    ui.label(format!(
                        "{}  {}  {}",
                        l.name,
                        fmt(doc, l.thickness),
                        l.material
                    ));
                }
                ui.weak("Edit layers in Tools > Wall Types.");
            });
            ui.end_row();
        }
        Object::LayerSet(s) => {
            ed.text(ui, "Name", "name", &s.name);
            Ed::label(ui, "");
            ui.weak("Toggle layers in the Layer Display side window.");
            ui.end_row();
        }
        Object::DefaultSet(s) => {
            ed.text(ui, "Name", "name", &s.name);
            Ed::label(ui, "");
            ui.weak("Edit values in Edit > Default Settings.");
            ui.end_row();
        }
    }
}

fn renovation_row(ui: &mut Ui, ed: &mut Ed<'_>, current: Renovation) {
    Ed::label(ui, "Renovation");
    let mut r = current;
    ui.horizontal(|ui| {
        for v in Renovation::ALL {
            ui.radio_value(&mut r, v, v.name());
        }
    });
    if r != current {
        ed.out.push(Command::SetRenovation {
            ids: vec![ed.id],
            status: r,
        });
    }
    ui.end_row();
}

/// Floor structure: one depth, or a list of layers (finish first).
fn platform_editor(ui: &mut Ui, ed: &mut Ed<'_>, f: &Floor) {
    if f.platform.is_empty() {
        if let Some(l) = ed.length(ui, "Floor Structure", "floor_structure", f.floor_structure) {
            ed.out.push(patch(ed.id, json!({ "floor_structure": l })));
        }
        Ed::label(ui, "");
        if ui.button("Define layers…").clicked() {
            ed.out
                .push(patch(ed.id, json!({ "platform": default_platform() })));
        }
        ui.end_row();
        return;
    }
    Ed::label(ui, "Floor Structure");
    ui.label(format!("{} total", fmt(ed.doc, f.floor_structure)));
    ui.end_row();
    let mut layers = f.platform.clone();
    let mut changed = false;
    let mut remove = None;
    for (i, layer) in f.platform.iter().enumerate() {
        Ed::label(ui, if i == 0 { "Layers (top first)" } else { "" });
        ui.horizontal(|ui| {
            let key = ed.key(&format!("platform_name_{i}"));
            if let Some(t) = ed.bufs.text(ui, key, &layer.name, 110.0) {
                layers[i].name = t;
                changed = true;
            }
            let key = ed.key(&format!("platform_thick_{i}"));
            if let Some(l) = ed.bufs.length(ui, ed.doc, key, layer.thickness)
                && l.mm() > 0.0
            {
                layers[i].thickness = l;
                changed = true;
            }
            if ui.small_button("✕").on_hover_text("Remove layer").clicked() {
                remove = Some(i);
            }
        });
        ui.end_row();
    }
    if let Some(i) = remove {
        layers.remove(i);
        changed = true;
    }
    Ed::label(ui, "");
    if ui.button("Add layer").clicked() {
        layers.push(WallLayer {
            name: "Layer".into(),
            thickness: Length::from_inches(0.5),
            material: String::new(),
        });
        changed = true;
    }
    ui.end_row();
    if changed {
        ed.out.push(patch(ed.id, json!({ "platform": layers })));
    }
}

pub fn opening_name(k: OpeningKind) -> String {
    match k {
        OpeningKind::Door(s) => s.name().into(),
        OpeningKind::Window(s) => s.name().into(),
    }
}

/// Ceiling style picker; returns the new style when changed.
fn ceiling_editor(ui: &mut Ui, ed: &mut Ed<'_>, current: CeilingStyle) -> Option<CeilingStyle> {
    Ed::label(ui, "Ceiling");
    let mut chosen = current;
    let name = |c: &CeilingStyle| match c {
        CeilingStyle::Flat => "Flat",
        CeilingStyle::Vaulted => "Vaulted (follows roof)",
        CeilingStyle::Tray { .. } => "Tray",
        CeilingStyle::Coffered { .. } => "Coffered",
        CeilingStyle::Barrel { .. } => "Barrel vault",
        CeilingStyle::None => "None (open above)",
    };
    let same_kind =
        |a: &CeilingStyle, b: &CeilingStyle| std::mem::discriminant(a) == std::mem::discriminant(b);
    egui::ComboBox::from_id_salt(ed.key("ceiling"))
        .selected_text(name(&current))
        .show_ui(ui, |ui| {
            for c in [
                CeilingStyle::Flat,
                CeilingStyle::Vaulted,
                CeilingStyle::Tray {
                    inset: Length::from_inches(18.0),
                    rise: Length::from_inches(12.0),
                },
                CeilingStyle::Coffered {
                    spacing: Length::from_inches(48.0),
                    beam_width: Length::from_inches(6.0),
                    depth: Length::from_inches(8.0),
                },
                CeilingStyle::Barrel {
                    rise: Length::from_inches(24.0),
                },
                CeilingStyle::None,
            ] {
                if ui
                    .selectable_label(same_kind(&c, &current), name(&c))
                    .clicked()
                    && !same_kind(&c, &current)
                {
                    chosen = c;
                }
            }
        });
    ui.end_row();
    if let CeilingStyle::Tray { inset, rise } = current {
        let i = ed.length(ui, "Tray Inset", "tray_inset", inset);
        let r = ed.length(ui, "Tray Rise", "tray_rise", rise);
        if i.is_some() || r.is_some() {
            chosen = CeilingStyle::Tray {
                inset: i.unwrap_or(inset),
                rise: r.unwrap_or(rise),
            };
        }
    }
    if let CeilingStyle::Coffered {
        spacing,
        beam_width,
        depth,
    } = current
    {
        let s = ed.length(ui, "Beam Spacing", "coffer_spacing", spacing);
        let w = ed.length(ui, "Beam Width", "coffer_width", beam_width);
        let d = ed.length(ui, "Beam Depth", "coffer_depth", depth);
        if s.is_some() || w.is_some() || d.is_some() {
            chosen = CeilingStyle::Coffered {
                spacing: s.unwrap_or(spacing),
                beam_width: w.unwrap_or(beam_width),
                depth: d.unwrap_or(depth),
            };
        }
    }
    if let CeilingStyle::Barrel { rise } = current
        && let Some(r) = ed.length(ui, "Vault Rise", "barrel_rise", rise)
    {
        chosen = CeilingStyle::Barrel { rise: r };
    }
    (chosen != current).then_some(chosen)
}

/// One window editing several rooms at once (type, ceiling, heights).
pub fn room_multi_window(app: &mut App, ctx: &egui::Context) {
    let Some(ids) = app.room_multi.clone() else {
        return;
    };
    let doc = app.session.document();
    let rooms: Vec<Room> = ids
        .iter()
        .filter_map(|id| doc.get::<Room>(*id).cloned())
        .collect();
    if rooms.is_empty() {
        app.room_multi = None;
        return;
    }
    let mut open = true;
    let mut set: Option<Value> = None;
    let bufs = &mut app.bufs;
    egui::Window::new(format!("Room Specification ({} rooms)", rooms.len()))
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            egui::Grid::new("room_multi").num_columns(2).show(ui, |ui| {
                ui.label("Room Type");
                let same_type = rooms.iter().all(|r| r.room_type == rooms[0].room_type);
                let current = if same_type {
                    doc.get::<RoomType>(rooms[0].room_type)
                        .map_or("", |t| t.name.as_str())
                } else {
                    "(various)"
                };
                egui::ComboBox::from_id_salt("multi_room_type")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for t in doc.iter::<RoomType>() {
                            if ui.selectable_label(false, &t.name).clicked() {
                                set = Some(json!({ "room_type": t.id }));
                            }
                        }
                    });
                ui.end_row();
                ui.label("Ceiling");
                ui.horizontal(|ui| {
                    for (label, style) in [
                        ("Flat", CeilingStyle::Flat),
                        ("Vaulted", CeilingStyle::Vaulted),
                        ("None", CeilingStyle::None),
                    ] {
                        if ui.button(label).clicked() {
                            set = Some(json!({ "ceiling": style }));
                        }
                    }
                });
                ui.end_row();
                ui.label("Ceiling Height");
                ui.horizontal(|ui| {
                    if ui.button("Use default").clicked() {
                        set = Some(json!({ "ceiling_height": null }));
                    }
                    let key = egui::Id::unique("multi_ceiling_height");
                    let shown = doc
                        .room_ceiling_height(&rooms[0])
                        .unwrap_or(Length::from_feet(8.0));
                    if let Some(l) = bufs.length(ui, doc, key, shown)
                        && l.mm() > 0.0
                    {
                        set = Some(json!({ "ceiling_height": l }));
                    }
                });
                ui.end_row();
            });
            ui.weak("Changes apply to every selected room.");
        });
    if !open {
        app.room_multi = None;
    }
    if let Some(v) = set {
        let commands = ids.iter().map(|id| patch(*id, v.clone())).collect();
        app.run(commands);
    }
}

fn face_item_name(i: FaceItem) -> &'static str {
    match i {
        FaceItem::DoorLeft => "Door (hinge left)",
        FaceItem::DoorRight => "Door (hinge right)",
        FaceItem::DoubleDoor => "Pair of doors",
        FaceItem::Drawer => "Drawer",
        FaceItem::Open => "Open shelves",
        FaceItem::Panel => "Fixed panel",
        FaceItem::GlassDoor => "Glass door",
    }
}

fn cabinet_editor(ui: &mut Ui, ed: &mut Ed<'_>, c: &Cabinet) {
    let id = ed.id;
    let doc = ed.doc;
    ed.choice(
        ui,
        "Type",
        "cabinet_kind",
        c.cabinet_kind,
        &[
            (CabinetKind::Base, "Base"),
            (CabinetKind::Wall, "Wall"),
            (CabinetKind::Full, "Full height"),
        ],
    );
    for (label, field, v) in [
        ("Width", "width", c.width),
        ("Depth", "depth", c.depth),
        ("Height", "height", c.height),
    ] {
        if let Some(l) = ed.length(ui, label, field, v)
            && l.mm() > 0.0
        {
            ed.out.push(patch(id, json!({ field: l })));
        }
    }
    if let Some(l) = ed.length(ui, "Off the Floor", "elevation", c.elevation) {
        ed.out.push(patch(id, json!({ "elevation": l })));
    }
    if let Some(n) = ed.number(ui, "Facing (°)", "facing", c.facing) {
        ed.out.push(patch(id, json!({ "facing": n })));
    }
    ed.choice(
        ui,
        "Construction",
        "construction",
        c.construction,
        &[
            (CabinetConstruction::Framed, "Face frame"),
            (CabinetConstruction::Frameless, "Frameless"),
        ],
    );
    // Shape, with its size when it has one.
    Ed::label(ui, "Shape");
    let names = ["Standard", "Angled ends", "Bow front", "Curved"];
    let current = match c.shape {
        CabinetShape::Standard => 0,
        CabinetShape::Angled { .. } => 1,
        CabinetShape::Bow { .. } => 2,
        CabinetShape::Curved { .. } => 3,
    };
    let mut chosen = current;
    egui::ComboBox::from_id_salt(ed.key("shape"))
        .selected_text(names[current])
        .show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                ui.selectable_value(&mut chosen, i, *n);
            }
        });
    if chosen != current {
        let i = Length::from_inches;
        let shape = match chosen {
            1 => CabinetShape::Angled { left: i(0.0), right: i(12.0) },
            2 => CabinetShape::Bow { depth: i(3.0) },
            3 => CabinetShape::Curved { radius: i(48.0) },
            _ => CabinetShape::Standard,
        };
        ed.out.push(patch(id, json!({ "shape": shape })));
    }
    ui.end_row();
    match c.shape {
        CabinetShape::Angled { left, right } => {
            let l = ed.length(ui, "Angled Left", "shape.left", left);
            let r = ed.length(ui, "Angled Right", "shape.right", right);
            if l.is_some() || r.is_some() {
                let shape = CabinetShape::Angled { left: l.unwrap_or(left), right: r.unwrap_or(right) };
                ed.out.push(patch(id, json!({ "shape": shape })));
            }
        }
        CabinetShape::Bow { depth } => {
            if let Some(d) = ed.length(ui, "Bow Depth", "shape.depth", depth) {
                ed.out.push(patch(id, json!({ "shape": CabinetShape::Bow { depth: d } })));
            }
        }
        CabinetShape::Curved { radius } => {
            if let Some(r) = ed.length(ui, "Radius (− = concave)", "shape.radius", radius) {
                ed.out.push(patch(id, json!({ "shape": CabinetShape::Curved { radius: r } })));
            }
        }
        CabinetShape::Standard => {}
    }
    ed.choice(
        ui,
        "Door Style",
        "door_style",
        c.door_style,
        &[
            (DoorStyleCab::Slab, "Slab"),
            (DoorStyleCab::Shaker, "Shaker"),
            (DoorStyleCab::Raised, "Raised panel"),
        ],
    );
    ed.choice(
        ui,
        "Hardware",
        "hardware",
        c.hardware,
        &[
            (HardwareStyle::Pull, "Pulls"),
            (HardwareStyle::Knob, "Knobs"),
            (HardwareStyle::None, "None"),
        ],
    );
    // Front layout: rows top to bottom.
    Ed::label(ui, "Front (top to bottom)");
    ui.vertical(|ui| {
        let mut face = c.face.clone();
        let mut changed = false;
        let mut remove_row = None;
        let items = [
            FaceItem::DoorLeft,
            FaceItem::DoorRight,
            FaceItem::DoubleDoor,
            FaceItem::Drawer,
            FaceItem::Open,
            FaceItem::Panel,
            FaceItem::GlassDoor,
        ];
        for (r, row) in face.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                for (k, item) in row.items.iter_mut().enumerate() {
                    egui::ComboBox::from_id_salt(("face", id, r, k))
                        .selected_text(face_item_name(*item))
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for i in items {
                                if ui.selectable_value(item, i, face_item_name(i)).changed() {
                                    changed = true;
                                }
                            }
                        });
                }
                if ui.small_button("+").on_hover_text("Split into another cell").clicked() {
                    row.items.push(FaceItem::Drawer);
                    changed = true;
                }
                if row.items.len() > 1 && ui.small_button("−").on_hover_text("Remove a cell").clicked() {
                    row.items.pop();
                    changed = true;
                }
                let mut fixed = row.height.is_some();
                if ui.checkbox(&mut fixed, "fixed").changed() {
                    row.height = fixed.then(|| Length::from_inches(6.0));
                    changed = true;
                }
                if let Some(h) = row.height {
                    let mut inches = h.inches();
                    if ui
                        .add(egui::DragValue::new(&mut inches).range(2.0..=96.0).suffix("\""))
                        .changed()
                    {
                        row.height = Some(Length::from_inches(inches));
                        changed = true;
                    }
                }
                if ui.small_button("✕").on_hover_text("Remove this row").clicked() {
                    remove_row = Some(r);
                }
            });
        }
        if let Some(r) = remove_row
            && face.len() > 1
        {
            face.remove(r);
            changed = true;
        }
        if ui.small_button("Add row").clicked() {
            face.push(FaceRow { height: None, items: vec![FaceItem::DoorRight] });
            changed = true;
        }
        if changed {
            ed.out.push(patch(id, json!({ "face": face })));
        }
    });
    ui.end_row();
    if let Some(n) = ed.number(ui, "Shelves", "shelves", c.shelves as f64) {
        ed.out.push(patch(id, json!({ "shelves": n.round().max(0.0) as u32 })));
    }
    // Toe kick.
    if c.cabinet_kind != CabinetKind::Wall {
        Ed::label(ui, "Toe Kick");
        let mut on = c.toe_kick.is_some();
        if ui.checkbox(&mut on, "").changed() {
            let d = &doc.active_defaults().cabinets;
            let v = if on { json!([d.toe_kick_height, d.toe_kick_depth]) } else { Value::Null };
            ed.out.push(patch(id, json!({ "toe_kick": v })));
        }
        ui.end_row();
    }
    // Countertop.
    Ed::label(ui, "Countertop");
    let mut on = c.countertop.is_some();
    if ui.checkbox(&mut on, "").changed() {
        let d = &doc.active_defaults().cabinets;
        let v = if on {
            json!(CabinetTop {
                thickness: d.countertop_thickness,
                overhang: d.countertop_overhang,
                edge: d.edge,
                backsplash: d.backsplash,
            })
        } else {
            Value::Null
        };
        ed.out.push(patch(id, json!({ "countertop": v })));
    }
    ui.end_row();
    if let Some(top) = &c.countertop {
        let mut t = top.clone();
        let mut changed = false;
        for (label, field, value) in [
            ("  Thickness", "thickness", top.thickness),
            ("  Overhang", "overhang", top.overhang),
            ("  Backsplash", "backsplash", top.backsplash),
        ] {
            if let Some(l) = ed.length(ui, label, &format!("countertop.{field}"), value) {
                match field {
                    "thickness" => t.thickness = l,
                    "overhang" => t.overhang = l,
                    _ => t.backsplash = l,
                }
                changed = true;
            }
        }
        Ed::label(ui, "  Edge Profile");
        egui::ComboBox::from_id_salt(ed.key("countertop.edge"))
            .selected_text(top.edge.name())
            .show_ui(ui, |ui| {
                for e in EdgeProfile::ALL {
                    if ui.selectable_value(&mut t.edge, e, e.name()).changed() {
                        changed = true;
                    }
                }
            });
        ui.end_row();
        if changed {
            ed.out.push(patch(id, json!({ "countertop": t })));
        }
        if c.cabinet_kind == CabinetKind::Base {
            ed.check(ui, "Sink", "sink", "Sink in this cabinet", c.sink);
        }
    }
    if let Some(l) = ed.length(ui, "Filler Left", "filler_left", c.filler_left) {
        ed.out.push(patch(id, json!({ "filler_left": l })));
    }
    if let Some(l) = ed.length(ui, "Filler Right", "filler_right", c.filler_right) {
        ed.out.push(patch(id, json!({ "filler_right": l })));
    }
    ed.check(ui, "Auto Fillers", "no_auto_filler", "Exclude from auto fillers", c.no_auto_filler);
    ed.pick::<Layer>(ui, "Layer", "layer", Some(c.layer), None);
}

/// Ask for an image file.
pub fn pick_image() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "tif", "tiff"])
        .pick_file()
        .map(|p| p.to_string_lossy().into_owned())
}
