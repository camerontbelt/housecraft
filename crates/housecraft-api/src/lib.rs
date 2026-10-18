//! The one API every HouseCraft front-end (GUI, CLI, MCP, scripts) goes through.
//!
//! Edits are serializable [`Command`]s applied atomically by a [`Session`],
//! which also owns undo/redo. Read access goes through [`query`].

mod command;
pub mod query;
mod session;

pub use command::{Command, Outcome};
pub use housecraft_model as model;
pub use housecraft_units as units;
pub use session::Session;

use housecraft_model::{Id, ObjectKind};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ApiError {
    #[error("{0} not found")]
    NotFound(Id),
    #[error("{id} is a {actual}, expected a {expected}")]
    WrongKind {
        id: Id,
        expected: ObjectKind,
        actual: ObjectKind,
    },
    #[error("{0}")]
    Invalid(String),
    #[error("command {index} ({op}) failed: {source}")]
    InCommand {
        index: usize,
        op: &'static str,
        #[source]
        source: Box<ApiError>,
    },
}

/// JSON Schema for a batch of commands (an array of [`Command`]).
pub fn command_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Vec<Command>)).expect("schema serializes")
}
