//! Background documents: bounds, and the application flows that park, resume,
//! and close them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::widget::text_editor::{Action, Edit};
use iced::window;

use super::super::recovery;
use super::super::trail::Direction;
use super::super::{App, Message, Overlay, Pending, unsaved_summary};
use super::{Background, MAX_CLEAN, Parked};
use crate::app::application::session::Session;
use crate::document::Document;
use crate::io::collaboration_store;
use crate::io::image_store::References;
use crate::io::storage::LoadedFile;

fn parked(path: Option<&str>, dirty: bool) -> Parked {
    let mut session = Session::new(
        Document::new("text".into()),
        path.map(PathBuf::from),
        path.unwrap_or("Untitled.md").into(),
    );
    session.dirty = dirty;
    Parked {
        session,
        references: References::default(),
        checkpoint: recovery::State::default().park(),
    }
}

fn paths(parked: &[Parked]) -> Vec<&Path> {
    parked
        .iter()
        .filter_map(|parked| parked.session.path().map(|value| value.as_path()))
        .collect()
}

#[test]
fn only_clean_documents_are_released_and_the_least_recent_go_first() {
    let mut background = Background::default();
    assert!(
        background
            .park(parked(Some("/unsaved-first.md"), true))
            .is_empty()
    );
    for index in 0..MAX_CLEAN {
        let path = format!("/clean-{index}.md");
        assert!(background.park(parked(Some(&path), false)).is_empty());
    }
    assert!(background.park(parked(None, true)).is_empty());

    let released = background.park(parked(Some("/one-more.md"), false));
    assert_eq!(paths(&released), [Path::new("/clean-0.md")]);
    let released = background.park(parked(Some("/and-another.md"), false));
    assert_eq!(paths(&released), [Path::new("/clean-1.md")]);
    assert_eq!(background.dirty().count(), 2);
}

#[test]
fn a_clean_copy_returns_only_while_its_file_is_unchanged() {
    let mut background = Background::default();
    let mut clean = parked(Some("/note.md"), false);
    clean.session.set_saved_version(Some("v1".into()));
    background.park(clean);
    background.park(parked(Some("/note.md"), true));
    let path = Path::new("/note.md");

    assert!(
        background
            .take_unchanged(path, "edited elsewhere", Some("v1"))
            .is_none()
    );
    assert!(
        background
            .take_unchanged(path, "text", Some("v2"))
            .is_none()
    );
    assert!(background.take_unchanged(path, "text", None).is_none());
    let returned = background.take_unchanged(path, "text", Some("v1")).unwrap();
    assert!(!returned.session.dirty);
    assert!(background.take_dirty(Some(path)).is_some());
    assert!(background.take_dirty(None).is_none());
}

#[test]
fn moved_paths_release_clean_copies_below_them_and_report_unsaved_ones() {
    let mut background = Background::default();
    for (path, dirty) in [
        ("/notes/a.md", false),
        ("/notes/deep/b.md", false),
        ("/notes/deep/c.md", true),
        ("/notes-archive/d.md", false),
    ] {
        background.park(parked(Some(path), dirty));
    }
    assert_eq!(
        background
            .dirty_under(Path::new("/notes"))
            .unwrap()
            .path()
            .map(|path| path.as_path()),
        Some(Path::new("/notes/deep/c.md"))
    );
    assert!(background.dirty_under(Path::new("/notes/a.md")).is_none());

    let released = background.release_clean_under(Path::new("/notes"));
    assert_eq!(
        paths(&released),
        [Path::new("/notes/a.md"), Path::new("/notes/deep/b.md")]
    );
    assert!(
        background
            .take_unchanged(Path::new("/notes-archive/d.md"), "text", None)
            .is_some(),
        "a sibling folder sharing the prefix is not below the moved one"
    );
    assert_eq!(background.dirty().count(), 1);
}

fn app(source: &str, path: Option<&str>) -> App {
    let (mut app, _task) = App::new();
    let name = path.map_or("Untitled.md", |path| path.rsplit('/').next().unwrap());
    app.set_document(source.into(), path.map(PathBuf::from), name.into());
    app
}

fn send(app: &mut App, message: Message) {
    drop(app.update(message));
}

fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        send(app, Message::Edit(Action::Edit(Edit::Insert(character))));
    }
}

/// Completes a save of the page as it stands, without touching the disk.
fn saved(app: &mut App, path: &str) {
    let state = app.session.document.collaboration().state();
    send(
        app,
        Message::Saved {
            result: Ok(path.into()),
            source: Arc::new(app.session.document.source().to_owned()),
            version: collaboration_store::version(&state),
            generation: app.session.document.collaboration().generation(),
        },
    );
    assert!(!app.session.dirty);
}

fn loaded(path: &str, source: &str) -> Message {
    Message::Loaded(Ok(LoadedFile {
        path: path.into(),
        source: source.into(),
        collaboration: None,
    }))
}

#[test]
fn an_unchanged_file_returns_with_its_undo_history_and_a_changed_one_is_reread() {
    let mut app = app("One", Some("/writing/one.md"));
    type_text(&mut app, "!");
    saved(&mut app, "/writing/one.md");
    let session = app.session.id();
    let checkpoint = app.session.document.collaboration().state();
    let source = app.session.document.source().to_owned();

    send(&mut app, loaded("/writing/two.md", "Two"));
    assert_eq!(app.session.document.source(), "Two");
    assert!(
        app.notification.is_none(),
        "a clean document leaves quietly"
    );
    send(
        &mut app,
        Message::Loaded(Ok(LoadedFile {
            path: "/writing/one.md".into(),
            source: source.clone(),
            collaboration: Some(checkpoint),
        })),
    );
    assert_eq!(app.session.id(), session);
    assert!(app.session.document.can_undo());
    send(&mut app, Message::Undo);
    assert_eq!(app.session.document.source(), "One");
    send(&mut app, Message::Redo);
    saved(&mut app, "/writing/one.md");

    send(&mut app, loaded("/writing/two.md", "Two"));
    send(&mut app, loaded("/writing/one.md", "Rewritten elsewhere"));
    assert_ne!(app.session.id(), session);
    assert_eq!(app.session.document.source(), "Rewritten elsewhere");
    assert!(!app.session.document.can_undo());
    send(&mut app, Message::Step(Direction::Back));
    assert!(app.loading, "a clean document is reached through its file");
}

#[test]
fn choosing_the_file_of_an_unsaved_page_keeps_the_writing() {
    let mut app = app("Saved", Some("/writing/note.md"));
    type_text(&mut app, "Unsaved ");
    send(
        &mut app,
        Message::ChosenFile(Some("/writing/note.md".into())),
    );
    assert!(!app.loading && !app.busy);
    assert_eq!(app.session.document.source(), "Unsaved Saved");
    assert!(app.notification.as_deref().unwrap().contains("note.md"));

    // The same file reached while its writing is in the background returns
    // that writing instead of reading the saved copy over it.
    send(&mut app, loaded("/writing/other.md", "Other"));
    send(
        &mut app,
        Message::ChosenFile(Some("/writing/note.md".into())),
    );
    assert!(!app.loading);
    assert_eq!(app.session.document.source(), "Unsaved Saved");
    assert!(app.session.dirty && app.session.document.can_undo());
}

#[test]
fn closing_asks_once_about_every_unsaved_document_and_discard_forgets_them() {
    let mut app = app("", None);
    type_text(&mut app, "First");
    send(&mut app, Message::New);
    assert!(app.overlay.is_none());
    type_text(&mut app, "Second");
    send(&mut app, Message::New);
    let id = window::Id::unique();

    send(&mut app, Message::CloseRequested(id));
    assert_eq!(app.overlay, Some(Overlay::Unsaved));
    assert_eq!(app.unsaved().count(), 2);
    send(&mut app, Message::CloseOverlay);
    assert!(app.pending.is_none());
    assert_eq!(app.background.dirty().count(), 2, "cancel keeps everything");

    send(&mut app, Message::CloseRequested(id));
    send(&mut app, Message::ConfirmDiscard);
    assert!(app.overlay.is_none());
    assert_eq!(app.unsaved().count(), 0);
}

#[test]
fn saving_on_close_brings_each_unsaved_background_document_to_the_page() {
    let mut app = app("A", Some("/writing/a.md"));
    type_text(&mut app, "!");
    send(&mut app, loaded("/writing/b.md", "B"));
    type_text(&mut app, "?");
    send(&mut app, loaded("/writing/c.md", "C"));
    let id = window::Id::unique();

    send(&mut app, Message::CloseRequested(id));
    assert_eq!(app.overlay, Some(Overlay::Unsaved));
    for path in ["/writing/b.md", "/writing/a.md"] {
        send(&mut app, Message::ConfirmSave);
        assert_eq!(
            app.session.path().map(|value| value.as_path()),
            Some(Path::new(path))
        );
        assert!(app.busy, "the document on the page is being saved");
        assert_eq!(app.overlay, Some(Overlay::Unsaved));
        assert!(matches!(app.pending, Some(Pending::Close(pending)) if pending == id));
        app.pending = None;
        saved(&mut app, path);
        app.pending = Some(Pending::Close(id));
        app.overlay = Some(Overlay::Unsaved);
    }
    send(&mut app, Message::ConfirmSave);
    assert!(app.overlay.is_none() && app.pending.is_none());
    assert_eq!(app.unsaved().count(), 0);
}

#[test]
fn an_unsaved_background_document_returns_by_its_session() {
    let mut app = app("", None);
    type_text(&mut app, "First");
    let first = app.session.id();
    send(&mut app, Message::New);
    type_text(&mut app, "Second");

    app.overlay = Some(Overlay::Settings);
    send(&mut app, Message::Resume(first));
    assert_eq!(
        app.session.document.source(),
        "Second",
        "nothing moves under a dialog"
    );
    app.overlay = None;
    send(&mut app, Message::Resume(first));
    assert_eq!(app.session.id(), first);
    assert_eq!(app.session.document.source(), "First");
    assert!(app.session.dirty && app.session.document.can_undo());
    assert_eq!(app.background.dirty().count(), 1);

    send(&mut app, Message::Resume(first));
    assert_eq!(
        app.session.id(),
        first,
        "the page itself is not in the background"
    );
}

#[test]
fn the_close_prompt_names_a_few_documents_and_counts_the_rest() {
    assert_eq!(
        unsaved_summary(&["Draft.md"]),
        "Draft.md has unsaved changes. Save them before closing."
    );
    assert_eq!(
        unsaved_summary(&["a.md", "b.md"]),
        "2 documents have unsaved changes: a.md, b.md. Save them before closing."
    );
    assert_eq!(
        unsaved_summary(&["a.md", "b.md", "c.md", "d.md", "e.md"]),
        "5 documents have unsaved changes: a.md, b.md, c.md and 2 more. Save them before closing."
    );
}
