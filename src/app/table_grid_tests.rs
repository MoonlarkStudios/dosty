use super::*;
use crate::document::collaboration::{EditOrigin, SourceEdit};
use crate::document::projection::Projection;
use iced::widget::text_editor::{Action, Edit};

const SOURCE: &str = include_str!("../../tests/fixtures/table-grid.md");

fn app(source: &str) -> App {
    let (mut app, task) = App::new();
    drop(task);
    app.set_document(source.into(), None, "Launch plan.md".into());
    app.mode = EditorMode::Live;
    app.editor_focused = true;
    app.sync_editor();
    app
}
fn send(app: &mut App, message: Message) {
    drop(app.update_message(AppMessage::TableGrid(message)));
}
fn select(app: &mut App, row: usize, column: usize, extend: bool) {
    send(
        app,
        Message::Select {
            revision: Arc::clone(&app.projection.source_id),
            table: 0,
            row,
            column,
            extend,
            extent: Extent::Cell,
        },
    );
}
fn value(app: &App, row: usize, column: usize) -> &str {
    &app.projection.text[app.projection.rich.tables[0].rows[row][column].clone()]
}
fn synced(app: &App) {
    let fresh = Projection::with_editing_cell(
        app.document.source(),
        Some(app.selection.head),
        app.editing_cell_range(),
    );
    assert_eq!(app.projection.text, fresh.text);
    assert_eq!(app.projection.rich, fresh.rich);
    assert_eq!(app.editor.snapshot(), fresh.text);
    assert_eq!(
        app.document.source(),
        app.document.collaboration.materialize()
    );
}

#[test]
fn grid_selection_preserves_source_and_direction_and_clear_only_touches_the_rectangle() {
    for reverse in [false, true] {
        let mut app = app(SOURCE);
        let (a, h) = if reverse {
            ((3, 1), (1, 1))
        } else {
            ((1, 1), (3, 1))
        };
        select(&mut app, a.0, a.1, false);
        select(&mut app, h.0, h.1, true);
        assert_eq!(app.document.source(), SOURCE);
        assert!(!app.document.can_undo());
        let view = app.grid_view().unwrap();
        assert_eq!(view.anchor, a);
        assert_eq!(view.head, h);
        assert_eq!(app.selection.anchor > app.selection.head, reverse);
        send(&mut app, Message::Clear);
        for row in 1..4 {
            assert_eq!(value(&app, row, 1), "");
        }
        for word in [
            "Draft proposal",
            "Review together",
            "Publish the plan",
            "In progress",
            "Ready",
            "Planned",
        ] {
            assert!(app.document.source().contains(word));
        }
        assert!(app.document.source().starts_with("# Launch plan\n\nKeep"));
        assert!(
            app.document
                .source()
                .ends_with("Notes and decisions continue below the table.\n")
        );
        synced(&app);
        drop(app.update_message(AppMessage::Undo));
        assert_eq!(app.document.source(), SOURCE);
        assert_eq!(app.grid_view().unwrap().anchor, a);
        assert_eq!(app.grid_view().unwrap().head, h);
        drop(app.update_message(AppMessage::Redo));
        assert_eq!(value(&app, 2, 1), "");
        synced(&app);
    }
}

#[test]
fn pending_cell_edit_cancel_and_local_undo_never_modify_markdown() {
    let mut app = app(SOURCE);
    select(&mut app, 1, 1, false);
    send(&mut app, Message::Begin(None));
    assert_eq!(app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Maya");
    send(&mut app, Message::Draft(Action::Edit(Edit::Insert('!'))));
    assert_eq!(app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Maya!");
    send(&mut app, Message::DraftUndo(false));
    assert_eq!(app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Maya");
    send(&mut app, Message::DraftUndo(true));
    assert_eq!(app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Maya!");
    assert_eq!(app.document.source(), SOURCE);
    assert!(!app.document.can_undo());
    send(&mut app, Message::Cancel);
    assert!(app.grid.draft.is_none());
    assert_eq!(app.document.source(), SOURCE);
    assert!(!app.document.can_undo());
    send(&mut app, Message::Begin(Some(" replacement ".into())));
    send(&mut app, Message::Cancel);
    assert_eq!(value(&app, 1, 1), "Maya");
}

#[test]
fn cell_commit_is_one_undo_step_keeps_formatting_and_serializes_literal_unicode() {
    for text in ["Maya 🧑‍💻", " **literal** | café\n\nnext\t "] {
        let mut app = app(SOURCE);
        select(&mut app, 1, 1, false);
        send(&mut app, Message::Begin(Some(text.into())));
        send(&mut app, Message::Commit(None));
        assert_eq!(value(&app, 1, 1), text);
        assert!(app.grid.draft.is_none());
        assert!(app.document.source().contains("**"));
        synced(&app);
        drop(app.update_message(AppMessage::Undo));
        assert_eq!(app.document.source(), SOURCE);
        assert!(!app.document.can_undo());
        drop(app.update_message(AppMessage::Redo));
        assert_eq!(value(&app, 1, 1), text);
        synced(&app);
    }
}

#[test]
fn implicit_cells_are_selected_without_source_edits_and_materialized_only_on_commit() {
    for ending in ["\n", "\r\n", "\r"] {
        let source = format!(
            "Before{ending}{ending}> | A | B | C |{ending}> |---|---|---|{ending}> | first |{ending}{ending}After"
        );
        let mut app = app(&source);
        select(&mut app, 1, 2, false);
        assert_eq!(app.document.source(), source);
        send(&mut app, Message::Begin(Some("café | 🧑‍💻".into())));
        send(&mut app, Message::Cancel);
        assert_eq!(app.document.source(), source);
        send(&mut app, Message::Begin(Some("café | 🧑‍💻".into())));
        send(&mut app, Message::Commit(None));
        assert_eq!(value(&app, 1, 2), "café | 🧑‍💻");
        assert_eq!(value(&app, 1, 1), "");
        synced(&app);
        drop(app.update_message(AppMessage::Undo));
        assert_eq!(app.document.source(), source);
    }
}

#[test]
fn tab_remembers_entry_column_and_selected_rectangles_keep_their_bounds() {
    let mut app = app(SOURCE);
    select(&mut app, 1, 1, false);
    send(&mut app, Message::Tab(false));
    assert_eq!(app.grid_view().unwrap().active, (1, 2));
    send(&mut app, Message::Begin(Some("Done".into())));
    send(&mut app, Message::Commit(Some((1, 0))));
    assert_eq!(app.grid_view().unwrap().active, (2, 1));
    assert_eq!(value(&app, 1, 2), "Done");
    select(&mut app, 1, 1, false);
    select(&mut app, 2, 2, true);
    for expected in [(1, 2), (2, 1), (2, 2), (1, 1)] {
        send(&mut app, Message::Tab(false));
        let view = app.grid_view().unwrap();
        assert_eq!(view.active, expected);
        assert_eq!(view.anchor, (1, 1));
        assert_eq!(view.head, (2, 2));
    }
    send(&mut app, Message::Tab(true));
    assert_eq!(app.grid_view().unwrap().active, (2, 2));
    send(&mut app, Message::Begin(Some("Changed".into())));
    send(&mut app, Message::Commit(Some((1, 0))));
    assert_eq!(value(&app, 2, 2), "Changed");
    assert_eq!(app.grid_view().unwrap().active, (1, 1));
}

#[test]
fn grid_navigation_clamps_and_final_tab_adds_a_row_with_one_structural_undo() {
    let mut app = app(SOURCE);
    select(&mut app, 0, 0, false);
    send(
        &mut app,
        Message::Move {
            rows: -1,
            columns: -1,
            extend: false,
        },
    );
    assert_eq!(app.grid_view().unwrap().active, (0, 0));
    send(&mut app, Message::Tab(true));
    assert_eq!(app.document.source(), SOURCE);
    select(&mut app, 3, 2, false);
    send(&mut app, Message::Tab(false));
    assert_eq!(app.grid_view().unwrap().active, (4, 0));
    assert_eq!(app.projection.rich.tables[0].rows.len(), 5);
    drop(app.update_message(AppMessage::Undo));
    assert_eq!(app.document.source(), SOURCE);
    assert!(!app.document.can_undo());
    synced(&app);
}

#[test]
fn drafts_follow_unrelated_source_changes_and_reject_changed_cells_and_stale_clicks() {
    let mut app = app(SOURCE);
    select(&mut app, 1, 1, false);
    send(&mut app, Message::Begin(Some("New owner".into())));
    let revision = Arc::clone(&app.projection.source_id);
    app.document
        .transact(
            &[SourceEdit {
                range: 0..0,
                text: "Intro\n\n".into(),
            }],
            EditOrigin::Agent,
            None,
            true,
        )
        .unwrap();
    app.sync_editor();
    send(
        &mut app,
        Message::Select {
            revision,
            table: 0,
            row: 3,
            column: 2,
            extend: false,
            extent: Extent::Cell,
        },
    );
    assert_eq!(app.grid_view().unwrap().active, (1, 1));
    assert!(app.grid.draft.is_some());
    send(&mut app, Message::Commit(None));
    assert_eq!(value(&app, 1, 1), "New owner");
    assert!(app.document.source().starts_with("Intro\n\n"));
    select(&mut app, 2, 1, false);
    send(&mut app, Message::Begin(Some("My draft".into())));
    let start = app.document.source().find("Noah").unwrap();
    app.document
        .transact(
            &[SourceEdit {
                range: start..start + 4,
                text: "Their edit".into(),
            }],
            EditOrigin::Agent,
            None,
            true,
        )
        .unwrap();
    app.sync_editor();
    send(&mut app, Message::Commit(None));
    assert_eq!(value(&app, 2, 1), "Their edit");
    assert!(app.grid.draft.is_some());
    send(&mut app, Message::Cancel);
    assert_eq!(value(&app, 2, 1), "Their edit");
}

use crate::editor::rich_editor::RichEditor;
use iced::advanced::{Layout, Shell, clipboard as clipboard_api, layout, mouse, widget};
use iced::futures::{FutureExt, StreamExt};
use iced::{Event, Point, Rectangle, Renderer, Size, keyboard};

struct Harness {
    app: App,
    tree: widget::Tree,
    renderer: Renderer,
    size: Size,
    clipboard: TestClipboard,
}

#[derive(Default)]
struct TestClipboard {
    standard: Option<String>,
    primary: Option<String>,
    refuse_writes: bool,
}

impl iced::advanced::Clipboard for TestClipboard {
    fn read(&self, kind: clipboard_api::Kind) -> Option<String> {
        match kind {
            clipboard_api::Kind::Standard => self.standard.clone(),
            clipboard_api::Kind::Primary => self.primary.clone(),
        }
    }

    fn write(&mut self, kind: clipboard_api::Kind, contents: String) {
        if self.refuse_writes {
            return;
        }
        match kind {
            clipboard_api::Kind::Standard => self.standard = Some(contents),
            clipboard_api::Kind::Primary => self.primary = Some(contents),
        }
    }
}
impl Harness {
    fn new(source: &str) -> Self {
        let app = app(source);
        let tree = widget::Tree::new(app.editor_view(false).as_widget());
        let mut this = Self {
            app,
            tree,
            renderer: Renderer::Secondary(iced_tiny_skia::Renderer::new(
                iced::Font::DEFAULT,
                18.0.into(),
            )),
            size: Size::new(640.0, 560.0),
            clipboard: TestClipboard::default(),
        };
        this.frame();
        this
    }
    fn frame(&mut self) {
        let mut root = self.app.editor_view(false);
        self.tree.diff(root.as_widget());
        root.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, self.size),
        );
    }
    fn cell(&self, row: usize, column: usize) -> Rectangle {
        RichEditor::table_cell(&self.tree, 0, row, column).unwrap()
    }
    fn focus_cell(&mut self, table: usize, row: usize, column: usize) {
        let task = self
            .app
            .update_message(AppMessage::TableGrid(Message::Select {
                revision: Arc::clone(&self.app.projection.source_id),
                table,
                row,
                column,
                extend: false,
                extent: Extent::Cell,
            }));
        self.effects(task);
        self.frame();
    }
    fn effects(&mut self, task: Task<AppMessage>) {
        if let Some(mut effects) = iced_runtime::task::into_stream(task) {
            let mut pending_polls = 0;
            loop {
                match effects.next().now_or_never() {
                    Some(Some(iced_runtime::Action::Widget(mut operation))) => {
                        let mut root = self.app.editor_view(false);
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
                            operation.as_mut(),
                        );
                        let _ = operation.finish();
                    }
                    Some(Some(iced_runtime::Action::Output(message))) => {
                        let task = self.app.update_message(message);
                        self.effects(task);
                    }
                    Some(Some(iced_runtime::Action::Clipboard(action))) => match action {
                        iced_runtime::clipboard::Action::Write { target, contents } => {
                            iced::advanced::Clipboard::write(&mut self.clipboard, target, contents);
                        }
                        iced_runtime::clipboard::Action::Read { target, channel } => {
                            let contents = iced::advanced::Clipboard::read(&self.clipboard, target);
                            let _ = channel.send(contents);
                        }
                    },
                    Some(None) => break,
                    None if pending_polls < 16 => {
                        // Iced's mapped/chained streams may yield between
                        // immediate clipboard effects. No OS work runs here.
                        pending_polls += 1;
                    }
                    action => panic!("Unexpected cell editor effect: {action:?}"),
                }
            }
        }
    }
    fn interaction(&mut self, point: Point) -> mouse::Interaction {
        let mut root = self.app.editor_view(false);
        self.tree.diff(root.as_widget());
        let node = root.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, self.size),
        );
        root.as_widget().mouse_interaction(
            &self.tree,
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &Rectangle::with_size(self.size),
            &self.renderer,
        )
    }
    fn event(&mut self, event: Event, point: Point) -> iced::window::RedrawRequest {
        let mut messages = Vec::new();
        let (status, redraw) = {
            let mut root = self.app.editor_view(false);
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
                mouse::Cursor::Available(point),
                &self.renderer,
                &mut self.clipboard,
                &mut shell,
                &Rectangle::with_size(self.size),
            );
            (shell.event_status(), shell.redraw_request())
        };
        let routed = super::super::event_message(event, status, iced::window::Id::unique());
        for message in messages.into_iter().chain(routed) {
            let task = self.app.update_message(message);
            self.effects(task);
        }
        self.frame();
        redraw
    }
    fn click(&mut self, row: usize, column: usize) {
        let p = self.cell(row, column).center();
        self.event(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            p,
        );
        self.event(
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            p,
        );
    }
    fn key(&mut self, key: Key, modifiers: Modifiers, text: Option<&str>) {
        self.event(
            Event::Keyboard(keyboard::Event::KeyPressed {
                modified_key: key.clone(),
                key,
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers,
                text: text.map(Into::into),
                repeat: false,
            }),
            Point::new(620.0, 540.0),
        );
    }
}

#[test]
fn real_widget_click_type_enter_arrows_newline_escape_and_ime_use_the_cell_editor() {
    let mut h = Harness::new(SOURCE);
    h.click(1, 1);
    assert_eq!(h.app.grid_view().unwrap().active, (1, 1));
    assert!(h.app.grid.draft.is_none());
    h.key(Key::Character("Z".into()), Modifiers::empty(), Some("Z"));
    assert_eq!(h.app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Z");
    assert_eq!(h.app.document.source(), SOURCE);
    h.key(Key::Character("z".into()), Modifiers::COMMAND, None);
    assert_eq!(h.app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Maya");
    h.key(
        Key::Character("Z".into()),
        Modifiers::COMMAND | Modifiers::SHIFT,
        None,
    );
    assert_eq!(h.app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Z");
    h.key(Key::Named(Named::ArrowLeft), Modifiers::empty(), None);
    h.key(Key::Character("A".into()), Modifiers::empty(), Some("A"));
    assert_eq!(h.app.grid.draft.as_ref().unwrap().buffer.snapshot(), "AZ");
    h.key(Key::Named(Named::Enter), Modifiers::COMMAND, None);
    h.key(Key::Character("b".into()), Modifiers::empty(), Some("b"));
    assert_eq!(
        h.app.grid.draft.as_ref().unwrap().buffer.snapshot(),
        "A\nbZ"
    );
    h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
    assert_eq!(value(&h.app, 1, 1), "A\nbZ");
    assert_eq!(h.app.grid_view().unwrap().active, (2, 1));
    h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
    assert_eq!(h.app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Noah");
    h.key(Key::Named(Named::Escape), Modifiers::empty(), None);
    assert!(h.app.grid.draft.is_none());
    h.key(Key::Named(Named::ArrowRight), Modifiers::empty(), None);
    assert_eq!(h.app.grid_view().unwrap().active, (2, 2));
    h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
    h.event(
        Event::InputMethod(iced::advanced::input_method::Event::Opened),
        Point::ORIGIN,
    );
    h.event(
        Event::InputMethod(iced::advanced::input_method::Event::Preedit(
            "にほん".into(),
            Some(0..9),
        )),
        Point::ORIGIN,
    );
    h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
    assert!(h.app.grid.draft.is_some());
    h.event(
        Event::InputMethod(iced::advanced::input_method::Event::Commit("日本語".into())),
        Point::ORIGIN,
    );
    h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
    assert_eq!(value(&h.app, 2, 2), "Ready日本語");
    synced(&h.app);
}

#[test]
fn real_widget_drag_shift_click_gutters_and_keyboard_selection_keep_the_grid() {
    let mut h = Harness::new(SOURCE);
    let a = h.cell(1, 1).center();
    let b = h.cell(3, 2).center();
    h.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        a,
    );
    h.event(Event::Mouse(mouse::Event::CursorMoved { position: b }), b);
    h.event(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        b,
    );
    let view = h.app.grid_view().unwrap();
    assert_eq!(view.anchor, (1, 1));
    assert_eq!(view.head, (3, 2));
    assert_eq!(h.app.document.source(), SOURCE);
    h.key(Key::Named(Named::ArrowUp), Modifiers::SHIFT, None);
    assert_eq!(h.app.grid_view().unwrap().head, (2, 2));
    h.event(
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::SHIFT)),
        Point::ORIGIN,
    );
    h.click(1, 0);
    assert_eq!(h.app.grid_view().unwrap().head, (1, 0));
    h.event(
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::empty())),
        Point::ORIGIN,
    );
    let cell = h.cell(2, 0);
    let gutter = Point::new(cell.x - 14.0, cell.center_y());
    h.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        gutter,
    );
    h.event(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        gutter,
    );
    let view = h.app.grid_view().unwrap();
    assert_eq!(view.anchor, (2, 0));
    assert_eq!(view.head, (2, 2));
    let cell = h.cell(0, 1);
    let header = Point::new(cell.center_x(), cell.y);
    h.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        header,
    );
    h.event(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        header,
    );
    let view = h.app.grid_view().unwrap();
    assert_eq!(view.anchor, (0, 1));
    assert_eq!(view.head, (3, 1));
    h.key(Key::Named(Named::Delete), Modifiers::empty(), None);
    for row in 0..4 {
        assert_eq!(value(&h.app, row, 1), "");
    }
    assert_eq!(value(&h.app, 1, 0), "Draft proposal");
    assert_eq!(value(&h.app, 1, 2), "In progress");
    synced(&h.app);
}

#[test]
fn selecting_a_large_column_keeps_the_active_cell_and_viewport_and_drafts_survive_table_removal() {
    let source = format!(
        "| A | B | C |\n|---|---|---|\n{}\nAfter",
        "| first | owner | status |\n".repeat(1000)
    );
    let mut h = Harness::new(&source);
    select(&mut h.app, 500, 1, false);
    h.frame();
    let before = RichEditor::intent_stats(&h.tree).0;
    send(&mut h.app, Message::SelectExtent(Extent::Column));
    h.frame();
    assert_eq!(h.app.grid_view().unwrap().active, (500, 1));
    assert!((RichEditor::intent_stats(&h.tree).0 - before).abs() < 1.0);
    send(&mut h.app, Message::Begin(None));
    h.frame();
    send(&mut h.app, Message::Cancel);
    h.frame();
    assert!((RichEditor::intent_stats(&h.tree).0 - before).abs() < 1.0);
    send(&mut h.app, Message::Begin(Some("Retained draft".into())));
    h.frame();
    let table = h.app.projection.rich.tables[0].source_range.clone();
    h.app
        .document
        .transact(
            &[SourceEdit {
                range: table,
                text: String::new(),
            }],
            EditOrigin::Agent,
            None,
            true,
        )
        .unwrap();
    h.app.sync_editor();
    h.frame();
    assert!(h.app.grid_view().is_none());
    assert_eq!(
        h.app.grid.draft.as_ref().unwrap().buffer.snapshot(),
        "Retained draft"
    );
    assert_eq!(h.tree.children.len(), 1);
    send(&mut h.app, Message::Commit(None));
    assert!(h.app.grid.draft.is_some());
    send(&mut h.app, Message::Cancel);
    assert!(h.app.grid.draft.is_none());
}

#[test]
fn keyboard_region_edges_row_column_shortcuts_and_double_click_keep_text_editing_distinct() {
    let mut h = Harness::new(SOURCE);
    h.click(1, 1);
    h.click(1, 1);
    assert_eq!(h.app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Maya");
    h.key(Key::Named(Named::Escape), Modifiers::empty(), None);
    h.key(
        Key::Named(Named::ArrowDown),
        Modifiers::COMMAND | Modifiers::SHIFT,
        None,
    );
    let view = h.app.grid_view().unwrap();
    assert_eq!(view.anchor, (1, 1));
    assert_eq!(view.head, (3, 1));
    h.key(Key::Named(Named::Escape), Modifiers::empty(), None);
    h.key(Key::Named(Named::Space), Modifiers::SHIFT, None);
    let view = h.app.grid_view().unwrap();
    assert_eq!(view.anchor, (1, 0));
    assert_eq!(view.head, (1, 2));
    h.key(Key::Named(Named::Space), Modifiers::CTRL, None);
    let view = h.app.grid_view().unwrap();
    assert_eq!(view.anchor, (0, 1));
    assert_eq!(view.head, (3, 1));
    assert_eq!(h.app.document.source(), SOURCE);
}

#[test]
fn right_clicking_another_cell_commits_the_draft_and_opens_controls_on_the_first_click() {
    let mut h = Harness::new(SOURCE);
    h.click(1, 1);
    h.key(Key::Character("Z".into()), Modifiers::empty(), Some("Z"));
    let point = h.cell(1, 2).center();
    h.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        point,
    );
    assert!(matches!(h.app.overlay, Some(super::super::Overlay::Table)));
    assert!(h.app.grid.draft.is_none());
    assert_eq!(value(&h.app, 1, 1), "Z");
    let task = h.app.update_message(AppMessage::CloseOverlay);
    h.effects(task);
    h.frame();
    assert!(h.app.overlay.is_none());
    drop(h.app.update_message(AppMessage::Undo));
    assert_eq!(h.app.document.source(), SOURCE);
}

#[test]
fn right_clicking_table_handles_and_the_pending_cell_opens_the_targeted_context_menu() {
    for column in [false, true] {
        let mut h = Harness::new(SOURCE);
        let row = h.cell(1, 0);
        h.event(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            Point::new(row.x - 14.0, row.center_y()),
        );
        assert!(h.app.overlay.is_none());
        h.click(1, 1);
        let cell = h.cell(if column { 0 } else { 2 }, if column { 1 } else { 0 });
        let point = if column {
            Point::new(cell.center_x(), cell.y)
        } else {
            Point::new(cell.x - 14.0, cell.center_y())
        };
        let selection = h.app.selection;
        h.event(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            point,
        );
        assert!(h.app.table.is_menu());
        let grid = h.app.grid_view().unwrap();
        assert_eq!(
            (grid.anchor, grid.head),
            if column {
                ((0, 1), (3, 1))
            } else {
                ((2, 0), (2, 2))
            }
        );
        assert_eq!(h.app.selection, selection);
        let task = h.app.update_message(AppMessage::CloseOverlay);
        h.effects(task);
        h.frame();
        h.key(Key::Character("Z".into()), Modifiers::empty(), Some("Z"));
        let point = h.cell(1, 1).center();
        h.event(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            point,
        );
        assert!(h.app.table.is_menu());
        assert!(h.app.grid.draft.is_none());
        assert_eq!(value(&h.app, 1, 1), "Z");
        assert_eq!(h.app.grid_view().unwrap().active, (1, 1));
    }
}

#[test]
fn hover_chevrons_open_scoped_menus_without_moving_cells_or_changing_selection() {
    for zoom in [0.75, 1.0, 1.5] {
        for width in [320.0, 640.0] {
            for column in [false, true] {
                let mut h = Harness::new(SOURCE);
                h.app.zoom = zoom;
                h.size.width = width;
                h.size.height = 900.0;
                h.frame();
                h.click(1, 1);
                let selection = h.app.selection;
                let before = (0..4)
                    .flat_map(|r| (0..3).map(move |c| (r, c)))
                    .map(|(r, c)| h.cell(r, c))
                    .collect::<Vec<_>>();
                let cell = h.cell(if column { 0 } else { 2 }, if column { 1 } else { 0 });
                let point = if column {
                    Point::new(cell.x + cell.width - 10.0 * zoom, cell.y)
                } else {
                    let bottom_space = (cell.height - 22.0 * zoom) / 2.0;
                    Point::new(
                        cell.x - 14.0,
                        cell.center_y() + 11.0 * zoom + bottom_space.min(14.0 * zoom) / 2.0,
                    )
                };
                h.event(
                    Event::Mouse(mouse::Event::CursorMoved { position: point }),
                    point,
                );
                assert_eq!(h.interaction(point), mouse::Interaction::Pointer);
                assert_eq!(h.app.selection, selection);
                h.event(
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    point,
                );
                assert!(h.app.table.is_menu());
                let grid = h.app.grid_view().unwrap();
                assert_eq!(
                    (grid.anchor, grid.head),
                    if column {
                        ((0, 1), (3, 1))
                    } else {
                        ((2, 0), (2, 2))
                    }
                );
                assert_eq!(h.app.selection, selection);
                assert_eq!(h.app.document.source(), SOURCE);
                assert!(!h.app.document.can_undo());
                for (index, rect) in before.iter().enumerate() {
                    assert_eq!(h.cell(index / 3, index % 3), *rect);
                }
                let task = h.app.update_message(AppMessage::CloseOverlay);
                h.effects(task);
                h.frame();
                let label = if column {
                    Point::new(cell.center_x(), cell.y)
                } else {
                    Point::new(cell.x - 14.0, cell.center_y())
                };
                h.event(
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    label,
                );
                h.event(
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    label,
                );
                assert!(h.app.overlay.is_none());
                assert_eq!(
                    (
                        h.app.grid_view().unwrap().anchor,
                        h.app.grid_view().unwrap().head
                    ),
                    if column {
                        ((0, 1), (3, 1))
                    } else {
                        ((2, 0), (2, 2))
                    }
                );
                h.event(
                    Event::Mouse(mouse::Event::CursorMoved { position: label }),
                    label,
                );
                assert_eq!(h.interaction(label), mouse::Interaction::Crosshair);
                assert_eq!(
                    h.event(
                        Event::Mouse(mouse::Event::CursorMoved { position: point }),
                        point
                    ),
                    iced::window::RedrawRequest::NextFrame
                );
                assert_eq!(
                    h.event(
                        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::SHIFT)),
                        point
                    ),
                    iced::window::RedrawRequest::NextFrame
                );
            }
        }
    }
}

#[test]
fn column_chevron_commits_a_header_draft_once_and_shift_click_still_extends_selection() {
    let mut h = Harness::new(SOURCE);
    h.click(0, 1);
    h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
    h.key(Key::Character("!".into()), Modifiers::empty(), Some("!"));
    let cell = h.cell(0, 1);
    let point = Point::new(cell.x + cell.width - 10.0, cell.y + 2.0);
    assert_eq!(h.interaction(point), mouse::Interaction::Pointer);
    h.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        point,
    );
    assert!(h.app.table.is_menu());
    assert!(h.app.grid.draft.is_none());
    assert_eq!(value(&h.app, 0, 1), "Owner!");
    let task = h.app.update_message(AppMessage::CloseOverlay);
    h.effects(task);
    h.frame();
    drop(h.app.update_message(AppMessage::Undo));
    assert_eq!(h.app.document.source(), SOURCE);
    h.frame();
    h.click(1, 0);
    h.event(
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::SHIFT)),
        Point::ORIGIN,
    );
    let cell = h.cell(0, 2);
    let point = Point::new(cell.x + cell.width - 10.0, cell.y);
    h.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        point,
    );
    h.event(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        point,
    );
    assert!(h.app.overlay.is_none());
    let grid = h.app.grid_view().unwrap();
    assert_eq!(grid.anchor.1.min(grid.head.1), 0);
    assert_eq!(grid.anchor.1.max(grid.head.1), 2);
    assert_eq!(h.app.document.source(), SOURCE);
}

#[test]
fn native_draft_paste_normalizes_crlf_without_creating_physical_table_rows() {
    let source = SOURCE.replace('\n', "\r\n");
    let mut app = app(&source);
    select(&mut app, 1, 1, false);
    send(&mut app, Message::Begin(None));
    send(&mut app, Message::Draft(Action::SelectAll));
    send(
        &mut app,
        Message::Draft(Action::Edit(Edit::Paste(Arc::new(
            " first\r\nsecond\rthird\tinside ".into(),
        )))),
    );
    let expected = " first\nsecond\nthird\tinside ";
    assert_eq!(app.grid.draft.as_ref().unwrap().buffer.snapshot(), expected);
    send(&mut app, Message::Commit(None));
    assert_eq!(value(&app, 1, 1), expected);
    assert_eq!(app.projection.rich.tables[0].rows.len(), 4);
    assert!(app.document.source().contains("\r\n| Review together"));
    assert!(
        app.document
            .source()
            .ends_with("Notes and decisions continue below the table.\r\n")
    );
    synced(&app);
    drop(app.update_message(AppMessage::Undo));
    assert_eq!(app.document.source(), source);
}

#[test]
fn table_activation_and_escape_keep_content_geometry_source_and_viewport() {
    for width in [300.0, 640.0] {
        let mut h = Harness::new(SOURCE);
        h.size.width = width;
        let point = SOURCE.find("Notes and decisions").unwrap();
        drop(
            h.app
                .update_message(AppMessage::SelectSource(Selection::caret(point))),
        );
        h.frame();
        let cells = (0..4)
            .flat_map(|row| (0..3).map(move |column| (row, column)))
            .map(|(row, column)| h.cell(row, column))
            .collect::<Vec<_>>();
        let scroll = RichEditor::intent_stats(&h.tree).0;
        h.click(1, 1);
        let selected = h.app.selection;
        for (index, rect) in cells.iter().enumerate() {
            assert_eq!(h.cell(index / 3, index % 3), *rect);
        }
        assert_eq!(RichEditor::intent_stats(&h.tree).0, scroll);
        h.key(Key::Named(Named::Escape), Modifiers::empty(), None);
        assert!(h.app.grid_view().is_none());
        assert_eq!(h.app.selection, selected);
        assert_eq!(h.app.document.source(), SOURCE);
        assert!(!h.app.document.can_undo());
        for (index, rect) in cells.iter().enumerate() {
            assert_eq!(h.cell(index / 3, index % 3), *rect);
        }
        h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
        assert_eq!(h.app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Maya");
        h.key(Key::Named(Named::Escape), Modifiers::empty(), None);
        h.key(Key::Named(Named::Escape), Modifiers::empty(), None);
        assert!(h.app.grid_view().is_none());
        synced(&h.app);
    }
}

#[test]
fn table_handles_outside_the_viewport_do_not_select_hidden_rows() {
    let mut h = Harness::new(SOURCE);
    h.click(1, 1);
    h.size.height = h.cell(2, 0).y - 4.0;
    h.frame();
    let cell = h.cell(3, 0);
    let point = Point::new(cell.x - 14.0, cell.center_y());
    assert!(point.y > h.size.height);
    let selected = h.app.selection;
    h.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        point,
    );
    assert_eq!(h.app.selection, selected);
    assert_eq!(h.app.document.source(), SOURCE);
    assert!(!h.app.document.can_undo());
}

#[test]
fn clicking_prose_commits_and_returns_the_table_to_reading_presentation() {
    let mut h = Harness::new(SOURCE);
    h.click(1, 1);
    h.key(Key::Character("A".into()), Modifiers::empty(), Some("A"));
    let point = SOURCE.find("Notes and decisions").unwrap() + 3;
    let stable = h.app.document.collaboration.position(point, Assoc::After);
    drop(
        h.app
            .update_message(AppMessage::SelectSource(Selection::caret(point))),
    );
    h.frame();
    assert!(h.app.grid_view().is_none());
    assert!(h.app.grid.draft.is_none());
    assert_eq!(value(&h.app, 1, 1), "A");
    assert_eq!(
        h.app.selection.head,
        h.app.document.collaboration.resolve(&stable).unwrap()
    );
    drop(h.app.update_message(AppMessage::Undo));
    assert_eq!(h.app.document.source(), SOURCE);
    synced(&h.app);
}

#[test]
fn quiet_tables_remember_implicit_cell_targets_for_keyboard_and_ime_reentry() {
    let source = "| A | B | C |\n|---|---|---|\n| first |\n\nAfter";
    let mut h = Harness::new(source);
    h.click(1, 2);
    h.key(Key::Named(Named::Escape), Modifiers::empty(), None);
    assert!(h.app.grid_view().is_none());
    assert_eq!(h.app.grid_resume().unwrap().active, (1, 2));
    h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
    assert_eq!(h.app.grid_view().unwrap().active, (1, 2));
    assert!(h.app.grid.draft.is_some());
    h.key(Key::Named(Named::Escape), Modifiers::empty(), None);
    h.key(Key::Named(Named::Escape), Modifiers::empty(), None);
    h.event(
        Event::InputMethod(iced::advanced::input_method::Event::Opened),
        Point::ORIGIN,
    );
    h.event(
        Event::InputMethod(iced::advanced::input_method::Event::Preedit(
            "にほん".into(),
            Some(0..9),
        )),
        Point::ORIGIN,
    );
    h.event(
        Event::InputMethod(iced::advanced::input_method::Event::Commit("日本語".into())),
        Point::ORIGIN,
    );
    assert_eq!(h.app.grid_view().unwrap().active, (1, 2));
    assert_eq!(
        h.app.grid.draft.as_ref().unwrap().buffer.snapshot(),
        "日本語"
    );
    assert_eq!(h.app.document.source(), source);
    h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
    assert_eq!(value(&h.app, 1, 2), "日本語");
    synced(&h.app);
}

#[test]
fn plain_vertical_table_escape_reaches_prose_and_typing_keeps_the_table_unchanged() {
    for ending in ["\n", "\r\n", "\r"] {
        let before = "Before café words wrap across several lines while keeping the nearest prose destination.";
        let after = "After 日本語 words wrap across several lines while keeping the nearest prose destination.";
        let source = [
            before,
            "",
            "| Header words wrap across several lines | Header<br>second line | Last |",
            "|---|---|---|",
            "| middle | sample | retained |",
            "| Last words wrap across several lines | α<br>β<br><br>γ |",
            "",
            after,
        ]
        .join(ending);
        for width in [260.0, 640.0] {
            for (row, key, prose) in [(0, Named::ArrowUp, before), (2, Named::ArrowDown, after)] {
                for column in [0, 1, 2] {
                    let mut h = Harness::new(&source);
                    h.size.width = width;
                    h.focus_cell(0, row, column);
                    let range = h.app.projection.rich.tables[0].source_range.clone();
                    let table = source[range].to_owned();
                    h.key(Key::Named(key), Modifiers::empty(), None);
                    assert!(
                        h.app.grid_view().is_none(),
                        "{ending:?}, width {width}, row {row}, column {column}"
                    );
                    let at = h.app.selection.head;
                    let start = source.find(prose).unwrap();
                    assert!(
                        (start..=start + prose.len()).contains(&at),
                        "{ending:?}, width {width}, row {row}, column {column}: caret {at} is outside adjacent prose {start}..{}",
                        start + prose.len()
                    );
                    assert!(h.app.selection.is_caret());
                    assert_eq!(h.app.document.source(), source);
                    assert!(!h.app.document.can_undo());
                    let selected = h.app.selection;
                    h.key(Key::Character("Ω".into()), Modifiers::empty(), Some("Ω"));
                    let mut expected = source.clone();
                    expected.insert(at, 'Ω');
                    assert_eq!(h.app.document.source(), expected);
                    assert!(h.app.grid.draft.is_none());
                    assert_eq!(
                        &h.app.document.source()
                            [h.app.projection.rich.tables[0].source_range.clone()],
                        table
                    );
                    synced(&h.app);
                    h.key(Key::Character("z".into()), Modifiers::COMMAND, None);
                    assert_eq!(h.app.document.source(), source);
                    assert_eq!(h.app.selection, selected);
                    assert!(!h.app.document.can_undo());
                }
            }
        }
    }
}

#[test]
fn plain_vertical_table_escape_preserves_quoted_and_nested_table_containers() {
    for ending in ["\n", "\r\n", "\r"] {
        for (preamble, prefix) in [("", "> "), ("- Outer", "  > ")] {
            let body = [
                "Before café paragraph",
                "",
                "| A | B | C |",
                "|---|---|---|",
                "| last<br>line | implicit |",
                "",
                "After 日本語 paragraph",
            ]
            .iter()
            .map(|line| format!("{prefix}{line}"))
            .collect::<Vec<_>>()
            .join(ending);
            let source = if preamble.is_empty() {
                body
            } else {
                format!("{preamble}{ending}{body}")
            };
            for (row, key, prose) in [
                (0, Named::ArrowUp, "Before café paragraph"),
                (1, Named::ArrowDown, "After 日本語 paragraph"),
            ] {
                let mut h = Harness::new(&source);
                h.size.width = 280.0;
                h.focus_cell(0, row, 2);
                let table = source[h.app.projection.rich.tables[0].source_range.clone()].to_owned();
                h.key(Key::Named(key), Modifiers::empty(), None);
                assert!(
                    h.app.grid_view().is_none(),
                    "{ending:?}, prefix {prefix:?}, {key:?}"
                );
                let at = h.app.selection.head;
                let start = source.find(prose).unwrap();
                assert!(
                    (start..=start + prose.len()).contains(&at),
                    "{ending:?}, prefix {prefix:?}, {key:?}: caret {at} outside prose {start}..{}",
                    start + prose.len()
                );
                assert_eq!(h.app.document.source(), source);
                assert!(!h.app.document.can_undo());
                h.key(Key::Character("Ω".into()), Modifiers::empty(), Some("Ω"));
                let mut expected = source.clone();
                expected.insert(at, 'Ω');
                assert_eq!(h.app.document.source(), expected);
                assert_eq!(
                    &h.app.document.source()[h.app.projection.rich.tables[0].source_range.clone()],
                    table
                );
                synced(&h.app);
                h.key(Key::Character("z".into()), Modifiers::COMMAND, None);
                assert_eq!(h.app.document.source(), source);
                assert!(!h.app.document.can_undo());
            }
        }
    }
}

#[test]
fn plain_vertical_table_escape_crosses_adjacent_tables_in_the_same_measured_column() {
    let source = "Before\n\n| A | B | C |\n|---|---|---|\n| last | implicit |\n\n| D | E | F |\n|---|---|---|\n| next | row | values |\n\nAfter";
    for width in [260.0, 640.0] {
        let mut h = Harness::new(source);
        h.size.width = width;
        h.focus_cell(0, 1, 2);
        h.key(Key::Named(Named::ArrowDown), Modifiers::empty(), None);
        let grid = h.app.grid_view().unwrap();
        assert_eq!((grid.table, grid.active), (1, (0, 2)));
        h.key(Key::Named(Named::ArrowUp), Modifiers::empty(), None);
        let grid = h.app.grid_view().unwrap();
        assert_eq!((grid.table, grid.active), (0, (1, 2)));
        assert_eq!(h.app.document.source(), source);
        assert!(!h.app.document.can_undo());
        synced(&h.app);
    }
}

#[test]
fn plain_vertical_table_escape_preserves_modifier_ranges_and_native_draft_arrows() {
    let source = "| Header<br>second line | B |\n|---|---|\n| Last<br>second line | value |";
    let mut h = Harness::new(source);
    h.focus_cell(0, 0, 0);
    let selected = h.app.selection;
    let grid = h.app.grid_view().unwrap();
    for (revision, table, row, column) in [
        (Arc::new(()), 0, 0, 0),
        (Arc::clone(&h.app.projection.source_id), 0, 1, 0),
        (Arc::clone(&h.app.projection.source_id), 0, 0, usize::MAX),
        (Arc::clone(&h.app.projection.source_id), usize::MAX, 0, 0),
    ] {
        send(
            &mut h.app,
            Message::ExitBoundary {
                revision,
                table,
                row,
                column,
                up: true,
            },
        );
        assert_eq!(h.app.grid_view().unwrap(), grid);
        assert_eq!(h.app.selection, selected);
        assert_eq!(h.app.document.source(), source);
        assert!(!h.app.document.can_undo());
    }
    h.key(Key::Named(Named::ArrowUp), Modifiers::SHIFT, None);
    assert_eq!(h.app.grid_view().unwrap().active, (0, 0));
    h.key(Key::Named(Named::ArrowDown), Modifiers::SHIFT, None);
    let grid = h.app.grid_view().unwrap();
    assert_eq!((grid.anchor, grid.head), ((0, 0), (1, 0)));
    h.focus_cell(0, 0, 0);
    h.key(Key::Named(Named::ArrowUp), Modifiers::COMMAND, None);
    assert_eq!(h.app.grid_view().unwrap().active, (0, 0));
    h.key(Key::Named(Named::ArrowDown), Modifiers::CTRL, None);
    assert_eq!(h.app.grid_view().unwrap().active, (1, 0));
    h.key(Key::Named(Named::ArrowDown), Modifiers::SHIFT, None);
    assert_eq!(h.app.grid_view().unwrap().head, (1, 0));
    for row in [0, 1] {
        h.focus_cell(0, row, 0);
        h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
        let draft = h.app.grid.draft.as_ref().unwrap();
        let text = draft.buffer.snapshot().to_owned();
        let end = draft.buffer.selection().head;
        let selected = h.app.selection;
        let revision = Arc::clone(&h.app.projection.source_id);
        send(
            &mut h.app,
            Message::ExitBoundary {
                revision,
                table: 0,
                row,
                column: 0,
                up: row == 0,
            },
        );
        assert_eq!(h.app.grid.draft.as_ref().unwrap().buffer.snapshot(), text);
        assert_eq!(
            h.app.grid.draft.as_ref().unwrap().buffer.selection().head,
            end
        );
        assert_eq!(h.app.selection, selected);
        assert_eq!(h.app.document.source(), source);
        assert!(!h.app.document.can_undo());
        h.key(Key::Named(Named::ArrowUp), Modifiers::empty(), None);
        assert!(h.app.grid.draft.as_ref().unwrap().buffer.selection().head < end);
        h.key(Key::Named(Named::ArrowUp), Modifiers::empty(), None);
        h.key(Key::Named(Named::ArrowDown), Modifiers::empty(), None);
        h.key(Key::Named(Named::ArrowDown), Modifiers::empty(), None);
        assert_eq!(h.app.grid.draft.as_ref().unwrap().buffer.snapshot(), text);
        assert_eq!(h.app.grid_view().unwrap().active, (row, 0));
        h.key(Key::Named(Named::Escape), Modifiers::empty(), None);
    }
    assert_eq!(h.app.document.source(), source);
    assert!(!h.app.document.can_undo());
}

fn assert_arrow_creates_paragraph_with_separate_typing_history(
    h: &mut Harness,
    source: &str,
    expected: &str,
    caret: usize,
    key: Named,
) {
    let original_selection = h.app.selection;
    assert_eq!(h.app.document.source(), source);
    assert!(!h.app.document.can_undo());
    h.key(Key::Named(key), Modifiers::empty(), None);
    assert!(h.app.grid_view().is_none());
    assert!(h.app.grid.draft.is_none());
    assert_eq!(h.app.document.source(), expected);
    assert_eq!(h.app.selection, Selection::caret(caret));
    assert!(h.app.document.can_undo());
    synced(&h.app);
    for _ in 0..3 {
        h.key(Key::Named(key), Modifiers::empty(), None);
        assert_eq!(
            h.app.document.source(),
            expected,
            "repeated outward arrows must not add paragraphs"
        );
    }
    h.key(Key::Character("Ω".into()), Modifiers::empty(), Some("Ω"));
    let mut typed = expected.to_owned();
    typed.insert(caret, 'Ω');
    assert_eq!(h.app.document.source(), typed);
    assert!(h.app.grid.draft.is_none());
    synced(&h.app);
    h.key(Key::Character("z".into()), Modifiers::COMMAND, None);
    assert_eq!(h.app.document.source(), expected);
    assert_eq!(h.app.selection, Selection::caret(caret));
    assert!(
        h.app.document.can_undo(),
        "typing undo must retain the paragraph creation step"
    );
    h.key(Key::Character("z".into()), Modifiers::COMMAND, None);
    assert_eq!(h.app.document.source(), source);
    assert_eq!(h.app.selection, original_selection);
    assert!(!h.app.document.can_undo());
    h.key(
        Key::Character("Z".into()),
        Modifiers::COMMAND | Modifiers::SHIFT,
        None,
    );
    assert_eq!(h.app.document.source(), expected);
    assert_eq!(h.app.selection, Selection::caret(caret));
    h.key(
        Key::Character("Z".into()),
        Modifiers::COMMAND | Modifiers::SHIFT,
        None,
    );
    assert_eq!(h.app.document.source(), typed);
    synced(&h.app);
    h.key(Key::Character("z".into()), Modifiers::COMMAND, None);
    h.key(Key::Character("z".into()), Modifiers::COMMAND, None);
    assert_eq!(h.app.document.source(), source);
    assert!(!h.app.document.can_undo());
}

#[test]
fn plain_vertical_table_escape_creates_missing_edge_paragraphs_as_one_undo_step() {
    for ending in ["\n", "\r\n", "\r"] {
        let table = ["| A | B | C |", "|---|---|---|", "| one | two |"].join(ending);
        for trailing in [String::new(), ending.to_owned(), ending.repeat(2)] {
            let source = format!("{table}{trailing}");
            let mut h = Harness::new(&source);
            h.focus_cell(0, 0, 2);
            let expected = format!("{}{source}", ending.repeat(2));
            assert_arrow_creates_paragraph_with_separate_typing_history(
                &mut h,
                &source,
                &expected,
                0,
                Named::ArrowUp,
            );
            if trailing != ending.repeat(2) {
                let mut h = Harness::new(&source);
                h.focus_cell(0, 1, 2);
                let expected = format!("{table}{}", ending.repeat(2));
                let caret = expected.len();
                assert_arrow_creates_paragraph_with_separate_typing_history(
                    &mut h,
                    &source,
                    &expected,
                    caret,
                    Named::ArrowDown,
                );
            }
        }
        let source = format!("{ending}{table}");
        let mut h = Harness::new(&source);
        h.focus_cell(0, 0, 2);
        let expected = format!("{}{table}", ending.repeat(2));
        assert_arrow_creates_paragraph_with_separate_typing_history(
            &mut h,
            &source,
            &expected,
            0,
            Named::ArrowUp,
        );
    }
}

#[test]
fn plain_vertical_table_escape_uses_existing_blank_paragraphs_without_source_or_history_changes() {
    for ending in ["\n", "\r\n", "\r"] {
        let table = ["| A | B |", "|---|---|", "| one | two |"].join(ending);
        for up in [false, true] {
            let source = if up {
                format!("{}{table}", ending.repeat(2))
            } else {
                format!("{table}{}", ending.repeat(2))
            };
            let mut h = Harness::new(&source);
            h.focus_cell(0, if up { 0 } else { 1 }, 1);
            let key = if up { Named::ArrowUp } else { Named::ArrowDown };
            let caret = if up { 0 } else { source.len() };
            for _ in 0..3 {
                h.key(Key::Named(key), Modifiers::empty(), None);
                assert!(h.app.grid_view().is_none());
                assert_eq!(h.app.selection, Selection::caret(caret));
                assert_eq!(h.app.document.source(), source);
                assert!(!h.app.document.can_undo());
            }
            h.key(Key::Character("Ω".into()), Modifiers::empty(), Some("Ω"));
            let mut expected = source.clone();
            expected.insert(caret, 'Ω');
            assert_eq!(h.app.document.source(), expected);
            assert_eq!(h.app.projection.rich.tables.len(), 1);
            assert_eq!(h.app.projection.rich.tables[0].rows.len(), 2);
            h.key(Key::Character("z".into()), Modifiers::COMMAND, None);
            assert_eq!(h.app.document.source(), source);
            assert!(!h.app.document.can_undo());
        }
    }
}

#[test]
fn plain_vertical_table_escape_creates_plain_paragraphs_outside_edge_containers() {
    for ending in ["\n", "\r\n", "\r"] {
        let quoted = ["> | A | B | C |", "> |---|---|---|", "> | one | two |"].join(ending);
        for up in [false, true] {
            let mut h = Harness::new(&quoted);
            h.focus_cell(0, if up { 0 } else { 1 }, 2);
            let expected = if up {
                format!("{}{quoted}", ending.repeat(2))
            } else {
                format!("{quoted}{}", ending.repeat(2))
            };
            let caret = if up { 0 } else { expected.len() };
            assert_arrow_creates_paragraph_with_separate_typing_history(
                &mut h,
                &quoted,
                &expected,
                caret,
                if up { Named::ArrowUp } else { Named::ArrowDown },
            );
        }
        let source = format!("{quoted}{ending}> ");
        let mut h = Harness::new(&source);
        h.focus_cell(0, 1, 2);
        let expected = format!("{source}{}", ending.repeat(2));
        let caret = expected.len();
        assert_arrow_creates_paragraph_with_separate_typing_history(
            &mut h,
            &source,
            &expected,
            caret,
            Named::ArrowDown,
        );
        let nested = [
            "- Item",
            "  > | A | B | C |",
            "  > |---|---|---|",
            "  > | one | two |",
        ]
        .join(ending);
        let mut h = Harness::new(&nested);
        h.focus_cell(0, 1, 2);
        let expected = format!("{nested}{}", ending.repeat(2));
        let caret = expected.len();
        assert_arrow_creates_paragraph_with_separate_typing_history(
            &mut h,
            &nested,
            &expected,
            caret,
            Named::ArrowDown,
        );
    }
}

#[test]
fn plain_vertical_table_escape_reaches_prose_from_a_virtualized_last_implicit_cell() {
    let source = format!(
        "Before\n\n| A | B | C |\n|---|---|---|\n{}| last | implicit |\n\nAfter café paragraph",
        "| body | owner | status |\n".repeat(999)
    );
    let mut h = Harness::new(&source);
    h.size = Size::new(280.0, 180.0);
    h.focus_cell(0, 1000, 2);
    let table = source[h.app.projection.rich.tables[0].source_range.clone()].to_owned();
    h.key(Key::Named(Named::ArrowDown), Modifiers::empty(), None);
    assert!(h.app.grid_view().is_none());
    let at = h.app.selection.head;
    assert!((source.find("After café paragraph").unwrap()..=source.len()).contains(&at));
    assert_eq!(h.app.document.source(), source);
    assert!(!h.app.document.can_undo());
    h.key(Key::Character("Ω".into()), Modifiers::empty(), Some("Ω"));
    let mut expected = source.clone();
    expected.insert(at, 'Ω');
    assert_eq!(h.app.document.source(), expected);
    assert_eq!(
        &h.app.document.source()[h.app.projection.rich.tables[0].source_range.clone()],
        table
    );
    synced(&h.app);
    h.key(Key::Character("z".into()), Modifiers::COMMAND, None);
    assert_eq!(h.app.document.source(), source);
    assert!(!h.app.document.can_undo());
}

#[path = "table_clipboard_tests.rs"]
mod clipboard;
