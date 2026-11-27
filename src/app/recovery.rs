//! Debounced recovery scheduling. Late writes from a retired document are removed
//! after completion, so saving/discarding cannot resurrect an obsolete draft.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Task};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::session::Session;
use super::{App, Message as AppMessage, Overlay, Pending};
use crate::document::Document;
use crate::io::{recovery, storage};

type Version = (u64, u64);
use crate::theme::styles;

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Available(Result<bool, storage::FileError>),
    OpenFolder,
    Show,
    Listed(Result<Vec<recovery::Entry>, storage::FileError>),
    Restore(Uuid),
    Loaded(Result<recovery::Snapshot, storage::FileError>),
    KeepAndClose,
    Kept(iced::window::Id, Result<(), storage::FileError>),
    Written {
        id: Uuid,
        generation: Version,
        result: Result<(), storage::FileError>,
    },
    Removed(Result<(), storage::FileError>),
}
pub(super) struct State {
    id: Uuid,
    directory: Option<std::path::PathBuf>,
    edited: Instant,
    last_checkpoint: Instant,
    saved: Option<Version>,
    inflight: Option<Uuid>,
    retired: HashSet<Uuid>,
    prior_paths: Vec<Uuid>,
    cleanup: Vec<Uuid>,
    entries: Vec<recovery::Entry>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            directory: if cfg!(test) { None } else { root() },
            edited: Instant::now(),
            last_checkpoint: Instant::now(),
            saved: None,
            inflight: None,
            retired: HashSet::new(),
            prior_paths: Vec::new(),
            cleanup: Vec::new(),
            entries: Vec::new(),
        }
    }
}
/// The recovery identity of a document kept open behind the page.
///
/// A parked document no longer changes, so it carries only which copy is its
/// own and which generation that copy holds.
pub(super) struct Checkpoint {
    id: Uuid,
    saved: Option<Version>,
    prior_paths: Vec<Uuid>,
}
impl Checkpoint {
    pub(super) fn id(&self) -> Uuid {
        self.id
    }
    pub(super) fn needs_write(&self, generation: Version) -> bool {
        self.saved != Some(generation)
    }
    /// Records a completed write and returns the superseded copies to remove.
    fn written(&mut self, generation: Version) -> Vec<Uuid> {
        self.saved = Some(generation);
        std::mem::take(&mut self.prior_paths)
    }
}
impl State {
    /// Detaches the current document's identity so its recovery copy outlives
    /// its time on the page, and starts a fresh identity for the next document.
    pub(super) fn park(&mut self) -> Checkpoint {
        let checkpoint = Checkpoint {
            id: std::mem::replace(&mut self.id, Uuid::new_v4()),
            saved: self.saved.take(),
            prior_paths: std::mem::take(&mut self.prior_paths),
        };
        self.edited = Instant::now();
        self.last_checkpoint = self.edited;
        checkpoint
    }
    /// Adopts a parked identity for the document taking the page. The identity
    /// it replaces must already have been parked or retired.
    pub(super) fn resume(&mut self, checkpoint: Checkpoint) {
        self.id = checkpoint.id;
        self.saved = checkpoint.saved;
        self.prior_paths = checkpoint.prior_paths;
        self.edited = Instant::now();
        self.last_checkpoint = self.edited;
    }
    /// Removes the copies of a parked document that was saved, released, or
    /// explicitly discarded.
    pub(super) fn discard(&mut self, checkpoint: Checkpoint) {
        let mut ids = checkpoint.prior_paths;
        ids.push(checkpoint.id);
        self.release(ids);
    }
    /// Removes copies now, or after the write still in flight for one of them.
    fn release(&mut self, ids: Vec<Uuid>) {
        for id in ids {
            if self.inflight == Some(id) {
                self.retired.insert(id);
            } else {
                self.cleanup.push(id);
            }
        }
    }
    pub(super) fn path_changed(&mut self) {
        // Keep the previous recovery copy until its replacement is published,
        // but refresh path metadata even if the source generation is unchanged.
        let previous = std::mem::replace(&mut self.id, Uuid::new_v4());
        self.prior_paths.push(previous);
        self.saved = None;
        self.edited = Instant::now();
    }

    pub fn needs_checkpoint(&self, generation: Version) -> bool {
        self.saved != Some(generation) && self.inflight.is_none()
    }
    fn checkpoint_due(&self, generation: Version, now: Instant) -> bool {
        self.needs_checkpoint(generation)
            && (now.saturating_duration_since(self.edited) >= Duration::from_secs(2)
                || now.saturating_duration_since(self.last_checkpoint) >= Duration::from_secs(30))
    }
    pub fn edited(&mut self) {
        self.edited = Instant::now();
    }
    pub fn retire(&mut self) {
        let id = std::mem::replace(&mut self.id, Uuid::new_v4());
        self.saved = None;
        self.edited = Instant::now();
        self.last_checkpoint = self.edited;
        let mut retired = std::mem::take(&mut self.prior_paths);
        retired.push(id);
        self.release(retired);
    }
    fn written(&mut self, id: Uuid, generation: Version, success: bool) {
        if self.inflight == Some(id) {
            self.inflight = None;
        }
        if self.retired.remove(&id) {
            self.cleanup.push(id);
        } else if id == self.id && success {
            self.saved = Some(generation);
            self.cleanup.append(&mut self.prior_paths);
        }
    }
}
fn snapshot(session: &Session) -> recovery::Snapshot {
    recovery::Snapshot {
        state: session.document.collaboration().state(),
        path: session.path().cloned(),
        name: session.display_name.clone(),
        baseline: session.saved_source().to_owned(),
        baseline_version: session.saved_version().cloned(),
        hosted: session.hosted().cloned().map(Box::new),
        anchor: session.selection.anchor,
        head: session.selection.head,
    }
}
fn root() -> Option<std::path::PathBuf> {
    storage::preferences_path().and_then(|path| path.parent().map(|p| p.join("recovery")))
}
impl App {
    pub(super) fn can_keep_recovery_on_close(&self) -> bool {
        !self.busy && self.recovery.inflight.is_none() && self.recovery.directory.is_some()
    }

    #[cfg(test)]
    pub(super) fn use_recovery_directory(&mut self, directory: std::path::PathBuf) {
        self.recovery.directory = Some(directory);
    }

    pub(super) fn recovery_discovery(&self) -> Task<AppMessage> {
        let Some(root) = self.recovery.directory.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    recovery::list(&root).map(|entries| !entries.is_empty())
                })
                .await
                .map_err(|e| storage::FileError::Io(e.to_string()))?
            },
            |result| AppMessage::Recovery(Message::Available(result)),
        )
    }
    pub(super) fn recovery_cleanup(&mut self) -> Task<AppMessage> {
        let ids = std::mem::take(&mut self.recovery.cleanup);
        if ids.is_empty() {
            return Task::none();
        }
        let Some(root) = self.recovery.directory.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    for id in ids {
                        recovery::remove(&root, id)?;
                    }
                    Ok(())
                })
                .await
                .map_err(|e| storage::FileError::Io(e.to_string()))?
            },
            |result| AppMessage::Recovery(Message::Removed(result)),
        )
    }
    pub(super) fn update_recovery(&mut self, message: Message) -> Task<AppMessage> {
        let Some(root) = self.recovery.directory.clone() else {
            return Task::none();
        };
        match message {
            Message::Available(result) => match result {
                Ok(true) if self.notification.is_none() => {
                    self.notification =
                        Some("Recovery copies are available in File → Help & settings.".into())
                }
                Err(error) => tracing::warn!(%error, "Could not list recovery copies"),
                _ => {}
            },
            Message::OpenFolder => {
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                            open::that_detached(root).map_err(|e| e.to_string())
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    AppMessage::LinkOpened,
                );
            }
            Message::Tick => {
                if self.busy && self.overlay == Some(Overlay::Unsaved) {
                    return Task::none();
                }
                // One write runs at a time. A document that just left the page
                // goes first: it has no later tick of its own to rely on.
                let parked = self
                    .recovery
                    .inflight
                    .is_none()
                    .then(|| self.background.needing_checkpoint())
                    .flatten()
                    .map(|parked| {
                        (
                            parked.checkpoint.id(),
                            parked.session.recovery_version(),
                            snapshot(&parked.session),
                        )
                    });
                let write = parked.or_else(|| {
                    let generation = self.session.recovery_version();
                    let due = self.session.is_dirty()
                        && !self.loading
                        && self.recovery.checkpoint_due(generation, Instant::now());
                    due.then(|| {
                        self.recovery.last_checkpoint = Instant::now();
                        (self.recovery.id, generation, snapshot(&self.session))
                    })
                });
                if let Some((id, generation, snapshot)) = write {
                    self.recovery.inflight = Some(id);
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                recovery::write(&root, id, &snapshot)
                            })
                            .await
                            .map_err(|e| storage::FileError::Io(e.to_string()))?
                        },
                        move |result| {
                            AppMessage::Recovery(Message::Written {
                                id,
                                generation,
                                result,
                            })
                        },
                    );
                }
            }
            Message::Written {
                id,
                generation,
                result,
            } => {
                self.recovery.written(id, generation, result.is_ok());
                if result.is_ok()
                    && let Some(checkpoint) = self.background.checkpoint_mut(id)
                {
                    let superseded = checkpoint.written(generation);
                    self.recovery.cleanup.extend(superseded);
                }
                if let Err(error) = result {
                    self.notification = Some(format!("Recovery copy could not be saved: {error}"));
                }
            }
            Message::KeepAndClose if self.can_keep_recovery_on_close() => {
                let Some(Pending::Close(window)) = self.pending else {
                    return Task::none();
                };
                if self.overlay != Some(Overlay::Unsaved) || !self.commit_cell() {
                    return Task::none();
                }
                let copies = self
                    .session
                    .is_dirty()
                    .then(|| (self.recovery.id, snapshot(&self.session)))
                    .into_iter()
                    .chain(
                        self.background
                            .dirty_checkpoints()
                            .map(|parked| (parked.checkpoint.id(), snapshot(&parked.session))),
                    )
                    .collect::<Vec<_>>();
                self.busy = true;
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            for (id, snapshot) in copies {
                                recovery::write(&root, id, &snapshot)?;
                            }
                            Ok(())
                        })
                        .await
                        .map_err(|error| storage::FileError::Io(error.to_string()))?
                    },
                    move |result| AppMessage::Recovery(Message::Kept(window, result)),
                );
            }
            Message::KeepAndClose => {}
            Message::Kept(window, result) => {
                self.busy = false;
                match result {
                    Ok(()) => {
                        self.pending = None;
                        return self.finish_close(window);
                    }
                    Err(error) => {
                        self.chrome.save_error =
                            Some(format!("Recovery copy could not be saved: {error}"));
                    }
                }
            }
            Message::Show => {
                if self.overlay.is_some() || self.loading {
                    return Task::none();
                }
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || recovery::list(&root))
                            .await
                            .map_err(|e| storage::FileError::Io(e.to_string()))?
                    },
                    |result| AppMessage::Recovery(Message::Listed(result)),
                );
            }
            Message::Listed(result) => match result {
                Ok(entries) if self.overlay.is_none() => {
                    self.recovery.entries = entries;
                    self.overlay = Some(Overlay::Recovery);
                }
                Err(error) => self.notification = Some(error.to_string()),
                _ => {}
            },
            Message::Restore(id) => {
                if !matches!(self.overlay, Some(Overlay::Recovery)) || self.busy {
                    return Task::none();
                }
                self.busy = true;
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || recovery::read(&root, id))
                            .await
                            .map_err(|e| storage::FileError::Io(e.to_string()))?
                    },
                    |result| AppMessage::Recovery(Message::Loaded(result)),
                );
            }
            Message::Loaded(result) => {
                self.busy = false;
                if !matches!(self.overlay, Some(Overlay::Recovery)) {
                    return Task::none();
                }
                match result {
                    Ok(snapshot) => {
                        self.overlay = None;
                        return self.request(Pending::Recover(Box::new(snapshot)));
                    }
                    Err(error) => self.notification = Some(error.to_string()),
                }
            }
            Message::Removed(result) => {
                if let Err(error) = result {
                    tracing::warn!(%error, "Could not remove retired recovery copy");
                }
            }
        }
        Task::none()
    }
    pub(super) fn restore_draft(&mut self, snapshot: recovery::Snapshot) -> Task<AppMessage> {
        match Document::from_state(&snapshot.state) {
            Ok(document) => {
                let session = Session::recovered(document, snapshot);
                let selection = session.selection;
                self.install_session(session);
                self.session.selection = selection;
                self.sync_editor();
                self.notification = Some(if self.session.hosted().is_some() {
                    "Hosted draft recovered. Save checks its last saved workspace revision."
                } else {
                    "Recovery copy opened. Save checks the last disk version; Save As keeps both. The original recovery copy is retained."
                }.into());
            }
            Err(error) => self.notification = Some(error.to_string()),
        }
        iced::widget::operation::focus(super::EDITOR)
    }
    pub(super) fn recovery_view(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let mut entries = column![].spacing(10);
        for entry in &self.recovery.entries {
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(entry.modified);
            let age = if seconds < 60 {
                "Just now".into()
            } else if seconds < 3600 {
                format!("{} min ago", seconds / 60)
            } else if seconds < 86400 {
                format!("{} h ago", seconds / 3600)
            } else {
                format!("{} d ago", seconds / 86400)
            };
            entries = entries.push(
                row![
                    column![
                        text(&entry.name).size(15),
                        text(age).size(11).color(tokens.muted)
                    ]
                    .spacing(4)
                    .width(iced::Fill),
                    self.small_button(
                        "Open copy",
                        AppMessage::Recovery(Message::Restore(entry.id)),
                        false
                    )
                ]
                .spacing(12),
            );
        }
        if self.recovery.entries.is_empty() {
            entries = entries.push(
                text("No recovery copies are available.")
                    .size(14)
                    .color(tokens.muted),
            );
        }
        let content = column![
            text("Recovery copies").font(styles::BODY_FONT).size(28),
            text("Unsaved writing is checkpointed after two idle seconds, or every 30 seconds while you keep writing. Open a copy to inspect it; your Markdown is never overwritten automatically. Earlier copies are retained in the recovery folder.").size(13).color(tokens.muted),
            scrollable(entries).height(iced::Fill).style(styles::scrollbar(tokens)),
            row![self.small_button("Show folder", AppMessage::Recovery(Message::OpenFolder), false), self.small_button("Close", AppMessage::CloseOverlay, false)].spacing(8)
        ].spacing(20).height(iced::Fill);
        container(content)
            .width(560)
            .height(540)
            .padding(28)
            .style(styles::card(tokens))
            .into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::selection::Selection;
    #[test]
    fn old_path_write_cannot_suppress_or_remove_new_path_recovery() {
        let mut state = State::default();
        let previous = state.id;
        state.saved = Some((4, 0));
        state.inflight = Some(previous);
        state.path_changed();
        let current = state.id;
        assert_ne!(previous, current);
        assert!(state.cleanup.is_empty());
        state.written(previous, (4, 0), true);
        assert_eq!(state.saved, None);
        assert!(state.needs_checkpoint((4, 0)));
        assert!(
            state.cleanup.is_empty(),
            "old copy survives until replacement"
        );
        state.inflight = Some(current);
        state.written(current, (4, 0), false);
        assert!(state.cleanup.is_empty());
        state.inflight = Some(current);
        state.written(current, (4, 0), true);
        assert_eq!(state.saved, Some((4, 0)));
        assert_eq!(state.cleanup, vec![previous]);
    }

    #[test]
    fn explicit_save_retires_old_path_copy_after_its_inflight_write() {
        let mut state = State::default();
        let old = state.id;
        state.inflight = Some(old);
        state.path_changed();
        state.retire();
        assert!(state.retired.contains(&old));
        assert!(!state.cleanup.contains(&old));
        state.written(old, (8, 0), true);
        assert!(state.cleanup.contains(&old));
        assert_eq!(state.saved, None);
    }
    #[test]
    fn late_write_cannot_resurrect_saved_or_discarded_document() {
        let mut state = State::default();
        let old = state.id;
        state.inflight = Some(old);
        state.retire();
        assert!(state.cleanup.is_empty());
        assert_ne!(old, state.id);
        state.written(old, (4, 0), true);
        assert_eq!(state.cleanup, vec![old]);
        assert_eq!(state.saved, None);
        state.written(state.id, (9, 0), true);
        assert_eq!(state.saved, Some((9, 0)));
        assert!(!state.needs_checkpoint((9, 0)));
        assert!(state.needs_checkpoint((10, 0)));
        let start = Instant::now();
        state.last_checkpoint = start;
        state.edited = start;
        assert!(!state.checkpoint_due((10, 0), start + Duration::from_secs(1)));
        assert!(state.checkpoint_due((10, 0), start + Duration::from_secs(2)));
        state.edited = start + Duration::from_secs(29);
        assert!(
            state.checkpoint_due((10, 0), start + Duration::from_secs(30)),
            "continuous typing must still be checkpointed"
        );
    }
    #[test]
    fn a_parked_identity_keeps_its_copy_until_resumed_or_discarded() {
        let mut state = State::default();
        let parked_id = state.id;
        state.saved = Some((3, 0));
        let mut checkpoint = state.park();
        assert_ne!(state.id, parked_id);
        assert_eq!(state.saved, None);
        assert!(state.cleanup.is_empty(), "parking removes nothing");
        assert_eq!(checkpoint.id(), parked_id);
        assert!(!checkpoint.needs_write((3, 0)));
        assert!(checkpoint.needs_write((4, 0)));
        assert!(checkpoint.written((4, 0)).is_empty());
        assert!(!checkpoint.needs_write((4, 0)));

        state.retire();
        state.cleanup.clear();
        state.resume(checkpoint);
        assert_eq!(state.id, parked_id);
        assert!(!state.needs_checkpoint((4, 0)));

        // A write still in flight must finish before its copy is removed.
        state.inflight = Some(parked_id);
        let checkpoint = state.park();
        state.discard(checkpoint);
        assert!(state.cleanup.is_empty());
        state.written(parked_id, (4, 0), true);
        assert_eq!(state.cleanup, vec![parked_id]);
    }

    #[tokio::test]
    async fn restored_draft_keeps_its_disk_conflict_guard_and_fresh_replica_identity() {
        use crate::io::collaboration_store;
        use iced::widget::text_editor::{Action, Edit};
        use std::sync::Arc;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        let (mut app, _) = App::new();
        app.set_document(
            "# Saved\r\n\r\nOriginal café.\r\n".into(),
            Some(path.clone()),
            "note.md".into(),
        );
        let state = app.session.document.collaboration().state();
        app.session.acknowledge_file_save(
            path.clone(),
            "note.md".into(),
            Some(collaboration_store::version(&state)),
            app.session.saved_source().to_owned(),
            app.session.document.collaboration().generation(),
        );
        collaboration_store::save_document(
            path.clone(),
            app.session.document.source().into(),
            state,
            None,
            None,
        )
        .await
        .unwrap();
        let selection = Selection {
            anchor: app.session.document.source().len(),
            head: 2,
        };
        app.session.selection = selection;
        app.sync_editor();
        drop(
            app.update(AppMessage::Edit(Action::Edit(Edit::Paste(Arc::new(
                "Draft 海 👩‍💻".into(),
            ))))),
        );
        let snapshot = recovery::Snapshot {
            state: app.session.document.collaboration().state(),
            path: app.session.path().cloned(),
            name: app.session.display_name.clone(),
            baseline: app.session.saved_source().to_owned(),
            baseline_version: app.session.saved_version().cloned(),
            hosted: None,
            anchor: 10,
            head: 2,
        };
        let expected = app.session.document.source().to_owned();
        let writer = app.session.document.collaboration().replica_id();
        let id = Uuid::new_v4();
        recovery::write(dir.path(), id, &snapshot).unwrap();
        std::fs::write(&path, "External work must survive.").unwrap();
        drop(app.restore_draft(recovery::read(dir.path(), id).unwrap()));
        assert_eq!(app.session.document.source(), expected);
        assert_eq!(
            app.session.document.source(),
            app.session.document.collaboration().materialize()
        );
        assert_eq!(app.editor.snapshot(), app.projection.text);
        assert!(app.session.selection.anchor > app.session.selection.head);
        assert_ne!(app.session.document.collaboration().replica_id(), writer);
        assert!(app.session.is_dirty());
        let result = collaboration_store::save_document(
            path.clone(),
            app.session.document.source().into(),
            app.session.document.collaboration().state(),
            Some(app.session.saved_source().to_owned()),
            app.session.saved_version().cloned(),
        )
        .await;
        assert!(matches!(result, Err(storage::FileError::Conflict)));
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "External work must survive."
        );
    }
}
