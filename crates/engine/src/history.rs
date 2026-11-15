//! Undo/redo as document snapshots. Snapshots are cheap: bitmaps and the original file's data are
//! shared copy-on-write, so a step only stores what it changed.

use std::sync::Arc;

use firecraft_doc::Document;

use crate::{ElementRef, PixelSelection};

/// What was selected at an undo step. Undo brings back the pixel selection and the bitmap it
/// applies to (Fireworks MX: undoing a Delete keeps the marquee; undoing "Edit Bitmap" goes back
/// to the bitmap edited before).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selected {
    pub objects: Vec<ElementRef>,
    /// Shared between steps while it doesn't change (a mask can be the canvas' size).
    pub pixels: Option<Arc<PixelSelection>>,
}

/// One undo step: its label and the document and selection before it.
type Step = (String, Document, Selected);

/// FireCraft's default number of undo steps (Fireworks MX defaults to 20). Snapshots share
/// unchanged bitmaps, so long histories stay cheap.
pub const DEFAULT_UNDO_STEPS: usize = 100;
/// Upper bound offered in Preferences.
pub const MAX_UNDO_STEPS: usize = 999;

#[derive(Clone, Debug)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
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
    /// The selection to keep with a step, sharing the mask with the newest step when it is
    /// the same selection.
    pub fn selected(&self, objects: &[ElementRef], pixels: Option<&PixelSelection>) -> Selected {
        let newest = self.undo.last().or(self.redo.last()).and_then(|(_, _, s)| s.pixels.as_ref());
        let pixels = pixels.map(|p| match newest {
            Some(a) if **a == *p => a.clone(),
            _ => Arc::new(p.clone()),
        });
        Selected {
            objects: objects.to_vec(),
            pixels,
        }
    }

    /// Remember `before` (and what was selected) as the state preceding the step `label`.
    pub fn push(&mut self, label: &str, before: &Document, selected: Selected) {
        self.undo.push((label.to_owned(), before.clone(), selected));
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

    /// Remove the most recent undo step and return its document (cancelling a live edit).
    pub fn take_last(&mut self) -> Option<Document> {
        self.undo.pop().map(|(_, d, _)| d)
    }

    /// Step back: the document and selection before the newest step (`current` goes to redo).
    pub fn undo(&mut self, current: &Document, selected: Selected) -> Option<(Document, Selected)> {
        let (label, doc, sel) = self.undo.pop()?;
        self.redo.push((label, current.clone(), selected));
        Some((doc, sel))
    }

    pub fn redo(&mut self, current: &Document, selected: Selected) -> Option<(Document, Selected)> {
        let (label, doc, sel) = self.redo.pop()?;
        self.undo.push((label, current.clone(), selected));
        Some((doc, sel))
    }

    /// Step labels for the History panel, oldest first.
    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.undo.iter().map(|(l, _, _)| l.as_str())
    }

    pub fn redo_labels(&self) -> impl Iterator<Item = &str> {
        self.redo.iter().rev().map(|(l, _, _)| l.as_str())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn next_undo(&self) -> Option<&str> {
        self.undo.last().map(|(l, _, _)| l.as_str())
    }
    pub fn next_redo(&self) -> Option<&str> {
        self.redo.last().map(|(l, _, _)| l.as_str())
    }
}
