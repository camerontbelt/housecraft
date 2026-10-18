use housecraft_model::{Document, Floor, Id, ObjectKind, ObjectRef, Wall, WallLayer, WallType};
use housecraft_units::{DisplayUnit, Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ApiError;

/// A single edit to a document. Serialized with an `"op"` tag, e.g.
/// `{"op": "add_wall", "start": [0, 0], "end": ["20'", 0]}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Command {
    /// Rename the document.
    SetDocumentName { name: String },
    /// Change how lengths are displayed.
    SetDisplayUnit { unit: DisplayUnit },
    /// Add a floor. Elevation defaults to the top of the highest floor.
    AddFloor {
        name: Option<String>,
        elevation: Option<Length>,
        /// Defaults to the highest floor's ceiling height, or 8'.
        ceiling_height: Option<Length>,
    },
    /// Change properties of a floor; omitted fields are left alone.
    UpdateFloor {
        id: Id,
        name: Option<String>,
        elevation: Option<Length>,
        ceiling_height: Option<Length>,
    },
    /// Add a layered wall type.
    AddWallType {
        name: String,
        /// Layers listed exterior to interior.
        layers: Vec<WallLayer>,
    },
    /// Set the wall type used when commands do not specify one.
    SetDefaultWallType { id: Id },
    /// Add one straight wall along a centreline.
    AddWall {
        /// Defaults to the lowest floor.
        floor: Option<Id>,
        start: Point2,
        end: Point2,
        /// Defaults to the document's default wall type.
        wall_type: Option<Id>,
        /// Defaults to the floor's ceiling height.
        height: Option<Length>,
    },
    /// Add a connected run of walls through `points`; `closed` adds the closing wall.
    AddWalls {
        floor: Option<Id>,
        points: Vec<Point2>,
        #[serde(default)]
        closed: bool,
        wall_type: Option<Id>,
        height: Option<Length>,
    },
    /// Move one or both wall endpoints.
    MoveWall {
        id: Id,
        start: Option<Point2>,
        end: Option<Point2>,
    },
    /// Change a wall's type or height override. `clear_height` removes the override.
    UpdateWall {
        id: Id,
        wall_type: Option<Id>,
        height: Option<Length>,
        #[serde(default)]
        clear_height: bool,
    },
    /// Delete an object. Deleting a floor deletes its walls; a wall type in use cannot be deleted.
    Delete { id: Id },
}

/// What a command or batch changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Outcome {
    pub created: Vec<Id>,
    pub modified: Vec<Id>,
    pub deleted: Vec<Id>,
}

impl Outcome {
    pub(crate) fn merge(&mut self, other: Outcome) {
        self.created.extend(other.created);
        self.modified.extend(other.modified);
        self.deleted.extend(other.deleted);
    }
}

impl Command {
    /// The `op` tag, for error messages.
    pub fn op(&self) -> &'static str {
        match self {
            Command::SetDocumentName { .. } => "set_document_name",
            Command::SetDisplayUnit { .. } => "set_display_unit",
            Command::AddFloor { .. } => "add_floor",
            Command::UpdateFloor { .. } => "update_floor",
            Command::AddWallType { .. } => "add_wall_type",
            Command::SetDefaultWallType { .. } => "set_default_wall_type",
            Command::AddWall { .. } => "add_wall",
            Command::AddWalls { .. } => "add_walls",
            Command::MoveWall { .. } => "move_wall",
            Command::UpdateWall { .. } => "update_wall",
            Command::Delete { .. } => "delete",
        }
    }

    /// Apply to `doc` in place. On error `doc` may be partially modified, so
    /// callers apply to a scratch copy (see [`crate::Session::apply`]).
    pub(crate) fn apply(&self, doc: &mut Document) -> Result<Outcome, ApiError> {
        let mut out = Outcome::default();
        match self {
            Command::SetDocumentName { name } => doc.name = name.clone(),
            Command::SetDisplayUnit { unit } => {
                if let DisplayUnit::FeetInches { denominator } | DisplayUnit::Inches { denominator } =
                    unit
                    && *denominator == 0
                {
                    return invalid("denominator must be at least 1");
                }
                doc.settings.display_unit = *unit;
            }
            Command::AddFloor {
                name,
                elevation,
                ceiling_height,
            } => {
                let top = doc.floors_by_elevation().last().map(|f| (*f).clone());
                let ceiling_height = ceiling_height
                    .or(top.as_ref().map(|f| f.ceiling_height))
                    .unwrap_or(Length::from_feet(8.0));
                positive(ceiling_height, "ceiling_height")?;
                let elevation = elevation
                    .or(top.as_ref().map(|f| f.elevation + f.ceiling_height))
                    .unwrap_or(Length::ZERO);
                let id = doc.alloc_id();
                let name = name
                    .clone()
                    .unwrap_or_else(|| format!("Floor {}", doc.floors.len() + 1));
                doc.floors.insert(
                    id,
                    Floor {
                        id,
                        name,
                        elevation,
                        ceiling_height,
                    },
                );
                out.created.push(id);
            }
            Command::UpdateFloor {
                id,
                name,
                elevation,
                ceiling_height,
            } => {
                expect_kind(doc, *id, ObjectKind::Floor)?;
                if let Some(h) = ceiling_height {
                    positive(*h, "ceiling_height")?;
                }
                let floor = doc.floors.get_mut(id).expect("checked");
                if let Some(n) = name {
                    floor.name = n.clone();
                }
                if let Some(e) = elevation {
                    floor.elevation = *e;
                }
                if let Some(h) = ceiling_height {
                    floor.ceiling_height = *h;
                }
                out.modified.push(*id);
            }
            Command::AddWallType { name, layers } => {
                if layers.is_empty() {
                    return invalid("a wall type needs at least one layer");
                }
                for l in layers {
                    positive(l.thickness, "layer thickness")?;
                }
                let id = doc.alloc_id();
                doc.wall_types.insert(
                    id,
                    WallType {
                        id,
                        name: name.clone(),
                        layers: layers.clone(),
                    },
                );
                out.created.push(id);
            }
            Command::SetDefaultWallType { id } => {
                expect_kind(doc, *id, ObjectKind::WallType)?;
                doc.settings.default_wall_type = Some(*id);
            }
            Command::AddWall {
                floor,
                start,
                end,
                wall_type,
                height,
            } => {
                let (floor, wall_type) = wall_context(doc, *floor, *wall_type, *height)?;
                out.created
                    .push(add_wall(doc, floor, *start, *end, wall_type, *height)?);
            }
            Command::AddWalls {
                floor,
                points,
                closed,
                wall_type,
                height,
            } => {
                if points.len() < 2 {
                    return invalid("add_walls needs at least two points");
                }
                let (floor, wall_type) = wall_context(doc, *floor, *wall_type, *height)?;
                let mut segments: Vec<_> = points.windows(2).map(|w| (w[0], w[1])).collect();
                if *closed {
                    if points.len() < 3 {
                        return invalid("a closed run needs at least three points");
                    }
                    segments.push((*points.last().expect("len >= 3"), points[0]));
                }
                for (a, b) in segments {
                    out.created
                        .push(add_wall(doc, floor, a, b, wall_type, *height)?);
                }
            }
            Command::MoveWall { id, start, end } => {
                expect_kind(doc, *id, ObjectKind::Wall)?;
                let wall = doc.walls.get_mut(id).expect("checked");
                if let Some(s) = start {
                    wall.start = *s;
                }
                if let Some(e) = end {
                    wall.end = *e;
                }
                check_wall_length(wall.start, wall.end)?;
                out.modified.push(*id);
            }
            Command::UpdateWall {
                id,
                wall_type,
                height,
                clear_height,
            } => {
                expect_kind(doc, *id, ObjectKind::Wall)?;
                if let Some(t) = wall_type {
                    expect_kind(doc, *t, ObjectKind::WallType)?;
                }
                if let Some(h) = height {
                    positive(*h, "height")?;
                }
                if *clear_height && height.is_some() {
                    return invalid("give either height or clear_height, not both");
                }
                let wall = doc.walls.get_mut(id).expect("checked");
                if let Some(t) = wall_type {
                    wall.wall_type = *t;
                }
                if let Some(h) = height {
                    wall.height = Some(*h);
                }
                if *clear_height {
                    wall.height = None;
                }
                out.modified.push(*id);
            }
            Command::Delete { id } => match doc.object(*id).ok_or(ApiError::NotFound(*id))? {
                ObjectRef::Floor(_) => {
                    let walls: Vec<Id> = doc.walls_on(*id).map(|w| w.id).collect();
                    for w in &walls {
                        doc.walls.remove(w);
                    }
                    doc.floors.remove(id);
                    out.deleted.extend(walls);
                    out.deleted.push(*id);
                }
                ObjectRef::WallType(_) => {
                    if let Some(w) = doc.walls.values().find(|w| w.wall_type == *id) {
                        return invalid(format!("wall type {id} is used by wall {}", w.id));
                    }
                    if doc.settings.default_wall_type == Some(*id) {
                        doc.settings.default_wall_type = None;
                    }
                    doc.wall_types.remove(id);
                    out.deleted.push(*id);
                }
                ObjectRef::Wall(_) => {
                    doc.walls.remove(id);
                    out.deleted.push(*id);
                }
            },
        }
        Ok(out)
    }
}

fn invalid<T>(msg: impl Into<String>) -> Result<T, ApiError> {
    Err(ApiError::Invalid(msg.into()))
}

fn positive(len: Length, what: &str) -> Result<(), ApiError> {
    if len.is_finite() && len.mm() > 0.0 {
        Ok(())
    } else {
        invalid(format!("{what} must be positive"))
    }
}

fn expect_kind(doc: &Document, id: Id, expected: ObjectKind) -> Result<(), ApiError> {
    match doc.kind_of(id) {
        None => Err(ApiError::NotFound(id)),
        Some(actual) if actual != expected => Err(ApiError::WrongKind {
            id,
            expected,
            actual,
        }),
        Some(_) => Ok(()),
    }
}

/// Resolve the floor and wall type for new walls, applying document defaults.
fn wall_context(
    doc: &Document,
    floor: Option<Id>,
    wall_type: Option<Id>,
    height: Option<Length>,
) -> Result<(Id, Id), ApiError> {
    let floor = match floor {
        Some(f) => {
            expect_kind(doc, f, ObjectKind::Floor)?;
            f
        }
        None => match doc.floors_by_elevation().first() {
            Some(f) => f.id,
            None => return invalid("the document has no floors; add one first"),
        },
    };
    let wall_type = match wall_type.or(doc.settings.default_wall_type) {
        Some(t) => {
            expect_kind(doc, t, ObjectKind::WallType)?;
            t
        }
        None => return invalid("no wall_type given and the document has no default wall type"),
    };
    if let Some(h) = height {
        positive(h, "height")?;
    }
    Ok((floor, wall_type))
}

fn check_wall_length(start: Point2, end: Point2) -> Result<(), ApiError> {
    let len = start.distance(end);
    if !len.is_finite() || len.mm() < 1.0 {
        return invalid("wall is shorter than 1 mm");
    }
    Ok(())
}

fn add_wall(
    doc: &mut Document,
    floor: Id,
    start: Point2,
    end: Point2,
    wall_type: Id,
    height: Option<Length>,
) -> Result<Id, ApiError> {
    check_wall_length(start, end)?;
    let id = doc.alloc_id();
    doc.walls.insert(
        id,
        Wall {
            id,
            floor,
            start,
            end,
            wall_type,
            height,
        },
    );
    Ok(id)
}
