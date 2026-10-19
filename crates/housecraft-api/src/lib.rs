//! The one API every HouseCraft front-end (GUI, CLI, MCP, scripts) goes through.
//!
//! Edits are serializable [`Command`]s applied atomically by a [`Session`],
//! which also owns undo/redo, keeps rooms in sync with walls and validates
//! the document after every transaction. Read access goes through [`query`].

pub mod blocks;
mod cabinets;
mod command;
mod paint;
mod foundation;
pub mod layouts;
pub mod materials;
mod ops;
pub mod query;
mod roofs;
mod rooms;
pub mod schedule;
mod session;
mod site;
mod solids;
mod symbols;
mod views;

pub use command::{Command, DetailText, PaintScope, LengthAnchor, Outcome, ShapeOp, TransformOp};
pub use housecraft_geom as geom;
pub use housecraft_model as model;
pub use housecraft_units as units;
pub use session::Session;

use housecraft_model::{Id, ObjectKind};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ApiError {
    #[error("{0} not found")]
    NotFound(Id),
    #[error("{id} is a {actual}, expected {expected}")]
    WrongKind {
        id: Id,
        expected: String,
        actual: ObjectKind,
    },
    #[error("{0}")]
    Invalid(String),
    /// The message already includes `error`, so it is deliberately not an error
    /// `source` (that would print it twice in error chains).
    #[error("command {index} ({op}) failed: {error}")]
    InCommand {
        index: usize,
        op: &'static str,
        error: Box<ApiError>,
    },
}

/// JSON Schema for a batch of commands (an array of [`Command`]).
pub fn command_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Vec<Command>)).expect("schema serializes")
}
