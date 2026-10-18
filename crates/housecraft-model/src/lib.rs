//! The HouseCraft document model.
//!
//! A [`Document`] is plain data. All edits from the GUI, CLI, MCP server and
//! scripts go through `housecraft-api` commands, which keep it consistent;
//! the fields are public so readers and file-format crates can build it directly.

use std::collections::BTreeMap;
use std::fmt;

use housecraft_units::{DisplayUnit, Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Document-scoped object identifier. Never reused within a document.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct Id(pub u64);

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Floor,
    WallType,
    Wall,
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ObjectKind::Floor => "floor",
            ObjectKind::WallType => "wall_type",
            ObjectKind::Wall => "wall",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub settings: Settings,
    /// Next id to hand out; always greater than every id in the document.
    pub next_id: u64,
    pub floors: BTreeMap<Id, Floor>,
    pub wall_types: BTreeMap<Id, WallType>,
    pub walls: BTreeMap<Id, Wall>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Settings {
    pub display_unit: DisplayUnit,
    /// Wall type used when a command does not specify one.
    pub default_wall_type: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Floor {
    pub id: Id,
    pub name: String,
    /// Elevation of the finished floor surface above the project datum.
    pub elevation: Length,
    /// Finished floor to finished ceiling.
    pub ceiling_height: Length,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallType {
    pub id: Id,
    pub name: String,
    /// Layers listed exterior to interior.
    pub layers: Vec<WallLayer>,
}

impl WallType {
    pub fn thickness(&self) -> Length {
        self.layers.iter().map(|l| l.thickness).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallLayer {
    pub name: String,
    pub thickness: Length,
    #[serde(default)]
    pub material: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wall {
    pub id: Id,
    pub floor: Id,
    /// Wall centreline start, in plan coordinates.
    pub start: Point2,
    /// Wall centreline end, in plan coordinates.
    pub end: Point2,
    pub wall_type: Id,
    /// Overrides the floor's ceiling height when set.
    pub height: Option<Length>,
}

impl Wall {
    pub fn length(&self) -> Length {
        self.start.distance(self.end)
    }
}

/// A borrowed reference to any object in the document.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectRef<'a> {
    Floor(&'a Floor),
    WallType(&'a WallType),
    Wall(&'a Wall),
}

impl ObjectRef<'_> {
    pub fn kind(&self) -> ObjectKind {
        match self {
            ObjectRef::Floor(_) => ObjectKind::Floor,
            ObjectRef::WallType(_) => ObjectKind::WallType,
            ObjectRef::Wall(_) => ObjectKind::Wall,
        }
    }
}

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

impl Document {
    /// A document with no objects at all.
    pub fn empty(name: impl Into<String>) -> Self {
        Document {
            name: name.into(),
            settings: Settings {
                display_unit: DisplayUnit::default(),
                default_wall_type: None,
            },
            next_id: 1,
            floors: BTreeMap::new(),
            wall_types: BTreeMap::new(),
            walls: BTreeMap::new(),
        }
    }

    /// A new plan with one floor and standard US wood-frame wall types.
    pub fn new(name: impl Into<String>) -> Self {
        let mut doc = Document::empty(name);
        let floor = doc.alloc_id();
        doc.floors.insert(
            floor,
            Floor {
                id: floor,
                name: "Floor 1".into(),
                elevation: Length::ZERO,
                ceiling_height: Length::from_feet(8.0),
            },
        );
        let layer = |name: &str, inches: f64, material: &str| WallLayer {
            name: name.into(),
            thickness: Length::from_inches(inches),
            material: material.into(),
        };
        let exterior = doc.alloc_id();
        doc.wall_types.insert(
            exterior,
            WallType {
                id: exterior,
                name: "Exterior 2x6".into(),
                layers: vec![
                    layer("Siding", 0.75, "Lap siding"),
                    layer("Sheathing", 7.0 / 16.0, "OSB"),
                    layer("Framing", 5.5, "2x6 stud"),
                    layer("Interior finish", 0.5, "Gypsum board"),
                ],
            },
        );
        let interior = doc.alloc_id();
        doc.wall_types.insert(
            interior,
            WallType {
                id: interior,
                name: "Interior 2x4".into(),
                layers: vec![
                    layer("Finish", 0.5, "Gypsum board"),
                    layer("Framing", 3.5, "2x4 stud"),
                    layer("Finish", 0.5, "Gypsum board"),
                ],
            },
        );
        doc.settings.default_wall_type = Some(exterior);
        doc
    }

    pub fn alloc_id(&mut self) -> Id {
        let id = Id(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn object(&self, id: Id) -> Option<ObjectRef<'_>> {
        if let Some(f) = self.floors.get(&id) {
            Some(ObjectRef::Floor(f))
        } else if let Some(t) = self.wall_types.get(&id) {
            Some(ObjectRef::WallType(t))
        } else {
            self.walls.get(&id).map(ObjectRef::Wall)
        }
    }

    pub fn kind_of(&self, id: Id) -> Option<ObjectKind> {
        self.object(id).map(|o| o.kind())
    }

    /// Floors ordered from lowest to highest elevation.
    pub fn floors_by_elevation(&self) -> Vec<&Floor> {
        let mut floors: Vec<_> = self.floors.values().collect();
        floors.sort_by(|a, b| {
            a.elevation
                .mm()
                .total_cmp(&b.elevation.mm())
                .then(a.id.cmp(&b.id))
        });
        floors
    }

    pub fn walls_on(&self, floor: Id) -> impl Iterator<Item = &Wall> {
        self.walls.values().filter(move |w| w.floor == floor)
    }

    /// Height of a wall, falling back to its floor's ceiling height.
    pub fn wall_height(&self, wall: &Wall) -> Option<Length> {
        wall.height
            .or_else(|| self.floors.get(&wall.floor).map(|f| f.ceiling_height))
    }

    /// Check referential integrity and basic invariants.
    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut issue = |object: Id, message: String| issues.push(Issue { object, message });

        let max_id = self
            .floors
            .keys()
            .chain(self.wall_types.keys())
            .chain(self.walls.keys())
            .max();
        if let Some(max) = max_id
            && max.0 >= self.next_id
        {
            issue(
                *max,
                format!("next_id {} is not above the largest id", self.next_id),
            );
        }
        let mut seen = std::collections::HashSet::new();
        for id in self
            .floors
            .keys()
            .chain(self.wall_types.keys())
            .chain(self.walls.keys())
        {
            if !seen.insert(*id) {
                issue(*id, "id used by more than one object".into());
            }
        }
        for (key, f) in &self.floors {
            if *key != f.id {
                issue(*key, format!("stored under key {key} but has id {}", f.id));
            }
            if f.ceiling_height.mm() <= 0.0 {
                issue(f.id, "ceiling height must be positive".into());
            }
        }
        for (key, t) in &self.wall_types {
            if *key != t.id {
                issue(*key, format!("stored under key {key} but has id {}", t.id));
            }
            if t.layers.is_empty() {
                issue(t.id, "wall type has no layers".into());
            }
        }
        for (key, w) in &self.walls {
            if *key != w.id {
                issue(*key, format!("stored under key {key} but has id {}", w.id));
            }
            if !self.floors.contains_key(&w.floor) {
                issue(w.id, format!("references missing floor {}", w.floor));
            }
            if !self.wall_types.contains_key(&w.wall_type) {
                issue(
                    w.id,
                    format!("references missing wall type {}", w.wall_type),
                );
            }
        }
        if let Some(t) = self.settings.default_wall_type
            && !self.wall_types.contains_key(&t)
        {
            issue(t, "default wall type does not exist".into());
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_is_valid() {
        let doc = Document::new("Test");
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
        assert_eq!(doc.floors.len(), 1);
        let ext = &doc.wall_types[&doc.settings.default_wall_type.unwrap()];
        assert!((ext.thickness().inches() - 7.1875).abs() < 1e-9);
    }

    #[test]
    fn validate_catches_dangling_refs() {
        let mut doc = Document::new("Test");
        let id = doc.alloc_id();
        doc.walls.insert(
            id,
            Wall {
                id,
                floor: Id(999),
                start: Point2::from_mm(0.0, 0.0),
                end: Point2::from_mm(1000.0, 0.0),
                wall_type: Id(998),
                height: None,
            },
        );
        assert_eq!(doc.validate().len(), 2);
    }

    #[test]
    fn round_trips_through_json() {
        let doc = Document::new("Test");
        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, back);
    }
}
