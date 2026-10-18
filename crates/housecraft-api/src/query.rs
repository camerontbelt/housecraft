//! Read-only views of a document, shaped for CLI output and MCP responses.

use housecraft_model::{Document, Id, ObjectKind, ObjectRef};
use housecraft_units::Length;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentSummary {
    pub name: String,
    pub floors: Vec<FloorSummary>,
    pub wall_types: Vec<ObjectInfo>,
    pub wall_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FloorSummary {
    pub id: Id,
    pub name: String,
    /// Formatted in the document's display unit.
    pub elevation: String,
    pub ceiling_height: String,
    pub wall_count: usize,
    pub total_wall_length: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObjectInfo {
    pub id: Id,
    pub kind: ObjectKind,
    pub label: String,
}

pub fn summary(doc: &Document) -> DocumentSummary {
    let fmt = |l: Length| l.format(doc.settings.display_unit);
    DocumentSummary {
        name: doc.name.clone(),
        floors: doc
            .floors_by_elevation()
            .into_iter()
            .map(|f| FloorSummary {
                id: f.id,
                name: f.name.clone(),
                elevation: fmt(f.elevation),
                ceiling_height: fmt(f.ceiling_height),
                wall_count: doc.walls_on(f.id).count(),
                total_wall_length: fmt(doc.walls_on(f.id).map(|w| w.length()).sum()),
            })
            .collect(),
        wall_types: list_objects(doc, Some(ObjectKind::WallType)),
        wall_count: doc.walls.len(),
    }
}

/// List objects, optionally filtered to one kind, in id order.
pub fn list_objects(doc: &Document, kind: Option<ObjectKind>) -> Vec<ObjectInfo> {
    let mut ids: Vec<Id> = doc
        .floors
        .keys()
        .chain(doc.wall_types.keys())
        .chain(doc.walls.keys())
        .copied()
        .collect();
    ids.sort();
    ids.into_iter()
        .filter_map(|id| doc.object(id))
        .filter(|o| kind.is_none_or(|k| o.kind() == k))
        .map(|o| info(doc, o))
        .collect()
}

/// Full JSON for one object, with a `kind` tag, or `None` if the id is unknown.
pub fn get_object(doc: &Document, id: Id) -> Option<serde_json::Value> {
    doc.object(id)
        .map(|o| serde_json::to_value(o).expect("model serializes"))
}

fn info(doc: &Document, obj: ObjectRef<'_>) -> ObjectInfo {
    let fmt = |l: Length| l.format(doc.settings.display_unit);
    let (id, label) = match obj {
        ObjectRef::Floor(f) => (f.id, f.name.clone()),
        ObjectRef::WallType(t) => (t.id, format!("{} ({})", t.name, fmt(t.thickness()))),
        ObjectRef::Wall(w) => {
            let type_name = doc
                .wall_types
                .get(&w.wall_type)
                .map_or("?", |t| t.name.as_str());
            (w.id, format!("{type_name}, {} long", fmt(w.length())))
        }
    };
    ObjectInfo {
        id,
        kind: obj.kind(),
        label,
    }
}
