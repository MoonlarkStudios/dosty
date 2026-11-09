//! Chrome changes must preserve the document and the native writing surface.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::{Pending, Sidebar, event_message};
use super::*;
use crate::document::selection::Selection;
use crate::io::storage::{LoadedFile, Preferences};
use crate::theme::Preset;
use iced::advanced::{Layout, Shell, clipboard, layout, mouse, widget};
use iced::keyboard::{self, Key, key};
use iced::widget::text_editor::{Action, Edit};
use iced::{Event, Rectangle, Renderer};

fn app(source: &str) -> App {
    let (mut app, initial_tasks) = App::new();
    drop(initial_tasks);
    app.set_document(source.into(), None, "Draft.md".into());
    app
}

fn send(app: &mut App, message: AppMessage) {
    drop(app.update_message(message));
    assert_eq!(
        app.document.source(),
        app.document.collaboration().materialize()
    );
}

fn workspace(app: &mut App) {
    send(
        app,
        AppMessage::Folder(folder::Message::Scanned {
            generation: 0,
            result: Ok(crate::io::folder::Snapshot {
                root: "/writing".into(),
                entries: Vec::new(),
                limited: false,
                warnings: Vec::new(),
            }),
        }),
    );
}

fn reverse_selection(source: &str) -> Selection {
    Selection {
        anchor: source.find("🦀").unwrap() + "🦀".len(),
        head: source.find("café").unwrap(),
    }
}

#[test]
fn launch_starts_with_one_blank_document_and_closed_navigation() {
    let (app, initial_tasks) = App::new();
    drop(initial_tasks);
    assert!(app.document.source().is_empty());
    assert!(app.editor.snapshot().is_empty());
    assert!(app.path.is_none());
    assert!(app.folder.root().is_none());
    assert!(!app.chrome.files.open);
    assert!(!app.chrome.sidebars_hidden);
    assert!(!app.dirty);
    assert!(app.overlay.is_none());
    assert!(!app.document.can_undo());
}

#[test]
fn files_panel_requires_a_workspace_and_closing_it_restores_standalone_writing() {
    let source = "# Draft\n\ncafé and 🦀";
    let mut harness = Harness::new(source, EditorMode::Live, Size::new(1180.0, 820.0));
    let before = harness.editor().0;
    harness.message(AppMessage::ToggleSidebar(Sidebar::Files));
    harness.message(AppMessage::ShowSidebar(Sidebar::Files));
    assert!(!harness.app.chrome.files.open);
    assert_eq!(harness.editor().0, before);
    workspace(&mut harness.app);
    harness.message(AppMessage::ToggleSidebar(Sidebar::Files));
    assert!(harness.app.chrome.files.open);
    assert!(harness.editor().0.x > before.x);
    harness.message(AppMessage::Folder(folder::Message::Close));
    assert!(!harness.app.chrome.files.open && harness.app.folder.root().is_none());
    assert_eq!(harness.editor().0, before);
    assert_eq!(harness.app.document.source(), source);
    assert!(!harness.app.document.can_undo());
}

#[test]
fn typing_keeps_navigation_visible_with_legacy_fade_preferences() {
    #[derive(Default)]
    struct Controls(Vec<Rectangle>);
    impl widget::Operation for Controls {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
            if ["titlebar-files", "titlebar-format", "titlebar-outline"]
                .iter()
                .any(|name| id == Some(&widget::Id::from(*name)))
            {
                self.0.push(bounds);
            }
        }
    }
    for mode in [EditorMode::Live, EditorMode::Source] {
        let mut harness = Harness::new("# Draft\n\ncafé and 🦀", mode, Size::new(1180.0, 820.0));
        harness.app.preferences =
            toml::from_str("always_show_controls = false\nanimate_controls = true").unwrap();
        workspace(&mut harness.app);
        harness.app.chrome.files.open = true;
        harness.app.chrome.outline.open = true;
        let mut before = Controls::default();
        harness.operate(&mut before);
        assert_eq!(before.0.len(), 3);
        assert!(
            before
                .0
                .iter()
                .all(|bounds| bounds.width >= 30.0 && bounds.height >= 24.0)
        );
        harness.message(AppMessage::Edit(Action::Edit(Edit::Insert('!'))));
        harness.input(
            Event::Window(window::Event::RedrawRequested(
                Instant::now() + Duration::from_secs(1),
            )),
            mouse::Cursor::Unavailable,
        );
        let mut after = Controls::default();
        harness.operate(&mut after);
        assert_eq!(before.0, after.0);
        assert!(harness.app.dirty);
        assert!(harness.app.chrome.files.open && harness.app.chrome.outline.open);
    }
}

#[test]
fn typeface_preference_survives_themes_without_changing_source_selection_or_history() {
    use crate::theme::WritingFont;
    let legacy: Preferences =
        toml::from_str("theme = 'Sage'\nalways_show_controls = false\nanimate_controls = true")
            .unwrap();
    assert_eq!(legacy.writing_font, WritingFont::SansSerif);
    assert_eq!(Preferences::default().writing_font, WritingFont::SansSerif);
    for mode in [EditorMode::Live, EditorMode::Source, EditorMode::Read] {
        let mut app = app("# Draft\n\ncafé **words** and 🦀\n\nLast line.");
        send(&mut app, AppMessage::Edit(Action::Edit(Edit::Insert('!'))));
        send(&mut app, AppMessage::SetMode(mode));
        app.selection = reverse_selection(app.document.source());
        app.sync_editor();
        let source = app.document.source().to_owned();
        let selection = app.selection;
        let generation = app.document.collaboration().generation();
        let native_selection = app.editor.selection();
        for font in [
            WritingFont::Serif,
            WritingFont::SansSerif,
            WritingFont::Serif,
        ] {
            send(&mut app, AppMessage::WritingFont(font));
            for preset in Preset::ALL {
                send(&mut app, AppMessage::Theme(preset));
                assert_eq!(app.resolved_theme.typography.font, font);
            }
            send(
                &mut app,
                AppMessage::ThemeLoaded(Ok(Some((
                    "custom.toml".into(),
                    include_str!("../../themes/paper.toml").into(),
                )))),
            );
            assert_eq!(app.resolved_theme.typography.font, font);
            assert_eq!(app.preferences.writing_font, font);
            assert_eq!(app.document.source(), source);
            assert_eq!(app.selection, selection);
            assert_eq!(app.editor.selection(), native_selection);
            assert_eq!(app.document.collaboration().generation(), generation);
            assert_eq!(app.preserve_viewport_generation, Some(generation));
            assert!(app.document.can_undo());
            let restored: Preferences =
                toml::from_str(&toml::to_string(&app.preferences).unwrap()).unwrap();
            assert_eq!(restored.writing_font, font);
        }
    }
}

#[test]
fn saved_snapshot_completion_keeps_later_edits_and_failed_saves_visible() {
    let mut app = app("café and 🦀");
    assert_eq!(app.save_state(), SaveState::Unsaved);
    app.path = Some("/tmp/quiet.md".into());
    assert_eq!(app.save_state(), SaveState::Saved);
    let source = Arc::new(app.document.source().to_owned());
    let generation = app.document.collaboration().generation();
    let version = crate::io::collaboration_store::version(&app.document.collaboration().state());
    app.chrome.saving = true;
    assert_eq!(app.save_state(), SaveState::Saving);
    send(&mut app, AppMessage::Edit(Action::Edit(Edit::Insert('!'))));
    send(
        &mut app,
        AppMessage::Saved {
            result: Ok("/tmp/quiet.md".into()),
            source: Arc::clone(&source),
            version: version.clone(),
            generation,
        },
    );
    assert_eq!(app.save_state(), SaveState::Modified);
    send(
        &mut app,
        AppMessage::Saved {
            result: Err(crate::io::storage::FileError::Conflict),
            source,
            version,
            generation,
        },
    );
    assert_eq!(app.save_state(), SaveState::Failed);
    send(&mut app, AppMessage::Dismiss);
    assert_eq!(
        app.save_state(),
        SaveState::Failed,
        "dismissing a notification must not erase a failed save"
    );
    app.set_document("new".into(), None, "New.md".into());
    assert_eq!(app.save_state(), SaveState::Unsaved);
}

#[test]
fn writing_statistics_are_independent_and_preferences_migrate_without_paths() {
    let mut app = app(&"word ".repeat(201));
    app.preferences = toml::from_str("theme = 'Sage'\nrecent = ['old.md']").unwrap();
    assert_eq!(app.writing_statistics(), "201 words · ~2 min read");
    app.preferences.show_word_count = false;
    assert_eq!(app.writing_statistics(), "~2 min read");
    app.preferences.show_reading_time = false;
    assert_eq!(app.writing_statistics(), "");
    app.preferences.show_word_count = true;
    assert_eq!(app.writing_statistics(), "201 words");
    let saved = toml::to_string(&app.preferences).unwrap();
    let restored: Preferences = toml::from_str(&saved).unwrap();
    assert!(restored.show_word_count);
    assert!(!restored.show_reading_time);
    assert!(!saved.contains("old.md") && !saved.contains("recent"));
    app.set_document(String::new(), None, "Empty.md".into());
    app.preferences.show_reading_time = true;
    assert_eq!(app.writing_statistics(), "0 words · 0 min read");
    app.set_document("thought".into(), None, "Thought.md".into());
    assert_eq!(app.writing_statistics(), "1 word · <1 min read");
}

#[test]
fn hide_sidebars_preserves_reverse_utf8_selection_native_buffer_and_history() {
    let source = "# Draft\n\ncafé **words** and 🦀\n\nLast line.";
    for mode in [EditorMode::Live, EditorMode::Source, EditorMode::Read] {
        for sidebar in [false, true] {
            let mut app = app(source);
            send(&mut app, AppMessage::SetMode(EditorMode::Source));
            send(
                &mut app,
                AppMessage::Edit(Action::Move(iced::widget::text_editor::Motion::DocumentEnd)),
            );
            send(&mut app, AppMessage::Edit(Action::Edit(Edit::Insert('!'))));
            let edited = app.document.source().to_owned();
            send(&mut app, AppMessage::SetMode(mode));
            app.chrome.files.open = sidebar;
            app.chrome.outline.open = true;
            let selection = reverse_selection(&edited);
            app.selection = selection;
            app.sync_editor();
            let native_source = app.editor.snapshot().to_owned();
            let native_selection = app.editor.selection();
            let collaboration = app.document.collaboration();
            let identity = (collaboration.id(), collaboration.replica_id());
            let generation = collaboration.generation();
            let state = collaboration.state().update;
            assert!(app.document.can_undo());
            assert!(!app.document.can_redo());

            for expected_focus in [true, false, true, false] {
                send(&mut app, AppMessage::HideSidebars);
                assert_eq!(app.chrome.sidebars_hidden, expected_focus);
                assert_eq!(app.chrome.files.open, sidebar);
                assert!(app.chrome.outline.open);
                assert_eq!(app.mode, mode);
                assert_eq!(app.document.source(), edited);
                assert_eq!(app.selection, selection);
                assert!(app.selection.anchor > app.selection.head);
                assert_eq!(app.editor.snapshot(), native_source);
                assert_eq!(app.editor.selection(), native_selection);
                let collaboration = app.document.collaboration();
                assert_eq!((collaboration.id(), collaboration.replica_id()), identity);
                assert_eq!(collaboration.generation(), generation);
                assert_eq!(collaboration.state().update, state);
                assert_eq!(app.preserve_viewport_generation, Some(generation));
                assert!(app.dirty);
                assert!(app.document.can_undo());
                assert!(!app.document.can_redo());
            }

            send(&mut app, AppMessage::Undo);
            assert_eq!(app.document.source(), source);
            send(&mut app, AppMessage::Redo);
            assert_eq!(app.document.source(), edited);
        }
    }
}

#[test]
fn titlebar_commands_preserve_the_existing_unsaved_document_guard() {
    let parent = std::env::temp_dir().join("dosty-chrome-command-folder");
    for (index, command) in [
        AppMessage::New,
        AppMessage::Open,
        AppMessage::Folder(folder::Message::NewDocument(parent.clone())),
    ]
    .into_iter()
    .enumerate()
    {
        let source = "café and 🦀";
        let mut app = app(source);
        app.selection = reverse_selection(source);
        app.sync_editor();
        app.dirty = true;
        let selection = app.selection;
        send(&mut app, AppMessage::Chrome(Message::Menu(Menu::File)));
        assert_eq!(app.overlay, Some(Overlay::Chrome));
        send(
            &mut app,
            AppMessage::Chrome(Message::Run(Box::new(command))),
        );
        assert_eq!(app.overlay, Some(Overlay::Unsaved));
        assert!(app.chrome.menu.is_none());
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, selection);
        assert!(app.dirty);
        assert!(!app.loading);
        match (&app.pending, index) {
            (Some(Pending::New), 0) | (Some(Pending::Open(None)), 1) => {}
            (Some(Pending::FolderNew(path)), 2) => assert_eq!(path, &parent),
            pending => panic!("command lost its guarded pending action: {pending:?}"),
        }
    }
}

#[test]
fn opening_and_saving_files_never_adds_document_paths_to_preferences() {
    let mut app = app("");
    app.preferences = toml::from_str::<Preferences>(
        "theme = 'Paper'\nrecent = ['/private/old-project/previous.md']\n",
    )
    .unwrap();
    let appearance = toml::to_string(&app.preferences).unwrap();
    assert!(!appearance.contains("recent"));
    assert!(!appearance.contains("previous.md"));
    for name in ["first.md", "second.md"] {
        let path = std::env::temp_dir()
            .join("dosty-private-project")
            .join(name);
        send(
            &mut app,
            AppMessage::Loaded(Ok(LoadedFile {
                path: path.clone(),
                source: format!("# {name}\n\ncafé and 🦀"),
                collaboration: None,
            })),
        );
        assert_eq!(app.path.as_ref(), Some(&path));
        assert_eq!(toml::to_string(&app.preferences).unwrap(), appearance);
        let source = std::sync::Arc::new(app.document.source().to_owned());
        let version =
            crate::io::collaboration_store::version(&app.document.collaboration().state());
        let generation = app.document.collaboration().generation();
        send(
            &mut app,
            AppMessage::Saved {
                result: Ok(path),
                source,
                version,
                generation,
            },
        );
        assert_eq!(toml::to_string(&app.preferences).unwrap(), appearance);
        assert!(!app.dirty);
    }
}

#[test]
fn loading_blocks_search_replacement_and_document_navigation_without_losing_history() {
    let source = "# First\n\ncafé and 🦀\n\n# Second\n\ncafé again.";
    for mode in [EditorMode::Live, EditorMode::Source, EditorMode::Read] {
        let mut app = app(source);
        send(&mut app, AppMessage::SetMode(EditorMode::Source));
        send(
            &mut app,
            AppMessage::Edit(Action::Move(iced::widget::text_editor::Motion::DocumentEnd)),
        );
        send(&mut app, AppMessage::Edit(Action::Edit(Edit::Insert('!'))));
        app.document.break_history_group();
        send(&mut app, AppMessage::Edit(Action::Edit(Edit::Insert('?'))));
        send(&mut app, AppMessage::Undo);
        let edited = app.document.source().to_owned();
        send(&mut app, AppMessage::Query("café".into()));
        send(&mut app, AppMessage::Replacement("changed".into()));
        send(&mut app, AppMessage::SetMode(mode));
        let selection = reverse_selection(&edited);
        app.selection = selection;
        app.sync_editor();
        let native_source = app.editor.snapshot().to_owned();
        let native_selection = app.editor.selection();
        let generation = app.document.collaboration().generation();
        let replica = app.document.collaboration().state();
        assert!(app.document.can_undo());
        assert!(app.document.can_redo());
        // A move holds this guard while its worker uses the captured source.
        app.loading = true;
        app.busy = true;
        for command in [
            AppMessage::Replace,
            AppMessage::ReplaceAll,
            AppMessage::Append,
            AppMessage::Activate(0),
            AppMessage::JumpTo(2),
            AppMessage::Navigate(-1),
        ] {
            send(&mut app, command);
            assert_eq!(app.document.source(), edited);
            assert_eq!(app.selection, selection);
            assert_eq!(app.editor.snapshot(), native_source);
            assert_eq!(app.editor.selection(), native_selection);
            assert_eq!(app.document.collaboration().generation(), generation);
            assert_eq!(app.document.collaboration().state().update, replica.update);
            assert_eq!(app.document.collaboration().id(), replica.document);
            assert_eq!(app.mode, mode);
            assert!(app.document.can_undo());
            assert!(app.document.can_redo());
        }
        app.loading = false;
        app.busy = false;
        send(&mut app, AppMessage::Redo);
        assert_eq!(app.document.source(), format!("{edited}?"));
        send(&mut app, AppMessage::Undo);
        send(&mut app, AppMessage::Undo);
        assert_eq!(app.document.source(), source);
    }
}

struct Harness {
    app: App,
    tree: widget::Tree,
    renderer: Renderer,
    size: Size,
}

#[tokio::test]
async fn quick_open_keyboard_input_and_cancel_restore_the_previous_focus_and_selection() {
    use super::super::quick_open;
    fn focused(harness: &mut Harness, target: &'static str) -> bool {
        struct Probe {
            target: widget::Id,
            focused: bool,
        }
        impl widget::Operation for Probe {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
                operate(self);
            }
            fn focusable(
                &mut self,
                id: Option<&widget::Id>,
                _: Rectangle,
                state: &mut dyn widget::operation::Focusable,
            ) {
                if id == Some(&self.target) {
                    self.focused = state.is_focused();
                }
            }
        }
        let mut probe = Probe {
            target: widget::Id::from(target),
            focused: false,
        };
        harness.operate(&mut probe);
        probe.focused
    }
    async fn press(
        harness: &mut Harness,
        key: Key,
        modifiers: keyboard::Modifiers,
        text: Option<String>,
    ) -> Vec<AppMessage> {
        harness
            .input_async(
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: key.clone(),
                    modified_key: key,
                    physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
                    location: keyboard::Location::Standard,
                    modifiers,
                    text: text.map(Into::into),
                    repeat: false,
                }),
                mouse::Cursor::Unavailable,
            )
            .await
    }
    async fn type_text(harness: &mut Harness, value: &str) -> Vec<AppMessage> {
        let mut messages = Vec::new();
        for character in value.chars() {
            let value = character.to_string();
            messages.extend(
                press(
                    harness,
                    Key::Character(value.clone().into()),
                    keyboard::Modifiers::empty(),
                    Some(value),
                )
                .await,
            );
        }
        messages
    }
    for mode in [EditorMode::Live, EditorMode::Source] {
        for target in [EDITOR, crate::editor::file_tree::ID] {
            let source = "# Draft\n\ncafé and 🦀\n\n## Next";
            let mut harness = Harness::new(source, mode, Size::new(1180.0, 820.0));
            harness.message(AppMessage::Folder(folder::Message::Scanned {
                generation: 0,
                result: Ok(crate::io::folder::Snapshot {
                    root: "/writing".into(),
                    entries: vec![crate::io::folder::Entry {
                        path: "/writing/Notes.md".into(),
                        is_directory: false,
                    }],
                    limited: false,
                    warnings: Vec::new(),
                }),
            }));
            harness.app.chrome.files.open = true;
            harness.app.chrome.outline.open = true;
            harness.focus(target);
            let selection = harness.app.selection;
            let native_selection = harness.app.editor.selection();
            let editor_bounds = harness.editor().0;
            press(
                &mut harness,
                Key::Character("p".into()),
                keyboard::Modifiers::COMMAND,
                None,
            )
            .await;
            assert_eq!(harness.app.overlay, Some(Overlay::QuickOpen));
            assert!(focused(&mut harness, quick_open::INPUT));
            let messages = type_text(&mut harness, "notes").await;
            assert!(messages.iter().any(|message| matches!(message, AppMessage::QuickOpen(quick_open::Message::Query(query)) if query == "notes")), "{messages:?}");
            press(
                &mut harness,
                Key::Named(key::Named::Home),
                keyboard::Modifiers::empty(),
                None,
            )
            .await;
            press(
                &mut harness,
                Key::Named(key::Named::ArrowDown),
                keyboard::Modifiers::empty(),
                None,
            )
            .await;
            let messages = type_text(&mut harness, "x").await;
            assert!(messages.iter().any(|message| matches!(message, AppMessage::QuickOpen(quick_open::Message::Query(query)) if query == "xnotes")), "result navigation must not move the query caret");
            press(
                &mut harness,
                Key::Named(key::Named::Enter),
                keyboard::Modifiers::empty(),
                None,
            )
            .await;
            assert_eq!(harness.app.overlay, Some(Overlay::QuickOpen));
            press(
                &mut harness,
                Key::Named(key::Named::Escape),
                keyboard::Modifiers::empty(),
                None,
            )
            .await;
            assert!(harness.app.overlay.is_none());
            assert!(focused(&mut harness, target));
            assert_eq!(harness.editor().0, editor_bounds);
            assert_eq!(harness.app.document.source(), source);
            assert_eq!(harness.app.selection, selection);
            assert_eq!(harness.app.editor.selection(), native_selection);
            assert!(harness.app.chrome.files.open && harness.app.chrome.outline.open);
            assert!(!harness.app.document.can_undo());
        }
    }
}

impl Harness {
    fn new(source: &str, mode: EditorMode, size: Size) -> Self {
        let mut app = app(source);
        app.mode = mode;
        app.editor_focused = true;
        app.selection = reverse_selection(source);
        app.sync_editor();
        let tree = widget::Tree::new(app.view().as_widget());
        Self {
            app,
            tree,
            renderer: Renderer::Secondary(iced_tiny_skia::Renderer::new(
                iced::Font::DEFAULT,
                18.0.into(),
            )),
            size,
        }
    }

    fn operate(&mut self, operation: &mut dyn widget::Operation) {
        let mut root = self.app.view();
        self.tree.diff(root.as_widget());
        let node = root.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, self.size),
        );
        root.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            operation,
        );
    }

    fn message(&mut self, message: AppMessage) {
        let task = self.app.update_message(message);
        self.effects(task);
        assert_eq!(
            self.app.document.source(),
            self.app.document.collaboration().materialize()
        );
    }

    fn effects(&mut self, task: Task<AppMessage>) {
        use iced::futures::{FutureExt, StreamExt};
        if let Some(mut effects) = iced_runtime::task::into_stream(task) {
            loop {
                match effects.next().now_or_never() {
                    Some(Some(iced_runtime::Action::Widget(mut operation))) => {
                        self.operate(operation.as_mut());
                        let _ = operation.finish();
                    }
                    Some(Some(iced_runtime::Action::Output(message))) => self.message(message),
                    Some(None) => break,
                    action => panic!("unexpected asynchronous chrome effect: {action:?}"),
                }
            }
        }
    }

    async fn message_async(&mut self, message: AppMessage) {
        use iced::futures::StreamExt;
        let mut tasks = vec![self.app.update_message(message)];
        while let Some(task) = tasks.pop() {
            if let Some(mut effects) = iced_runtime::task::into_stream(task) {
                while let Some(effect) = effects.next().await {
                    match effect {
                        iced_runtime::Action::Widget(mut operation) => {
                            self.operate(operation.as_mut());
                            let _ = operation.finish();
                        }
                        iced_runtime::Action::Output(message) => {
                            tasks.push(self.app.update_message(message));
                        }
                        effect => panic!("unexpected file gesture effect: {effect:?}"),
                    }
                }
            }
        }
        assert_eq!(
            self.app.document.source(),
            self.app.document.collaboration().materialize()
        );
    }

    async fn input_async(&mut self, event: Event, cursor: mouse::Cursor) -> Vec<AppMessage> {
        let messages = self.input(event, cursor);
        for message in &messages {
            self.message_async(message.clone()).await;
        }
        messages
    }

    fn focus(&mut self, id: &'static str) {
        self.operate(&mut widget::operation::focusable::focus::<()>(
            widget::Id::from(id),
        ));
    }

    fn editor(&mut self) -> (Rectangle, bool) {
        #[derive(Default)]
        struct Probe(Option<(Rectangle, bool)>);
        impl widget::Operation for Probe {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
                operate(self);
            }
            fn focusable(
                &mut self,
                id: Option<&widget::Id>,
                bounds: Rectangle,
                state: &mut dyn widget::operation::Focusable,
            ) {
                if id == Some(&widget::Id::from(EDITOR)) {
                    self.0 = Some((bounds, state.is_focused()));
                }
            }
        }
        let mut probe = Probe::default();
        self.operate(&mut probe);
        probe.0.expect("the native editor must remain in the view")
    }

    fn key(&mut self, key: Key, modifiers: keyboard::Modifiers) {
        let event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        });
        for message in self.input(event, mouse::Cursor::Unavailable) {
            self.message(message);
        }
    }

    fn input(&mut self, event: Event, cursor: mouse::Cursor) -> Vec<AppMessage> {
        self.input_with_clipboard(event, cursor, &mut clipboard::Null)
    }

    fn input_with_clipboard(
        &mut self,
        event: Event,
        cursor: mouse::Cursor,
        clipboard: &mut dyn iced::advanced::Clipboard,
    ) -> Vec<AppMessage> {
        let mut messages = Vec::new();
        let status;
        {
            let mut root = self.app.view();
            self.tree.diff(root.as_widget());
            let node = root.as_widget_mut().layout(
                &mut self.tree,
                &self.renderer,
                &layout::Limits::new(Size::ZERO, self.size),
            );
            let mut shell = Shell::new(&mut messages);
            root.as_widget_mut().update(
                &mut self.tree,
                &event,
                Layout::new(&node),
                cursor,
                &self.renderer,
                clipboard,
                &mut shell,
                &Rectangle::with_size(self.size),
            );
            status = shell.event_status();
        }
        let route = match self.app.overlay {
            Some(Overlay::Chrome) => super::event_message,
            Some(Overlay::Folder) => folder::event_message,
            _ => event_message,
        };
        if let Some(message) = route(event, status, iced::window::Id::unique()) {
            messages.push(message);
        }
        messages
    }
}

#[test]
fn sidebar_toggles_preserve_selection_and_navigation_geometry() {
    let source = "# Draft\n\ncafé and 🦀\n\n## Next\n\nLast line.";
    for size in [Size::new(760.0, 520.0), Size::new(1180.0, 820.0)] {
        let mut harness = Harness::new(source, EditorMode::Live, size);
        harness.focus(EDITOR);
        let before = harness.editor().0;
        let selection = harness.app.selection;
        let generation = harness.app.document.collaboration().generation();
        workspace(&mut harness.app);
        harness.message(AppMessage::ToggleSidebar(Sidebar::Files));
        let docked = harness.editor().0;
        assert!(docked.x > before.x && docked.width <= before.width);
        harness.key(Key::Named(key::Named::Escape), keyboard::Modifiers::empty());
        assert!(
            harness.app.chrome.files.open,
            "Escape preserves toggled sidebars"
        );
        harness.message(AppMessage::HideSidebars);
        assert_eq!(harness.editor().0, before);
        assert!(harness.app.chrome.files.open);
        harness.message(AppMessage::HideSidebars);
        assert_eq!(harness.editor().0, docked);
        harness.message(AppMessage::ToggleSidebar(Sidebar::Files));
        assert_eq!(harness.editor().0, before);
        assert!(!harness.app.chrome.files.open);
        assert_eq!(harness.editor().0, before);
        assert_eq!(harness.app.selection, selection);
        assert_eq!(
            harness.app.document.collaboration().generation(),
            generation
        );
        assert!(!harness.app.document.can_undo());
    }
}

#[test]
fn hide_sidebars_shortcut_and_titlebar_reveal_preserve_the_writing_viewport() {
    let source = "# Heading\n\ncafé **words** and 🦀\n\nLast line.";
    for size in [Size::new(760.0, 520.0), Size::new(1180.0, 820.0)] {
        for mode in [EditorMode::Live, EditorMode::Source] {
            let mut harness = Harness::new(source, mode, size);
            workspace(&mut harness.app);
            harness.app.chrome.files.open = true;
            harness.focus(EDITOR);
            let (before, focused) = harness.editor();
            assert!(focused);
            let selection = harness.app.selection;
            let native_selection = harness.app.editor.selection();
            let native_source = harness.app.editor.snapshot().to_owned();
            harness.key(
                Key::Character("f".into()),
                keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
            );
            assert!(harness.app.chrome.sidebars_hidden);
            assert!(
                harness.app.chrome.files.open,
                "focus must retain the sidebar preference"
            );
            // Execute the focus operation returned by the application task.
            harness.focus(EDITOR);
            let (hidden, focused) = harness.editor();
            assert!(focused);
            assert_eq!((hidden.y, hidden.height), (before.y, before.height));
            assert!(hidden.y >= TITLEBAR_HEIGHT);
            assert!(hidden.y + hidden.height <= size.height);
            let (revealed, focused) = harness.editor();
            assert!(focused);
            assert_eq!(hidden, revealed);
            harness.key(Key::Named(key::Named::Escape), keyboard::Modifiers::empty());
            assert!(
                harness.app.chrome.sidebars_hidden,
                "Escape must not undo Hide Sidebars"
            );
            harness.key(
                Key::Character("f".into()),
                keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
            );
            assert!(!harness.app.chrome.sidebars_hidden);
            harness.focus(EDITOR);
            let (restored, focused) = harness.editor();
            assert!(focused);
            assert_eq!(restored, before);
            assert_eq!(harness.app.document.source(), source);
            assert_eq!(harness.app.selection, selection);
            assert_eq!(harness.app.editor.selection(), native_selection);
            assert_eq!(harness.app.editor.snapshot(), native_source);
            assert!(!harness.app.document.can_undo());
        }
    }
}

#[test]
fn statistics_stay_centered_in_the_writing_area_when_preferences_change() {
    #[derive(Default)]
    struct Bounds(Option<Rectangle>);
    impl widget::Operation for Bounds {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
            if id == Some(&widget::Id::from("writing-statistics")) {
                self.0 = Some(bounds);
            }
        }
    }
    for size in [Size::new(760.0, 520.0), Size::new(1180.0, 820.0)] {
        for open in [false, true] {
            let mut harness = Harness::new("# Draft\n\ncafé and 🦀", EditorMode::Live, size);
            harness.app.preferences = Preferences::default();
            workspace(&mut harness.app);
            harness.app.chrome.files.open = open;
            let before = harness.editor().0;
            let mut before_statistics = Bounds::default();
            harness.operate(&mut before_statistics);
            let statistics = before_statistics.0.unwrap();
            assert_eq!(statistics.center_x(), before.center_x());
            assert_eq!(statistics.y + statistics.height, size.height);
            assert!(statistics.y >= before.y + before.height);
            assert_eq!(statistics.height, STATISTICS_HEIGHT);
            harness.app.preferences.show_word_count = false;
            harness.app.preferences.show_reading_time = false;
            let mut hidden_statistics = Bounds::default();
            harness.operate(&mut hidden_statistics);
            assert_eq!(hidden_statistics.0, Some(statistics));
            assert_eq!(harness.editor().0, before);
            assert_eq!(harness.app.document.source(), "# Draft\n\ncafé and 🦀");
        }
    }
}

#[test]
fn both_sidebars_are_independent_and_narrow_windows_keep_a_usable_writing_column() {
    let mut harness = Harness::new(
        "# Draft\n\ncafé and 🦀\n\n## Next\n\nMore.",
        EditorMode::Live,
        Size::new(1180.0, 820.0),
    );
    workspace(&mut harness.app);
    harness.app.chrome.files.open = true;
    harness.app.chrome.outline.open = true;
    let source = harness.app.document.source().to_owned();
    let selection = harness.app.selection;
    assert_eq!(harness.app.docked_sidebars(1180.0), (true, true));
    assert_eq!(harness.app.docked_sidebars(760.0), (true, false));
    let wide = harness.editor().0;
    assert!(wide.x >= FILES_WIDTH);
    assert!(wide.x + wide.width <= 1180.0 - OUTLINE_WIDTH);
    harness.size = Size::new(760.0, 520.0);
    let narrow = harness.editor().0;
    assert!(narrow.width >= 400.0);
    harness.message(AppMessage::ToggleSidebar(Sidebar::Outline));
    assert!(harness.app.chrome.files.open);
    assert!(!harness.app.chrome.outline.open);
    harness.message(AppMessage::ToggleSidebar(Sidebar::Outline));
    harness.message(AppMessage::HideSidebars);
    assert_eq!(harness.app.docked_sidebars(1180.0), (false, false));
    harness.message(AppMessage::ToggleSearch);
    assert!(harness.app.chrome.sidebars_hidden);
    harness.message(AppMessage::ToggleSearch);
    harness.message(AppMessage::HideSidebars);
    assert!(harness.app.chrome.files.open && harness.app.chrome.outline.open);
    assert_eq!(harness.app.document.source(), source);
    assert_eq!(harness.app.selection, selection);
    assert!(!harness.app.document.can_undo());
    harness.message(AppMessage::Outline(super::super::outline::Message::Jump(1)));
    assert!(harness.app.chrome.files.open && harness.app.chrome.outline.open);
}

#[test]
fn format_menu_pointer_actions_preserve_unicode_selection_and_undo() {
    #[derive(Default)]
    struct Locate(Option<Rectangle>);
    impl widget::Operation for Locate {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
            if id == Some(&menu_item_id(1)) {
                self.0 = Some(bounds);
            }
        }
    }
    for mode in [EditorMode::Live, EditorMode::Source] {
        let source = "café and 🦀";
        let mut harness = Harness::new(source, mode, Size::new(1180.0, 820.0));
        harness.app.chrome.files.open = true;
        harness.app.chrome.outline.open = true;
        harness.focus(EDITOR);
        harness.message(AppMessage::SelectSource(Selection {
            anchor: source.len(),
            head: 0,
        }));
        harness.message(AppMessage::Chrome(Message::Menu(Menu::Format)));
        assert_eq!(
            harness.app.selection,
            Selection {
                anchor: source.len(),
                head: 0
            }
        );
        let mut locate = Locate::default();
        harness.operate(&mut locate);
        let point = locate.0.expect("Bold must be available in Format").center();
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            for message in harness.input(Event::Mouse(event), mouse::Cursor::Available(point)) {
                harness.message(message);
            }
        }
        assert!(harness.app.overlay.is_none());
        assert_eq!(harness.app.document.source(), "**café and 🦀**");
        assert!(harness.app.chrome.files.open && harness.app.chrome.outline.open);
        harness.message(AppMessage::Undo);
        assert_eq!(harness.app.document.source(), source);
        assert_eq!(
            harness.app.selection,
            Selection {
                anchor: source.len(),
                head: 0
            }
        );
    }
}

#[test]
fn native_edit_requests_preserve_focused_selection_and_are_consumed_once_in_order() {
    use crate::editor::menu_input;
    for mode in [EditorMode::Live, EditorMode::Source] {
        let mut harness = Harness::new("café and 🦀", mode, Size::new(760.0, 520.0));
        harness.focus(EDITOR);
        harness.message(AppMessage::MenuEdit(menu_input::Command::SelectAll));
        for message in harness.input(
            Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
            mouse::Cursor::Unavailable,
        ) {
            harness.message(message);
        }
        assert_eq!(harness.app.selection.range(), 0.."café and 🦀".len());
        harness.message(AppMessage::Edit(Action::Edit(Edit::Insert('A'))));
        harness.app.document.break_history_group();
        harness.message(AppMessage::Edit(Action::Edit(Edit::Insert('B'))));
        assert_eq!(harness.app.document.source(), "AB");
        harness.message(AppMessage::MenuEdit(menu_input::Command::Undo));
        harness.message(AppMessage::MenuEdit(menu_input::Command::Undo));
        let redraw = Event::Window(iced::window::Event::RedrawRequested(Instant::now()));
        for message in harness.input(redraw.clone(), mouse::Cursor::Unavailable) {
            harness.message(message);
        }
        assert_eq!(harness.app.document.source(), "café and 🦀");
        assert!(harness.app.menu_input.is_empty());
        for message in harness.input(redraw, mouse::Cursor::Unavailable) {
            harness.message(message);
        }
        assert_eq!(harness.app.document.source(), "café and 🦀");
        assert!(harness.app.document.can_redo());
    }
}

#[test]
fn replacing_a_document_discards_undelivered_native_edit_requests() {
    use crate::editor::menu_input;
    let mut harness = Harness::new(
        "old café and 🦀",
        EditorMode::Source,
        Size::new(760.0, 520.0),
    );
    harness.focus(EDITOR);
    harness.message(AppMessage::MenuEdit(menu_input::Command::SelectAll));
    harness
        .app
        .set_document("new café 🦀".into(), None, "New.md".into());
    assert!(harness.app.menu_input.is_empty());
    harness.focus(EDITOR);
    for message in harness.input(
        Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
        mouse::Cursor::Unavailable,
    ) {
        harness.message(message);
    }
    assert!(harness.app.selection.is_caret());
    harness.message(AppMessage::MenuEdit(menu_input::Command::SelectAll));
    for message in harness.input(
        Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
        mouse::Cursor::Unavailable,
    ) {
        harness.message(message);
    }
    assert_eq!(harness.app.selection.range(), 0.."new café 🦀".len());
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn rapid_native_undo_then_save_captures_the_restored_document() {
    use super::super::native_menus::Command;
    use crate::editor::menu_input;
    let source = "café and 🦀";
    for mode in [EditorMode::Live, EditorMode::Source] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Draft.md");
        std::fs::write(&path, source).unwrap();
        let mut harness = Harness::new(source, mode, Size::new(760.0, 520.0));
        harness.app.path = Some(path.clone());
        harness.focus(EDITOR);
        harness.message(AppMessage::Edit(Action::Edit(Edit::Insert('A'))));
        harness.app.document.break_history_group();
        harness.message(AppMessage::Edit(Action::Edit(Edit::Insert('B'))));
        for command in [
            Command::Edit(menu_input::Command::Undo),
            Command::Edit(menu_input::Command::Undo),
            Command::Mode(EditorMode::Read),
            Command::Save,
        ] {
            harness.message(AppMessage::NativeMenu(command));
        }
        assert_eq!(harness.app.mode, mode);
        for _ in 0..2 {
            harness
                .input_async(
                    Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
                    mouse::Cursor::Unavailable,
                )
                .await;
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        assert_eq!(harness.app.document.source(), source);
        assert_eq!(harness.app.mode, EditorMode::Read);
        assert!(!harness.app.dirty);
        assert!(harness.app.menu_input.is_empty());
        assert!(harness.app.native_menu_queue.is_empty());
    }
}

#[test]
fn native_cut_copy_paste_target_the_focused_search_field_instead_of_the_document() {
    use crate::editor::menu_input;
    struct ClipboardText(String);
    impl iced::advanced::Clipboard for ClipboardText {
        fn read(&self, _: clipboard::Kind) -> Option<String> {
            Some(self.0.clone())
        }
        fn write(&mut self, _: clipboard::Kind, contents: String) {
            self.0 = contents;
        }
    }
    let source = "café and 🦀";
    let mut harness = Harness::new(source, EditorMode::Source, Size::new(760.0, 520.0));
    harness.app.search_open = true;
    harness.message(AppMessage::Query("find café 🦀".into()));
    harness.focus(super::super::SEARCH);
    let mut clipboard = ClipboardText(String::new());
    for command in [
        menu_input::Command::SelectAll,
        menu_input::Command::Copy,
        menu_input::Command::Cut,
    ] {
        harness.message(AppMessage::MenuEdit(command));
        let redraw = Event::Window(iced::window::Event::RedrawRequested(Instant::now()));
        for message in
            harness.input_with_clipboard(redraw, mouse::Cursor::Unavailable, &mut clipboard)
        {
            harness.message(message);
        }
    }
    assert_eq!(clipboard.0, "find café 🦀");
    assert_eq!(harness.app.query, "");
    assert_eq!(harness.app.document.source(), source);
    clipboard.0 = "replacement 🦀".into();
    harness.message(AppMessage::MenuEdit(menu_input::Command::Paste));
    for message in harness.input_with_clipboard(
        Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
        mouse::Cursor::Unavailable,
        &mut clipboard,
    ) {
        harness.message(message);
    }
    assert_eq!(harness.app.query, "replacement 🦀");
    assert_eq!(harness.app.document.source(), source);
    assert!(!harness.app.document.can_undo());
}

#[test]
fn long_names_leave_the_save_indicator_visible() {
    #[derive(Default)]
    struct Bounds {
        name: Option<Rectangle>,
        status: Option<Rectangle>,
    }
    impl widget::Operation for Bounds {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
            if id == Some(&widget::Id::from("titlebar-name")) {
                self.name = Some(bounds);
            }
            if id == Some(&widget::Id::from("titlebar-save-state")) {
                self.status = Some(bounds);
            }
        }
    }
    let mut harness = Harness::new("café and 🦀", EditorMode::Live, Size::new(760.0, 520.0));
    harness.app.display_name = "AnExtremelyLongUnbrokenDocumentName".repeat(10);
    let mut bounds = Bounds::default();
    harness.operate(&mut bounds);
    let name = bounds.name.unwrap();
    let status = bounds.status.unwrap();
    assert!(name.x + name.width <= status.x);
    assert!(status.x + status.width < 760.0);
    assert!(status.y >= 0.0 && status.y + status.height <= TITLEBAR_HEIGHT);
    assert!(status.width >= 30.0);
}

#[test]
fn native_escape_dismisses_a_folder_name_field_without_changing_the_document() {
    let source = "café and 🦀";
    for mode in [EditorMode::Live, EditorMode::Source] {
        let mut harness = Harness::new(source, mode, Size::new(760.0, 520.0));
        let selection = harness.app.selection;
        let identity = harness.app.document.collaboration().id();
        send(
            &mut harness.app,
            AppMessage::Folder(folder::Message::NewFolder(
                std::env::temp_dir().join("dosty-chrome-folder-dialog"),
            )),
        );
        assert_eq!(harness.app.overlay, Some(Overlay::Folder));
        harness.focus("folder-name");
        harness.key(Key::Named(key::Named::Escape), keyboard::Modifiers::empty());
        assert!(harness.app.overlay.is_none());
        assert_eq!(harness.app.document.source(), source);
        assert_eq!(harness.app.selection, selection);
        assert_eq!(harness.app.document.collaboration().id(), identity);
        assert!(!harness.app.dirty);
        assert!(!harness.app.document.can_undo());
    }
}

#[test]
fn native_titlebar_menu_keyboard_choices_stay_visible_in_a_short_window() {
    #[derive(Default)]
    struct Bounds {
        target: Option<widget::Id>,
        item: Option<Rectangle>,
        viewport: Option<(Rectangle, iced::Vector)>,
    }
    impl widget::Operation for Bounds {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
            if id == self.target.as_ref() {
                self.item = Some(bounds);
            }
        }
        fn scrollable(
            &mut self,
            id: Option<&widget::Id>,
            bounds: Rectangle,
            _content: Rectangle,
            translation: iced::Vector,
            _state: &mut dyn widget::operation::Scrollable,
        ) {
            if id == Some(&widget::Id::from(MENU_SCROLL)) {
                self.viewport = Some((bounds, translation));
            }
        }
    }
    fn visible_choice(harness: &mut Harness) -> f32 {
        let menu = harness.app.chrome.menu.unwrap();
        let (index, _) = harness
            .app
            .chrome_commands(menu)
            .into_iter()
            .enumerate()
            .filter(|(_, command)| command.enabled)
            .nth(harness.app.chrome.selected)
            .unwrap();
        let mut probe = Bounds {
            target: Some(menu_item_id(index)),
            ..Bounds::default()
        };
        harness.operate(&mut probe);
        let (viewport, translation) = probe.viewport.unwrap();
        let item = probe.item.unwrap() - translation;
        assert!(item.y >= viewport.y - 0.5, "{item:?} above {viewport:?}");
        assert!(
            item.y + item.height <= viewport.y + viewport.height + 0.5,
            "{item:?} below {viewport:?}"
        );
        translation.y
    }

    let source = "# Heading\n\ncafé and 🦀";
    for mode in [EditorMode::Live, EditorMode::Source] {
        let mut harness = Harness::new(source, mode, Size::new(760.0, 300.0));
        let selection = harness.app.selection;
        harness.focus(EDITOR);
        harness.message(AppMessage::Chrome(Message::Menu(Menu::File)));
        assert_eq!(visible_choice(&mut harness), 0.0);
        harness.key(
            Key::Named(key::Named::ArrowUp),
            keyboard::Modifiers::empty(),
        );
        assert!(visible_choice(&mut harness) > 0.0);
        harness.key(
            Key::Named(key::Named::ArrowRight),
            keyboard::Modifiers::empty(),
        );
        assert_eq!(harness.app.chrome.menu, Some(Menu::Help));
        assert_eq!(harness.app.chrome.selected, 0);
        assert_eq!(visible_choice(&mut harness), 0.0);
        harness.key(
            Key::Named(key::Named::ArrowLeft),
            keyboard::Modifiers::empty(),
        );
        assert_eq!(harness.app.chrome.menu, Some(Menu::File));
        assert_eq!(visible_choice(&mut harness), 0.0);
        harness.key(Key::Named(key::Named::Tab), keyboard::Modifiers::empty());
        assert_eq!(harness.app.chrome.selected, 1);
        visible_choice(&mut harness);
        harness.key(Key::Named(key::Named::Tab), keyboard::Modifiers::SHIFT);
        assert_eq!(harness.app.chrome.selected, 0);
        harness.key(Key::Named(key::Named::Escape), keyboard::Modifiers::empty());
        assert!(harness.app.overlay.is_none());

        harness.message(AppMessage::Chrome(Message::Menu(Menu::Document)));
        let source_index = harness
            .app
            .chrome_commands(Menu::Document)
            .iter()
            .filter(|command| command.enabled)
            .position(|command| command.label == "Source")
            .unwrap();
        for _ in 0..source_index.saturating_sub(1) {
            harness.key(
                Key::Named(key::Named::ArrowDown),
                keyboard::Modifiers::empty(),
            );
        }
        harness.key(
            Key::Named(key::Named::ArrowDown),
            keyboard::Modifiers::empty(),
        );
        harness.key(Key::Named(key::Named::Enter), keyboard::Modifiers::empty());
        assert_eq!(harness.app.mode, EditorMode::Source);
        assert!(harness.app.overlay.is_none());
        assert_eq!(harness.app.document.source(), source);
        assert_eq!(harness.app.selection, selection);
        assert!(!harness.app.document.can_undo());
    }
}

#[tokio::test]
async fn native_file_drag_survives_undo_editing_and_cancelled_new_with_hover_expansion() {
    use crate::editor::file_tree;
    use iced::Point;

    fn tree_bounds(harness: &mut Harness) -> Rectangle {
        #[derive(Default)]
        struct Probe(Option<Rectangle>);
        impl widget::Operation for Probe {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
                operate(self);
            }
            fn focusable(
                &mut self,
                id: Option<&widget::Id>,
                bounds: Rectangle,
                _state: &mut dyn widget::operation::Focusable,
            ) {
                if id == Some(&widget::Id::from(file_tree::ID)) {
                    self.0 = Some(bounds);
                }
            }
        }
        let mut probe = Probe::default();
        harness.operate(&mut probe);
        probe.0.unwrap()
    }

    fn row_point(bounds: Rectangle, row: usize) -> Point {
        // Point inside the label, away from actions.
        Point::new(
            bounds.x + 80.0,
            bounds.y + (row as f32 + 0.5) * crate::theme::styles::SIDEBAR_ROW_HEIGHT,
        )
    }

    async fn pointer(harness: &mut Harness, event: mouse::Event, point: Point) -> Vec<AppMessage> {
        harness
            .input_async(Event::Mouse(event), mouse::Cursor::Available(point))
            .await
    }

    async fn drag(harness: &mut Harness, source: &std::path::Path, destination: &std::path::Path) {
        let bounds = tree_bounds(harness);
        // Closed Archive, expanded Drafts, The quiet hour.md.
        let from = row_point(bounds, 2);
        let to = row_point(bounds, 0);
        pointer(harness, mouse::Event::CursorMoved { position: from }, from).await;
        let pressed = pointer(
            harness,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            from,
        )
        .await;
        assert!(pressed.iter().any(|message| matches!(
            message,
            AppMessage::Folder(folder::Message::Tree(file_tree::Action::Focus))
        )));
        let near = from + iced::Vector::new(2.0, 0.0);
        let moved = pointer(harness, mouse::Event::CursorMoved { position: near }, near).await;
        assert!(!moved.iter().any(|message| matches!(
            message,
            AppMessage::Folder(folder::Message::Tree(
                file_tree::Action::Move { .. } | file_tree::Action::Toggle(_)
            ))
        )));
        pointer(harness, mouse::Event::CursorMoved { position: to }, to).await;
        // Advance the event clock without a wall-clock sleep or timing assertion.
        let expanded = harness
            .input_async(
                Event::Window(iced::window::Event::RedrawRequested(
                    Instant::now() + Duration::from_secs(1),
                )),
                mouse::Cursor::Available(to),
            )
            .await;
        assert!(
            expanded.iter().any(|message| matches!(
                message,
                AppMessage::Folder(folder::Message::Tree(file_tree::Action::Expand(path)))
                    if path == destination
            )),
            "a closed target should expand while the pointer is held over it"
        );
        let released = pointer(
            harness,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            to,
        )
        .await;
        assert!(
            released.iter().any(|message| matches!(
                message,
                AppMessage::Folder(folder::Message::Tree(file_tree::Action::Move {
                    source: actual_source,
                    destination: actual_destination,
                })) if actual_source == source && actual_destination == destination
            )),
            "the held pointer gesture should emit a move, not a folder toggle"
        );
    }

    let source = "# The quiet hour\n\ncafé and 🦀\n\nLast line.";
    for mode in [EditorMode::Live, EditorMode::Source] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let drafts = root.join("Drafts");
        let archive = root.join("Archive");
        std::fs::create_dir(&drafts).unwrap();
        std::fs::create_dir(&archive).unwrap();
        let from = drafts.join("The quiet hour.md");
        let to = archive.join("The quiet hour.md");
        std::fs::write(&from, source).unwrap();
        std::fs::write(archive.join("Existing.md"), "An archived note.").unwrap();
        let snapshot = crate::io::folder::scan(
            root,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .await
        .unwrap();
        let mut harness = Harness::new(source, mode, Size::new(1180.0, 820.0));
        harness.app.path = Some(from.clone());
        harness.app.display_name = "The quiet hour.md".into();
        harness.app.chrome.files.open = true;
        harness.message(AppMessage::Folder(folder::Message::Scanned {
            generation: 0,
            result: Ok(snapshot),
        }));
        let identity = harness.app.document.collaboration().id();
        let selection = harness.app.selection;
        drag(&mut harness, &from, &archive).await;
        assert!(!from.exists());
        assert!(to.exists());
        assert_eq!(harness.app.path.as_ref(), Some(&to));
        assert_eq!(harness.app.document.source(), source);
        assert_eq!(harness.app.selection, selection);
        assert!(harness.app.folder.can_undo());
        harness
            .message_async(AppMessage::Folder(folder::Message::UndoMove))
            .await;
        assert!(from.exists());
        assert!(!to.exists());
        assert_eq!(harness.app.path.as_ref(), Some(&from));

        // Close Archive through its actual row gesture before the second drag.
        let archive_point = row_point(tree_bounds(&mut harness), 0);
        pointer(
            &mut harness,
            mouse::Event::CursorMoved {
                position: archive_point,
            },
            archive_point,
        )
        .await;
        pointer(
            &mut harness,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            archive_point,
        )
        .await;
        let collapsed = pointer(
            &mut harness,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            archive_point,
        )
        .await;
        assert!(collapsed.iter().any(|message| matches!(
            message,
            AppMessage::Folder(folder::Message::Tree(file_tree::Action::Toggle(path)))
                if path == &archive
        )));

        harness.message(AppMessage::SetMode(EditorMode::Source));
        harness.focus(EDITOR);
        harness.message(AppMessage::Edit(Action::Move(
            iced::widget::text_editor::Motion::DocumentEnd,
        )));
        harness.message(AppMessage::Edit(Action::Edit(Edit::Paste(
            std::sync::Arc::new("\n\nDirty café 🦀".into()),
        ))));
        let edited = harness.app.document.source().to_owned();
        let selection = harness.app.selection;
        let generation = harness.app.document.collaboration().generation();
        assert!(harness.app.dirty);
        harness.key(Key::Character("n".into()), keyboard::Modifiers::COMMAND);
        assert_eq!(harness.app.overlay, Some(Overlay::Unsaved));
        harness.key(Key::Named(key::Named::Escape), keyboard::Modifiers::empty());
        assert!(harness.app.overlay.is_none());
        drag(&mut harness, &from, &archive).await;
        assert!(!from.exists());
        assert!(to.exists());
        assert_eq!(harness.app.path.as_ref(), Some(&to));
        assert_eq!(harness.app.document.source(), edited);
        assert_eq!(harness.app.editor.snapshot(), edited);
        assert_eq!(harness.app.selection, selection);
        assert_eq!(harness.app.document.collaboration().id(), identity);
        assert_eq!(
            harness.app.document.collaboration().generation(),
            generation
        );
        assert!(harness.app.dirty);
        assert!(harness.app.document.can_undo());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), source);
        harness.message(AppMessage::Undo);
        assert_eq!(harness.app.document.source(), source);
        harness.message(AppMessage::Redo);
        assert_eq!(harness.app.document.source(), edited);
    }
}
