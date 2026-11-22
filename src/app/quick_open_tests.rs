use super::super::trail::Direction;
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

fn entries(files: &[&str], folders: &[&str]) -> Vec<discovery::Entry> {
    let entry = |is_directory: bool| {
        move |path: &&str| discovery::Entry {
            path: PathBuf::from(path),
            is_directory,
        }
    };
    files
        .iter()
        .map(entry(false))
        .chain(folders.iter().map(entry(true)))
        .collect()
}

/// Each row as `heading>title@location`, with only the parts it has.
fn rows(state: &State) -> Vec<String> {
    state
        .rows
        .iter()
        .map(|row| {
            let heading = row
                .heading
                .as_ref()
                .map_or_else(String::new, |heading| format!("{heading}>"));
            let mark = if row.unsaved { "*" } else { "" };
            let location = if row.location.is_empty() {
                String::new()
            } else {
                format!("@{}", row.location)
            };
            format!("{heading}{mark}{}{location}", row.title)
        })
        .collect()
}

fn search(state: &mut State, query: &str) -> Vec<String> {
    state.query = query.into();
    state.list();
    rows(state)
}

#[test]
fn filename_and_relative_path_terms_rank_exact_and_prefix_matches_first() {
    let mut state = State::default();
    state.index(
        Path::new("/writing"),
        &entries(
            &[
                "/writing/Research/résumé.MD",
                "/writing/Notes/Draft.md",
                "/writing/Old draft.md",
                "/writing/Draft.md.bak.md",
                "/other/Draft.md",
            ],
            &[],
        ),
        &[],
    );
    assert_eq!(search(&mut state, "DRAFT.md")[0], "Draft@Notes");
    assert_eq!(state.matches, 3);
    assert_eq!(
        search(&mut state, "research RÉSUMÉ"),
        ["résumé@Research"],
        "titles drop the Markdown extension whatever its case"
    );
    assert!(search(&mut state, "missing").is_empty());
    assert!(state.selected_target().is_none());
}

#[test]
fn recently_active_documents_lead_and_break_ties_without_overriding_rank() {
    let files = [
        "/writing/A.md",
        "/writing/B.md",
        "/writing/Notes/C.md",
        "/writing/C draft.md",
    ];
    let recent = [PathBuf::from("/writing/Notes/C.md"), "/writing/B.md".into()];
    let mut state = State {
        root: Some("/writing".into()),
        active: Some("/writing/B.md".into()),
        ..State::default()
    };
    state.index(
        Path::new("/writing"),
        &entries(&files, &["/writing/Notes"]),
        &recent,
    );
    assert_eq!(
        rows(&state),
        ["Recent>C@Notes", "writing>Notes", "A", "B", "C draft"],
        "the page is left out of Recent; folders lead the place's contents"
    );
    // An exact filename still outranks a recent partial match.
    assert_eq!(search(&mut state, "a.md")[0], "A@writing");
    assert_eq!(search(&mut state, "c"), ["C@Notes", "C draft@writing"]);
}

#[test]
fn folders_are_stepped_into_and_out_of_while_the_query_is_empty() {
    let root = Path::new("/writing");
    let mut app = app(root, &[]);
    drop(app.update_folder(folder::Message::Scanned {
        generation: 0,
        result: Ok(discovery::Snapshot {
            root: root.to_owned(),
            entries: entries(
                &[
                    "/writing/Preface.md",
                    "/writing/Part One/Chapter 1.md",
                    "/writing/Part One/Scenes/Opening.md",
                ],
                &[
                    "/writing/Part One",
                    "/writing/Part One/Scenes",
                    "/writing/Research",
                ],
            ),
            limited: false,
            warnings: Vec::new(),
        }),
    }));
    drop(app.update_quick_open(Message::Open));
    assert_eq!(
        rows(&app.quick_open),
        ["writing>Part One", "Research", "Preface"]
    );

    drop(app.update_quick_open(Message::StepIn));
    assert_eq!(rows(&app.quick_open), ["Part One>Scenes", "Chapter 1"]);
    drop(app.update_quick_open(Message::Accept));
    assert_eq!(rows(&app.quick_open), ["Part One/Scenes>Opening"]);
    assert_eq!(app.overlay, Some(Overlay::QuickOpen));

    drop(app.update_quick_open(Message::StepOut));
    assert_eq!(rows(&app.quick_open), ["Part One>Scenes", "Chapter 1"]);
    drop(app.update_quick_open(Message::StepOut));
    assert_eq!(
        app.quick_open.selected_target(),
        Some(&Target::Folder("/writing/Part One".into())),
        "stepping out returns to the folder that was left"
    );
    drop(app.update_quick_open(Message::StepOut));
    assert_eq!(
        rows(&app.quick_open).len(),
        3,
        "the top has nowhere to step out to"
    );

    drop(app.update_quick_open(Message::Step(1)));
    drop(app.update_quick_open(Message::StepIn));
    assert!(
        rows(&app.quick_open).is_empty(),
        "an empty folder can be entered"
    );
    // Typing searches the whole place again, wherever browsing had reached.
    drop(app.update_quick_open(Message::Query("opening".into())));
    assert_eq!(rows(&app.quick_open), ["Opening@Part One/Scenes"]);
}

#[tokio::test]
async fn unsaved_documents_lead_the_switcher_and_return_as_they_were_left() {
    let root = Path::new("/writing");
    let mut app = app(root, &[root.join("Notes.md")]);
    drop(app.update_message(AppMessage::Edit(Action::Edit(Edit::Insert('!')))));
    let draft = app.session.document.source().to_owned();
    drop(app.update_message(AppMessage::New));
    drop(app.update_message(AppMessage::Edit(Action::Edit(Edit::Insert('?')))));

    drop(app.update_quick_open(Message::Open));
    assert_eq!(
        rows(&app.quick_open),
        ["Unsaved>*Draft@Not saved yet", "writing>Notes"],
        "the page itself is not listed"
    );
    drop(app.update_quick_open(Message::Query("dra".into())));
    assert_eq!(rows(&app.quick_open), ["*Draft@Not saved yet"]);
    let task = app.update_quick_open(Message::Accept);
    settle(&mut app, task).await;
    assert!(app.overlay.is_none());
    assert_eq!(app.session.document.source(), draft);
    assert!(app.session.dirty && app.session.document.can_undo());
    assert_eq!(app.background.dirty().count(), 1, "the other draft is kept");
}

#[tokio::test]
async fn search_reaches_every_place_and_choosing_a_place_browses_it() {
    let library = tempfile::tempdir().unwrap();
    let library = std::fs::canonicalize(library.path()).unwrap();
    let (essays, blog) = (library.join("Essays"), library.join("Blog"));
    std::fs::create_dir_all(blog.join("posts")).unwrap();
    std::fs::create_dir_all(&essays).unwrap();
    for path in [
        blog.join("posts/Quiet rooms.md"),
        blog.join("About.md"),
        essays.join("Quiet hours.md"),
    ] {
        std::fs::write(path, "text").unwrap();
    }
    let mut app = app(&essays, &[essays.join("Quiet hours.md")]);
    app.workspace
        .remember_for_preview(&[essays.to_str().unwrap(), blog.to_str().unwrap()]);

    let task = app.update_quick_open(Message::Open);
    assert_eq!(rows(&app.quick_open), ["Essays>Quiet hours", "Places>Blog"]);
    settle(&mut app, task).await;
    drop(app.update_quick_open(Message::Query("quiet".into())));
    assert_eq!(
        rows(&app.quick_open),
        ["Quiet hours@Essays", "Quiet rooms@Blog / posts"],
        "the open place comes first"
    );
    drop(app.update_quick_open(Message::Query("blog about".into())));
    assert_eq!(rows(&app.quick_open), ["About@Blog"]);

    // Choosing a place keeps the switcher open and shows that place.
    drop(app.update_quick_open(Message::Query(String::new())));
    drop(app.update_quick_open(Message::Step(1)));
    let task = app.update_quick_open(Message::Accept);
    settle(&mut app, task).await;
    assert_eq!(app.overlay, Some(Overlay::QuickOpen));
    assert_eq!(app.folder.root(), Some(blog.as_path()));
    assert_eq!(
        rows(&app.quick_open),
        ["Blog>posts", "About", "Places>Essays"]
    );

    // A document in another place opens, and its place follows it.
    drop(app.update_quick_open(Message::Query("hours".into())));
    assert_eq!(rows(&app.quick_open), ["Quiet hours@Essays"]);
    let task = app.update_quick_open(Message::Accept);
    settle(&mut app, task).await;
    assert!(app.overlay.is_none());
    assert_eq!(app.session.document.source(), "text");
    assert_eq!(app.folder.root(), Some(essays.as_path()));
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
    assert_eq!(app.quick_open.rows.len(), MAX_RESULTS);
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
    assert!(app.quick_open.rows.is_empty());
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
        app.session.selection = Selection {
            anchor: app.session.document.source().len(),
            head: 2,
        };
        app.sync_editor();
        let source = app.session.document.source().to_owned();
        let selection = app.session.selection;
        let generation = app.session.document.collaboration().generation();
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
        assert_eq!(app.session.document.source(), source);
        assert_eq!(app.session.selection, selection);
        assert_eq!(app.mode, mode);
        assert_eq!(
            app.session.document.collaboration().generation(),
            generation
        );
        assert!(app.session.document.can_undo());
        assert_eq!(toml::to_string(&app.preferences).unwrap(), preferences);
        assert!(
            !preferences.contains("/writing")
                && !preferences.contains("recent")
                && !preferences.contains("session")
        );
    }
}

#[tokio::test]
async fn opening_keeps_unsaved_writing_open_and_back_returns_to_it() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("Target.md");
    std::fs::write(&target, "# Target").unwrap();
    let mut app = app(directory.path(), std::slice::from_ref(&target));
    drop(app.update_message(AppMessage::Edit(Action::Edit(Edit::Insert('!')))));
    let source = app.session.document.source().to_owned();
    drop(app.update_quick_open(Message::Open));
    let task = app.update_quick_open(Message::Choose(target.clone()));
    settle(&mut app, task).await;
    assert!(app.overlay.is_none());
    assert!(app.pending.is_none());
    assert_eq!(app.session.path.as_ref(), Some(&target));
    assert_eq!(app.session.document.source(), "# Target");
    assert!(app.quick_open.candidates.is_empty());
    assert!(app.notification.as_deref().unwrap().contains("Draft.md"));
    let task = app.update_message(AppMessage::Step(Direction::Back));
    settle(&mut app, task).await;
    assert_eq!(app.session.document.source(), source);
    assert!(app.session.dirty);
    assert!(app.session.document.can_undo());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "# Target");
}

#[tokio::test]
async fn stale_file_choices_cannot_switch_the_workspace_to_a_directory() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("Changed.md");
    std::fs::create_dir(&target).unwrap();
    // The cached entry still describes the file that occupied this path.
    let mut app = app(directory.path(), std::slice::from_ref(&target));
    let source = app.session.document.source().to_owned();
    let identity = app.session.document.collaboration().id();
    drop(app.update_quick_open(Message::Open));
    let task = app.update_quick_open(Message::Choose(target));
    settle(&mut app, task).await;
    assert_eq!(app.folder.root(), Some(directory.path()));
    assert_eq!(app.session.document.source(), source);
    assert_eq!(app.session.document.collaboration().id(), identity);
    assert!(!app.loading && !app.busy);
    assert!(app.notification.is_some());
}

#[test]
fn the_switcher_opens_without_a_place_and_respects_loading_and_modal_guards() {
    let (mut app, tasks) = App::new();
    drop(tasks);
    drop(app.update_quick_open(Message::Open));
    assert_eq!(app.overlay, Some(Overlay::QuickOpen));
    assert!(app.quick_open.rows.is_empty());
    drop(app.update_quick_open(Message::Cancel));
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
    assert!(!app.session.dirty);
    drop(app.update_quick_open(Message::Open));
    drop(app.update_message(AppMessage::CloseOverlay));
    assert_eq!(app.session.document.source(), source);
    assert_eq!(
        app.grid.draft.as_ref().unwrap().buffer.snapshot(),
        "pending café"
    );
    drop(app.update_quick_open(Message::Open));
    drop(app.update_message(AppMessage::CloseRequested(iced::window::Id::unique())));
    assert!(app.session.document.source().contains("pending café"));
    assert_eq!(app.overlay, Some(Overlay::Unsaved));
    assert!(matches!(app.pending, Some(Pending::Close(_))));
    assert!(app.quick_open.candidates.is_empty());
}

#[test]
fn heading_scope_works_without_a_place_and_survives_a_library_result() {
    let source = "# First\n\n## Notes\n\n# 🦀 Ship\n\n## Notes";
    for mode in [EditorMode::Live, EditorMode::Source, EditorMode::Read] {
        let (mut app, tasks) = App::new();
        drop(tasks);
        app.set_document(
            source.into(),
            Some("/standalone/Notes.md".into()),
            "Notes.md".into(),
        );
        drop(app.update_message(AppMessage::SetMode(mode)));
        drop(app.update_message(AppMessage::HideSidebars));
        let hidden = app.chrome.sidebars_hidden;
        drop(app.update_quick_open(Message::Open));
        drop(app.update_quick_open(Message::Query("# 🦀 notes".into())));
        assert_eq!(rows(&app.quick_open), ["Notes@🦀 Ship"]);
        drop(app.update_quick_open(Message::Library {
            request: app.quick_open.request,
            snapshots: vec![discovery::Snapshot {
                root: "/other".into(),
                entries: entries(&["/other/#Notes.md"], &[]),
                limited: false,
                warnings: Vec::new(),
            }],
        }));
        assert_eq!(rows(&app.quick_open), ["Notes@🦀 Ship"]);
        drop(app.update_quick_open(Message::Accept));
        assert!(app.overlay.is_none());
        assert_eq!(
            app.session.selection,
            Selection::caret(source.rfind("## Notes").unwrap())
        );
        assert_eq!(app.mode, mode);
        assert_eq!(app.chrome.sidebars_hidden, hidden);
        assert_eq!(app.session.document.source(), source);
    }
}

#[test]
fn pinned_files_launch_normally_and_preserve_quick_switch() {
    let root = Path::new("/writing");
    let active = root.join("Active.md");
    let a = root.join("A.md");
    let b = root.join("B.md");
    let mut app = app(
        root,
        &[active.clone(), a.clone(), b.clone(), root.join("#Notes.md")],
    );
    app.set_document("# A".into(), Some(a.clone()), "A.md".into());
    app.set_document("# Active".into(), Some(active), "Active.md".into());
    drop(app.workspace_pin(&b, true));
    drop(app.workspace_pin(&a, true));
    drop(app.update_quick_open(Message::Open));
    assert_eq!(
        rows(&app.quick_open)[..2],
        ["Pinned>B@writing", "A@writing"]
    );
    assert_eq!(app.quick_open.selected_target(), Some(&Target::File(a)));
    assert!(
        !rows(&app.quick_open)
            .iter()
            .any(|row| row.starts_with("Recent>"))
    );

    drop(app.update_quick_open(Message::Query("#notes".into())));
    assert_eq!(rows(&app.quick_open), ["#Notes@writing"]);
}
