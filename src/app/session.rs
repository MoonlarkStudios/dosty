//! The durable state of one open document: its text, where it is saved, and how
//! it differs from the last saved copy.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::document::Document;
use crate::document::selection::Selection;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Identifies one session for as long as the application runs. Reloading a
/// file starts a new session with a new identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SessionId(u64);

/// One open document and its relationship to the file it was loaded from.
///
/// Everything here must survive when another document takes the page. View
/// state derived from it (the projection, the editor buffer, table drafts)
/// belongs to `App` and is rebuilt when a session is installed.
pub(super) struct Session {
    id: SessionId,
    pub(super) document: Document,
    /// UTF-8 byte offsets into the document source, with anchor/head direction.
    pub(super) selection: Selection,
    /// `None` until the document is first saved.
    pub(super) path: Option<PathBuf>,
    pub(super) display_name: String,
    /// Source last read from or written to `path`. Saving checks the file
    /// against it to detect external changes.
    pub(super) saved_source: String,
    /// Checkpoint version paired with `saved_source`, when one exists.
    pub(super) saved_version: Option<String>,
    /// Collaboration generation at the last save. A document whose source
    /// matches the baseline is still modified if its history has moved on.
    saved_generation: u64,
    pub(super) dirty: bool,
}

impl Session {
    /// Starts a clean session whose saved baseline is the document's own source.
    pub(super) fn new(document: Document, path: Option<PathBuf>, display_name: String) -> Self {
        let saved_source = document.source().to_owned();
        Self {
            id: SessionId(NEXT_ID.fetch_add(1, Ordering::Relaxed)),
            document,
            selection: Selection::default(),
            path,
            display_name,
            saved_source,
            saved_version: None,
            saved_generation: 0,
            dirty: false,
        }
    }

    pub(super) fn id(&self) -> SessionId {
        self.id
    }

    /// Recomputes `dirty` by comparing the document with its saved baseline.
    pub(super) fn refresh_dirty(&mut self) {
        self.dirty = self.document.source() != self.saved_source
            || self.document.collaboration().generation() != self.saved_generation;
    }

    /// Records a completed save of `source` as the new baseline.
    ///
    /// The save captured a snapshot, so writing that continued while it ran
    /// leaves the session dirty.
    pub(super) fn mark_saved(&mut self, source: String, version: String, generation: u64) {
        self.saved_source = source;
        self.saved_version = Some(version);
        self.saved_generation = generation;
        self.refresh_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::Session;
    use crate::document::Document;
    use crate::document::selection::Selection;

    fn session(source: &str) -> Session {
        Session::new(Document::new(source.into()), None, "Test.md".into())
    }

    fn append(session: &mut Session, text: &str) {
        let end = session.document.source().len();
        let before = Selection {
            anchor: end,
            head: end,
        };
        let after = Selection {
            anchor: end + text.len(),
            head: end + text.len(),
        };
        session
            .document
            .replace_edit(end..end, text, before, after)
            .unwrap();
    }

    #[test]
    fn a_new_session_is_clean_against_its_own_source() {
        let mut session = session("Quiet.");
        assert!(!session.dirty);
        assert_eq!(session.saved_source, "Quiet.");
        assert_eq!(session.saved_version, None);

        session.refresh_dirty();
        assert!(!session.dirty);
    }

    #[test]
    fn edits_make_a_session_dirty_until_that_snapshot_is_saved() {
        let mut session = session("Quiet.");
        append(&mut session, " Still.");
        session.refresh_dirty();
        assert!(session.dirty);

        let source = session.document.source().to_owned();
        let generation = session.document.collaboration().generation();
        session.mark_saved(source, "v1".into(), generation);
        assert!(!session.dirty);
        assert_eq!(session.saved_source, "Quiet. Still.");
        assert_eq!(session.saved_version.as_deref(), Some("v1"));
    }

    #[test]
    fn writing_during_a_save_keeps_the_session_dirty() {
        let mut session = session("Quiet.");
        let source = session.document.source().to_owned();
        let generation = session.document.collaboration().generation();
        append(&mut session, " Later.");

        session.mark_saved(source, "v1".into(), generation);
        assert!(session.dirty);
        assert_eq!(session.saved_source, "Quiet.");
    }

    #[test]
    fn undone_writing_is_dirty_even_though_the_source_matches_the_baseline() {
        let mut session = session("Quiet.");
        append(&mut session, " Still.");
        session.document.undo();
        assert_eq!(session.document.source(), session.saved_source);

        session.refresh_dirty();
        assert!(session.dirty);
    }
}
