use super::super::{EditorMode, Pending, folder};
use super::*;
use crate::document::selection::Selection;
use crate::io::folder as discovery;
use iced::widget::text_editor::{Action, Edit};

async fn settle(app: &mut App, task: Task<AppMessage>) {
    use iced::futures::StreamExt;
    let mut tasks = vec![task];
    while let Some(task) = tasks.pop() {
        if let Some(mut effects) = iced_runtime::task::into_stream(task) {
            while let Some(effect) = effects.next().await {
                if let iced_runtime::Action::Output(message) = effect {
                    tasks.push(app.update_message(message));
                }
            }
        }
    }
}

fn app(root: &Path, files: &[PathBuf]) -> App {
    let (mut app, tasks) = App::new();
    drop(tasks);
    app.set_document("# Draft\n\ncafé and 🦀".into(), None, "Draft.md".into());
    drop(
        app.update_folder(folder::Message::Scanned {
            generation: 0,
            result: Ok(discovery::Snapshot {
                root: root.to_owned(),
                entries: files
                    .iter()
                    .map(|path| discovery::Entry {
                        path: path.clone(),
                        is_directory: false,
                    })
                    .collect(),
                limited: false,
                warnings: Vec::new(),
            }),
        }),
    );
    app
}

#[test]
fn filename_and_relative_path_terms_rank_exact_and_prefix_matches_first() {
    let root = Path::new("/writing");
    let mut state = State::default();
    state.index(
        root,
        [
            "/writing/Research/résumé.MD",
            "/writing/Notes/Draft.md",
            "/writing/Old draft.md",
            "/writing/Draft.md.bak.md",
            "/other/Draft.md",
        ]
        .into_iter()
        .map(Path::new),
    );
    state.query = "DRAFT.md".into();
    state.search();
    assert_eq!(
        state.candidates[state.results[0]].relative,
        "Notes/Draft.md"
    );
    assert_eq!(state.matches, 3);
    state.query = "research RÉSUMÉ".into();
    state.search();
    assert_eq!(state.matches, 1);
    assert_eq!(
        state.candidates[state.results[0]].relative,
        "Research/résumé.MD"
    );
    state.query = "missing".into();
    state.search();
    assert!(state.selected_path().is_none());
}

#[test]
fn results_are_bounded_and_refresh_removes_stale_files() {
    let root = Path::new("/writing");
    let files = (0..300)
        .map(|index| root.join(format!("Draft {index:03}.md")))
        .collect::<Vec<_>>();
    let mut app = app(root, &files);
    drop(app.update_quick_open(Message::Open));
    assert_eq!(app.quick_open.matches, 300);
    assert_eq!(app.quick_open.results.len(), MAX_RESULTS);
    drop(app.update_quick_open(Message::Step(i32::MAX)));
    assert_eq!(app.quick_open.selected, MAX_RESULTS - 1);
    drop(app.update_folder(folder::Message::Scanned {
        generation: 0,
        result: Ok(discovery::Snapshot {
            root: root.to_owned(),
            entries: Vec::new(),
            limited: false,
            warnings: Vec::new(),
        }),
    }));
    assert!(app.quick_open.results.is_empty());
    drop(app.update_quick_open(Message::Accept));
    assert_eq!(app.overlay, Some(Overlay::QuickOpen));
}

#[test]
fn cancel_and_workspace_changes_discard_search_without_source_or_preference_changes() {
    for mode in [EditorMode::Live, EditorMode::Source, EditorMode::Read] {
        let root = Path::new("/writing");
        let mut app = app(root, &[root.join("Nested/Notes.md")]);
        drop(app.update_message(AppMessage::Edit(Action::Edit(Edit::Insert('!')))));
        drop(app.update_message(AppMessage::SetMode(mode)));
        app.selection = Selection {
            anchor: app.document.source().len(),
            head: 2,
        };
        app.sync_editor();
        let source = app.document.source().to_owned();
        let selection = app.selection;
        let generation = app.document.collaboration().generation();
        let preferences = toml::to_string(&app.preferences).unwrap();
        drop(app.update_quick_open(Message::Open));
        drop(app.update_quick_open(Message::Query("nested notes".into())));
        assert_eq!(app.quick_open.matches, 1);
        drop(app.update_message(AppMessage::CloseOverlay));
        assert!(
            app.quick_open.candidates.is_empty()
                && app.quick_open.query.is_empty()
                && app.quick_open.root.is_none()
        );
        drop(app.update_quick_open(Message::Open));
        drop(app.update_folder(folder::Message::Scanned {
            generation: 0,
            result: Ok(discovery::Snapshot {
                root: "/another".into(),
                entries: Vec::new(),
                limited: false,
                warnings: Vec::new(),
            }),
        }));
        assert!(app.overlay.is_none());
        assert!(app.quick_open.candidates.is_empty());
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, selection);
        assert_eq!(app.mode, mode);
        assert_eq!(app.document.collaboration().generation(), generation);
        assert!(app.document.can_undo());
        assert_eq!(toml::to_string(&app.preferences).unwrap(), preferences);
        assert!(
            !preferences.contains("/writing")
                && !preferences.contains("recent")
                && !preferences.contains("session")
        );
    }
}

#[tokio::test]
async fn opening_uses_the_unsaved_guard_and_cancel_preserves_the_existing_document() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("Target.md");
    std::fs::write(&target, "# Target").unwrap();
    let mut app = app(directory.path(), std::slice::from_ref(&target));
    drop(app.update_message(AppMessage::Edit(Action::Edit(Edit::Insert('!')))));
    let source = app.document.source().to_owned();
    drop(app.update_quick_open(Message::Open));
    let task = app.update_quick_open(Message::Choose(target.clone()));
    settle(&mut app, task).await;
    assert_eq!(app.overlay, Some(Overlay::Unsaved));
    assert!(matches!(&app.pending, Some(Pending::Open(Some(path))) if path == &target));
    assert_eq!(app.document.source(), source);
    assert!(app.quick_open.candidates.is_empty());
    drop(app.update_message(AppMessage::CloseOverlay));
    assert!(app.pending.is_none());
    assert_eq!(app.document.source(), source);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "# Target");
}

#[tokio::test]
async fn stale_file_choices_cannot_switch_the_workspace_to_a_directory() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("Changed.md");
    std::fs::create_dir(&target).unwrap();
    // The cached entry still describes the file that occupied this path.
    let mut app = app(directory.path(), std::slice::from_ref(&target));
    let source = app.document.source().to_owned();
    let identity = app.document.collaboration().id();
    drop(app.update_quick_open(Message::Open));
    let task = app.update_quick_open(Message::Choose(target));
    settle(&mut app, task).await;
    assert_eq!(app.folder.root(), Some(directory.path()));
    assert_eq!(app.document.source(), source);
    assert_eq!(app.document.collaboration().id(), identity);
    assert!(!app.loading && !app.busy);
    assert!(app.notification.is_some());
}

#[test]
fn quick_open_requires_a_workspace_and_respects_loading_and_modal_guards() {
    let (mut app, tasks) = App::new();
    drop(tasks);
    drop(app.update_quick_open(Message::Open));
    assert!(app.overlay.is_none());
    for overlay in [Overlay::Settings, Overlay::Unsaved, Overlay::Folder] {
        app.overlay = Some(overlay);
        drop(app.update_quick_open(Message::Open));
        assert_eq!(app.overlay, Some(overlay));
    }
    let root = Path::new("/writing");
    let mut app = self::app(root, &[root.join("Draft.md")]);
    app.loading = true;
    drop(app.update_quick_open(Message::Open));
    assert!(app.overlay.is_none());
}

#[test]
fn cancelling_keeps_a_table_draft_and_window_close_still_uses_the_save_guard() {
    use super::super::table_grid;
    let mut app = app(Path::new("/writing"), &[PathBuf::from("/writing/Notes.md")]);
    let source = "| A | B |\n|---|---|\n| old | value |\n";
    app.set_document(source.into(), None, "Table.md".into());
    drop(
        app.update_message(AppMessage::TableGrid(table_grid::Message::Select {
            revision: app.projection.source_id.clone(),
            table: 0,
            row: 1,
            column: 0,
            extend: false,
            extent: table_grid::Extent::Cell,
        })),
    );
    drop(
        app.update_message(AppMessage::TableGrid(table_grid::Message::Begin(Some(
            "pending café".into(),
        )))),
    );
    assert!(!app.dirty);
    drop(app.update_quick_open(Message::Open));
    drop(app.update_message(AppMessage::CloseOverlay));
    assert_eq!(app.document.source(), source);
    assert_eq!(
        app.grid.draft.as_ref().unwrap().buffer.snapshot(),
        "pending café"
    );
    drop(app.update_quick_open(Message::Open));
    drop(app.update_message(AppMessage::CloseRequested(iced::window::Id::unique())));
    assert!(app.document.source().contains("pending café"));
    assert_eq!(app.overlay, Some(Overlay::Unsaved));
    assert!(matches!(app.pending, Some(Pending::Close(_))));
    assert!(app.quick_open.candidates.is_empty());
}
