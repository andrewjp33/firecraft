//! Undo/redo as document snapshots. Snapshots are cheap: bitmaps and the original file's data are
//! shared copy-on-write, so a step only stores what it changed.

use firecraft_doc::Document;

/// FireCraft's default number of undo steps (Fireworks MX defaults to 20). Snapshots share
/// unchanged bitmaps, so long histories stay cheap.
pub const DEFAULT_UNDO_STEPS: usize = 100;
/// Upper bound offered in Preferences.
pub const MAX_UNDO_STEPS: usize = 999;

#[derive(Clone, Debug)]
pub struct History {
    undo: Vec<(String, Document)>,
    redo: Vec<(String, Document)>,
    pub limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: DEFAULT_UNDO_STEPS,
        }
    }
}

impl History {
    /// Remember `before` as the state preceding the step `label`.
    pub fn push(&mut self, label: &str, before: &Document) {
        self.undo.push((label.to_owned(), before.clone()));
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Change the number of kept steps, dropping the oldest if needed.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.clamp(1, MAX_UNDO_STEPS);
        let excess = self.undo.len().saturating_sub(self.limit);
        self.undo.drain(..excess);
    }

    /// Drop the most recent undo step without applying it (an edit that changed nothing).
    pub fn discard_last(&mut self) {
        self.undo.pop();
    }

    pub fn undo(&mut self, current: &Document) -> Option<Document> {
        let (label, doc) = self.undo.pop()?;
        self.redo.push((label, current.clone()));
        Some(doc)
    }

    pub fn redo(&mut self, current: &Document) -> Option<Document> {
        let (label, doc) = self.redo.pop()?;
        self.undo.push((label, current.clone()));
        Some(doc)
    }

    /// Step labels for the History panel, oldest first.
    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.undo.iter().map(|(l, _)| l.as_str())
    }

    pub fn redo_labels(&self) -> impl Iterator<Item = &str> {
        self.redo.iter().rev().map(|(l, _)| l.as_str())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn next_undo(&self) -> Option<&str> {
        self.undo.last().map(|(l, _)| l.as_str())
    }
    pub fn next_redo(&self) -> Option<&str> {
        self.redo.last().map(|(l, _)| l.as_str())
    }
}
