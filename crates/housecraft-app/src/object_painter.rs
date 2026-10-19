//! The object painter: copy chosen properties from one object to others
//! of the same kind (wall type and height, cabinet door style, text style…).

use std::collections::BTreeSet;

use egui::Context;
use housecraft_api::Command;
use housecraft_api::model::{Document, Id};
use serde_json::{Map, Value};

use crate::app::App;

/// Properties that say where an object is or what it belongs to, not how
/// it looks; never copied.
const PLACEMENT: &[&str] = &[
    "id", "kind", "floor", "position", "start", "end", "at", "points", "anchor", "wall", "a", "b",
    "center", "target", "controls", "geometry", "shape", "outline", "facing", "rotation",
];

pub struct ObjectPainter {
    pub source: Id,
    pub fields: BTreeSet<String>,
}

impl ObjectPainter {
    pub fn new(source: Id) -> Self {
        ObjectPainter {
            source,
            fields: BTreeSet::new(),
        }
    }

    fn values(doc: &Document, id: Id) -> Option<Map<String, Value>> {
        serde_json::to_value(doc.object(id)?).ok()?.as_object().cloned()
    }

    /// An update giving `target` the chosen properties of the source.
    pub fn apply_to(&self, doc: &Document, target: Id) -> Option<Command> {
        if target == self.source || doc.kind_of(target) != doc.kind_of(self.source) || self.fields.is_empty() {
            return None;
        }
        let src = Self::values(doc, self.source)?;
        let set: Map<String, Value> = self
            .fields
            .iter()
            .filter_map(|f| Some((f.clone(), src.get(f)?.clone())))
            .collect();
        Some(Command::Update { id: target, set })
    }
}

/// The window listing the source's properties to copy.
pub fn window(app: &mut App, ctx: &Context) {
    if app.plan.tool != crate::tools::Tool::ObjectPainter {
        return;
    }
    let doc = app.session.document();
    let Some(op) = &mut app.object_painter else {
        return;
    };
    let Some(values) = ObjectPainter::values(doc, op.source) else {
        app.object_painter = None;
        return;
    };
    let kind = doc.kind_of(op.source).map_or("object".into(), |k| k.to_string());
    let mut scope_all = false;
    egui::Window::new("Object Painter")
        .resizable(false)
        .default_pos(egui::pos2(260.0, 160.0))
        .show(ctx, |ui| {
            let what = doc
                .object(op.source)
                .map(|o| housecraft_api::query::describe(doc, o).label)
                .unwrap_or_default();
            ui.label(format!("Copy from {kind} {what}:"));
            for (k, v) in &values {
                if PLACEMENT.contains(&k.as_str()) {
                    continue;
                }
                let mut on = op.fields.contains(k);
                let shown = match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "default".into(),
                    _ => "…".into(),
                };
                if ui.checkbox(&mut on, format!("{k}: {shown}")).changed() {
                    if on {
                        op.fields.insert(k.clone());
                    } else {
                        op.fields.remove(k);
                    }
                }
            }
            ui.separator();
            ui.weak("Click objects to paint them; Alt+click picks a new source.");
            scope_all = ui.button("Paint every one on this floor").clicked();
        });
    if scope_all {
        let floor = app.plan.floor;
        let kind = doc.kind_of(op.source);
        let commands: Vec<Command> = doc
            .objects
            .values()
            .filter(|o| Some(o.kind()) == kind && doc.floor_of(o.id()) == Some(floor))
            .filter_map(|o| op.apply_to(doc, o.id()))
            .collect();
        if !commands.is_empty() {
            app.run(commands);
        }
    }
}
