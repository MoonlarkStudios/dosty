//! Documents kept open behind the page, each with its unsaved changes, undo
//! history, and recovery identity.

use std::path::Path;

use uuid::Uuid;

use super::recovery::Checkpoint;
use super::session::{Session, SessionId};
use crate::io::image_store::References;

/// Clean documents are a convenience and are bounded. Unsaved documents are the
/// writer's work and are kept until saved or explicitly discarded.
const MAX_CLEAN: usize = 8;
const MAX_CLEAN_SOURCE_BYTES: usize = 32 * 1024 * 1024;

/// A document that left the page, with the per-document state that `App`
/// otherwise holds for the page.
pub(super) struct Parked {
    pub(super) session: Session,
    pub(super) references: References,
    pub(super) checkpoint: Checkpoint,
}

#[derive(Default)]
pub(super) struct Background {
    /// Least recently used first.
    parked: Vec<Parked>,
}

impl Background {
    /// Keeps `parked` and returns the clean documents released to stay within
    /// bounds, least recently used first.
    pub(super) fn park(&mut self, parked: Parked) -> Vec<Parked> {
        self.parked.push(parked);
        let mut released = Vec::new();
        while self.clean_over_bounds() {
            let Some(oldest) = self.parked.iter().position(|parked| !parked.session.dirty) else {
                break;
            };
            released.push(self.parked.remove(oldest));
        }
        released
    }

    fn clean_over_bounds(&self) -> bool {
        let clean = self.parked.iter().filter(|parked| !parked.session.dirty);
        let (count, bytes) = clean.fold((0, 0), |(count, bytes), parked| {
            (count + 1, bytes + parked.session.document.source().len())
        });
        count > MAX_CLEAN || bytes > MAX_CLEAN_SOURCE_BYTES
    }

    /// Takes the session `id` if it has unsaved changes. A clean session is
    /// reached through its file instead, which confirms the file is unchanged.
    pub(super) fn take_unsaved(&mut self, id: SessionId) -> Option<Parked> {
        self.take_last(|session| session.dirty && session.id() == id)
    }

    fn take_last(&mut self, matches: impl Fn(&Session) -> bool) -> Option<Parked> {
        let index = self
            .parked
            .iter()
            .rposition(|parked| matches(&parked.session))?;
        Some(self.parked.remove(index))
    }

    /// Takes the most recent unsaved document, optionally only one saved at `path`.
    pub(super) fn take_dirty(&mut self, path: Option<&Path>) -> Option<Parked> {
        self.take_last(|session| {
            session.dirty && path.is_none_or(|path| session.path.as_deref() == Some(path))
        })
    }

    /// Takes the clean copy of `path` if the file still holds what it held when
    /// that copy left the page, so its undo history can return with it.
    pub(super) fn take_unchanged(
        &mut self,
        path: &Path,
        source: &str,
        version: Option<&str>,
    ) -> Option<Parked> {
        self.take_last(|session| {
            !session.dirty
                && session.path.as_deref() == Some(path)
                && session.saved_source == source
                && session.saved_version.as_deref() == version
        })
    }

    /// Removes clean documents saved at or below `root`. Their files moved or
    /// were replaced, so they would only describe a path that no longer holds them.
    pub(super) fn release_clean_under(&mut self, root: &Path) -> Vec<Parked> {
        let (released, kept) = std::mem::take(&mut self.parked)
            .into_iter()
            .partition(|parked| !parked.session.dirty && is_under(&parked.session, root));
        self.parked = kept;
        released
    }

    /// The most recent unsaved document saved at or below `root`.
    pub(super) fn dirty_under(&self, root: &Path) -> Option<&Session> {
        self.dirty().find(|session| is_under(session, root))
    }

    /// Unsaved documents, most recently used first.
    pub(super) fn dirty(&self) -> impl Iterator<Item = &Session> {
        self.parked
            .iter()
            .rev()
            .map(|parked| &parked.session)
            .filter(|session| session.dirty)
    }

    /// Removes every unsaved document. Only an explicit discard may call this.
    pub(super) fn discard_dirty(&mut self) -> Vec<Parked> {
        let (discarded, kept) = std::mem::take(&mut self.parked)
            .into_iter()
            .partition(|parked| parked.session.dirty);
        self.parked = kept;
        discarded
    }

    /// An unsaved document whose recovery copy does not yet hold its latest writing.
    pub(super) fn needing_checkpoint(&self) -> Option<&Parked> {
        self.parked.iter().find(|parked| {
            parked.session.dirty
                && parked
                    .checkpoint
                    .needs_write(parked.session.document.collaboration().generation())
        })
    }

    pub(super) fn checkpoint_mut(&mut self, id: Uuid) -> Option<&mut Checkpoint> {
        self.parked
            .iter_mut()
            .map(|parked| &mut parked.checkpoint)
            .find(|checkpoint| checkpoint.id() == id)
    }
}

fn is_under(session: &Session, root: &Path) -> bool {
    session
        .path
        .as_deref()
        .is_some_and(|path| path.starts_with(root))
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
