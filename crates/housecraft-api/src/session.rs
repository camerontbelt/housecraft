use housecraft_model::Document;

use crate::{ApiError, Command, Outcome};

const UNDO_LIMIT: usize = 200;

/// An open document plus its edit history.
///
/// Undo currently snapshots the whole document per transaction. That is simple
/// and correct; switch to inverse commands once documents get large.
#[derive(Debug, Clone)]
pub struct Session {
    doc: Document,
    undo: Vec<Document>,
    redo: Vec<Document>,
    dirty: bool,
}

impl Session {
    pub fn new(doc: Document) -> Self {
        Session {
            doc,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
        }
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn into_document(self) -> Document {
        self.doc
    }

    /// True if there are edits since the session opened or [`Session::mark_saved`].
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Apply a batch as one transaction: all commands succeed or nothing changes.
    /// The batch is a single undo step.
    pub fn apply(&mut self, commands: &[Command]) -> Result<Outcome, ApiError> {
        let mut scratch = self.doc.clone();
        let mut outcome = Outcome::default();
        for (index, cmd) in commands.iter().enumerate() {
            let out = cmd.apply(&mut scratch).map_err(|e| ApiError::InCommand {
                index,
                op: cmd.op(),
                error: Box::new(e),
            })?;
            outcome.merge(out);
        }
        if commands.is_empty() {
            return Ok(outcome);
        }
        let previous = std::mem::replace(&mut self.doc, scratch);
        self.undo.push(previous);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        Ok(outcome)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Returns false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.doc, prev));
        self.dirty = true;
        true
    }

    /// Returns false when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.doc, next));
        self.dirty = true;
        true
    }
}
