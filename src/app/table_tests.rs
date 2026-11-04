use super::*;
use crate::app::application::shortcut;
use crate::document::collaboration::SourceEdit;
use crate::document::projection::{Affinity, Projection};
use crate::document::selection::Selection;
use iced::keyboard::{Key, Modifiers};
use iced::widget::text_editor::{Action, Edit};
use iced::{Event, event, keyboard, window};

fn app(source: &str, mode: EditorMode) -> App {
    let (mut app, task) = App::new();
    drop(task);
    app.set_document(source.into(), None, "Table.md".into());
    app.mode = mode;
    app.editor_focused = true;
    app.sync_editor();
    app
}

fn send(app: &mut App, message: Message) {
    drop(app.update(AppMessage::Table(message)));
}

fn widget_effects(
    app: &mut App,
    tree: &mut iced::advanced::widget::Tree,
    renderer: &iced::Renderer,
    size: iced::Size,
    task: Task<AppMessage>,
) {
    use iced::advanced::{Layout, layout};
    use iced::futures::{FutureExt, StreamExt};
    if let Some(mut effects) = iced_runtime::task::into_stream(task) {
        loop {
            match effects.next().now_or_never() {
                Some(Some(iced_runtime::Action::Widget(mut operation))) => {
                    let mut root = app.view();
                    tree.diff(root.as_widget());
                    let node = root.as_widget_mut().layout(
                        tree,
                        renderer,
                        &layout::Limits::new(iced::Size::ZERO, size),
                    );
                    root.as_widget_mut().operate(
                        tree,
                        Layout::new(&node),
                        renderer,
                        operation.as_mut(),
                    );
                    let _ = operation.finish();
                }
                Some(Some(iced_runtime::Action::Output(message))) => {
                    let next = app.update_message(message);
                    widget_effects(app, tree, renderer, size, next);
                }
                Some(None) => break,
                action => panic!("Unexpected table UI effect: {action:?}"),
            }
        }
    }
}

fn assert_synced(app: &App) {
    let fresh = Projection::with_editing_cell(
        app.document.source(),
        (app.mode == EditorMode::Live).then_some(app.selection.head),
        app.editing_cell_range(),
    );
    assert_eq!(app.projection.text, fresh.text);
    assert_eq!(app.projection.rich, fresh.rich);
    assert_eq!(app.projection.highlights, fresh.highlights);
    assert_eq!(
        app.document.source(),
        app.document.collaboration().materialize()
    );
    assert_eq!(
        app.editor.snapshot(),
        if app.mode == EditorMode::Source {
            app.document.source()
        } else {
            &fresh.text
        }
    );
}

fn assert_mappings(app: &App) {
    let source = app.document.source();
    let fresh = Projection::with_editing_cell(
        source,
        (app.mode == EditorMode::Live).then_some(app.selection.head),
        app.editing_cell_range(),
    );
    for point in source
        .char_indices()
        .map(|(point, _)| point)
        .chain([source.len()])
    {
        assert_eq!(app.projection.to_display(point), fresh.to_display(point));
    }
    for point in fresh
        .text
        .char_indices()
        .map(|(point, _)| point)
        .chain([fresh.text.len()])
    {
        for affinity in [Affinity::Before, Affinity::After] {
            assert_eq!(
                app.projection.to_source(point, affinity, None),
                fresh.to_source(point, affinity, None)
            );
        }
    }
}

fn context_menu(app: &mut App, row: usize, column: usize, extent: crate::app::TableExtent) {
    send(
        app,
        Message::OpenContext {
            revision: std::sync::Arc::clone(&app.projection.source_id),
            table: 0,
            row,
            column,
            position: Point::new(740.0, 500.0),
            extent,
        },
    );
}

#[test]
fn context_menu_preview_and_cancel_preserve_directed_selection_source_and_history() {
    use crate::app::{TableExtent as E, TableGridMessage as G};
    let source = include_str!("../../tests/fixtures/table-grid.md");
    let mut app = app(source, EditorMode::Live);
    for (row, column, extend) in [(3, 2, false), (1, 1, true)] {
        drop(app.update_grid(G::Select {
            revision: std::sync::Arc::clone(&app.projection.source_id),
            table: 0,
            row,
            column,
            extend,
            extent: E::Cell,
        }));
    }
    let before = app.grid_view().unwrap();
    let selection = app.selection;
    context_menu(&mut app, 2, 2, E::Cell);
    let view = app.grid_view().unwrap();
    assert_eq!(
        (view.anchor, view.head, view.active),
        (before.anchor, before.head, before.active)
    );
    send(&mut app, Message::MenuScope(MenuScope::Row));
    assert_eq!(app.grid_view().unwrap().anchor, (2, 0));
    assert_eq!(app.selection, selection);
    drop(app.update_message(AppMessage::CloseOverlay));
    assert_eq!(app.selection, selection);
    assert_eq!(app.grid_view().unwrap().head, before.head);
    assert_eq!(app.document.source(), source);
    assert!(!app.document.can_undo());
    context_menu(&mut app, 0, 0, E::Cell);
    assert_eq!(app.grid_view().unwrap().anchor, (0, 0));
    assert_eq!(app.grid_view().unwrap().head, (0, 0));
    drop(app.update_message(AppMessage::CloseOverlay));
    assert_eq!(app.selection, selection);
}

#[test]
fn context_menu_clear_targets_only_its_row_column_or_existing_rectangle_with_one_undo() {
    use crate::app::{TableExtent as E, TableGridMessage as G};
    let source = "Before\n\n| A | B | C |\n|---|---|---|\n| one | two | three |\n| four | five | six |\n\nAfter";
    for extent in [E::Row, E::Column, E::Cell] {
        let mut app = app(source, EditorMode::Live);
        if extent == E::Cell {
            for (row, column, extend) in [(2, 2, false), (1, 1, true)] {
                drop(app.update_grid(G::Select {
                    revision: std::sync::Arc::clone(&app.projection.source_id),
                    table: 0,
                    row,
                    column,
                    extend,
                    extent: E::Cell,
                }));
            }
        }
        let selection = app.selection;
        let original = app.projection.rich.tables[0]
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|r| app.projection.text[r.clone()].to_owned())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        context_menu(&mut app, 1, 1, extent);
        send(&mut app, Message::MenuAction(ContextAction::Clear));
        assert!(app.overlay.is_none());
        for (row, values) in original.iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                let cleared = match extent {
                    E::Row => row == 1,
                    E::Column => column == 1,
                    _ => row >= 1 && column >= 1,
                };
                let range = &app.projection.rich.tables[0].rows[row][column];
                assert_eq!(
                    &app.projection.text[range.clone()],
                    if cleared { "" } else { value }
                );
            }
        }
        assert!(app.document.source().starts_with("Before\n\n"));
        assert!(app.document.source().ends_with("\n\nAfter"));
        assert_synced(&app);
        drop(app.update_message(AppMessage::Undo));
        assert_eq!(app.document.source(), source);
        if extent == E::Cell {
            assert_eq!(app.selection, selection);
        }
        assert!(!app.document.can_undo());
    }
}

#[test]
fn context_menu_commands_and_alignment_check_targets_and_restore_one_step_history() {
    use crate::app::TableExtent as E;
    let source = "Before\n\n| A | B |\n|---|---|\n| café | other |\n\nAfter";
    for (extent, command) in [
        (E::Row, Command::RowAfter),
        (E::Row, Command::DeleteRow),
        (E::Column, Command::ColumnAfter),
        (E::Column, Command::DeleteColumn),
        (E::Column, Command::Align(Alignment::Right)),
    ] {
        let mut app = app(source, EditorMode::Live);
        context_menu(&mut app, 1, 1, extent);
        if matches!(command, Command::Align(_)) {
            send(&mut app, Message::MenuScope(MenuScope::Alignment));
        }
        send(
            &mut app,
            Message::MenuAction(ContextAction::Command(command)),
        );
        assert!(app.overlay.is_none());
        assert_ne!(app.document.source(), source);
        assert_synced(&app);
        drop(app.update_message(AppMessage::Undo));
        assert_eq!(app.document.source(), source);
        assert!(!app.document.can_undo());
    }
    for overlap in [false, true] {
        let mut app = app(source, EditorMode::Live);
        context_menu(&mut app, 1, 1, E::Column);
        let mut replica =
            crate::document::Document::from_state(&app.document.collaboration().state()).unwrap();
        let at = if overlap {
            source.find("other").unwrap()
        } else {
            0
        };
        replica
            .transact(
                &[SourceEdit {
                    range: at..at,
                    text: "new ".into(),
                }],
                EditOrigin::Human,
                None,
                true,
            )
            .unwrap();
        drop(app.update_message(AppMessage::ReplicaLoaded(Ok(
            replica.collaboration().state(),
        ))));
        let before = app.document.source().to_owned();
        send(&mut app, Message::MenuAction(ContextAction::Clear));
        if overlap {
            assert_eq!(app.document.source(), before);
            assert!(app.table.form.as_ref().unwrap().error.is_some());
        } else {
            assert!(app.document.source().starts_with("new Before"));
            assert!(app.overlay.is_none());
        }
    }
}

#[test]
fn context_menu_fits_window_edges_and_outside_click_cancels_without_history() {
    use iced::advanced::{Layout, Shell, clipboard, layout, mouse, widget};
    let source = "Before\n\n| A | B |\n|---|---|\n| one | two |\n\nAfter";
    let mut app = app(source, EditorMode::Live);
    let selection = app.selection;
    context_menu(&mut app, 1, 1, crate::app::TableExtent::Column);
    for size in [Size::new(760.0, 520.0), Size::new(300.0, 240.0)] {
        let menu = Size::new(236.0, 220.0);
        let point = menu_position(Point::new(740.0, 500.0), size, menu);
        assert!(point.x >= 8.0 && point.y >= 8.0);
        assert!(point.x + menu.width <= size.width - 8.0);
        assert!(point.y + menu.height <= size.height - 8.0);
    }
    let size = Size::new(760.0, 520.0);
    let renderer = iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(
        iced::Font::DEFAULT,
        18.0.into(),
    ));
    let mut root = app.view();
    let mut tree = widget::Tree::new(root.as_widget());
    let node =
        root.as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
    let mut messages = Vec::new();
    root.as_widget_mut().update(
        &mut tree,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Layout::new(&node),
        mouse::Cursor::Available(Point::new(12.0, 450.0)),
        &renderer,
        &mut clipboard::Null,
        &mut Shell::new(&mut messages),
        &iced::Rectangle::with_size(size),
    );
    drop(root);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, AppMessage::CloseOverlay))
    );
    for message in messages {
        let task = app.update_message(message);
        widget_effects(&mut app, &mut tree, &renderer, size, task);
    }
    assert!(app.overlay.is_none());
    assert!(app.editor_focused);
    assert_eq!(app.selection, selection);
    assert_eq!(app.document.source(), source);
    assert!(!app.document.can_undo());
}

#[test]
fn keyboard_menu_choices_are_visible_and_changing_groups_resets_scrolling() {
    use iced::advanced::{Layout, Shell, clipboard, layout, mouse, widget};
    use iced::{Rectangle, Renderer, Vector};

    struct Bounds {
        target: widget::Id,
        item: Option<Rectangle>,
        viewport: Option<(Rectangle, Vector)>,
    }
    impl widget::Operation for Bounds {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
            if id == Some(&self.target) {
                self.item = Some(bounds);
            }
        }
        fn scrollable(
            &mut self,
            id: Option<&widget::Id>,
            bounds: Rectangle,
            _content: Rectangle,
            translation: Vector,
            _state: &mut dyn widget::operation::Scrollable,
        ) {
            if id == Some(&widget::Id::from(MENU_SCROLL)) {
                self.viewport = Some((bounds, translation));
            }
        }
    }
    fn geometry(
        app: &App,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        size: Size,
        index: usize,
    ) -> (Rectangle, Rectangle, f32) {
        let mut bounds = Bounds {
            target: menu_item_id(index),
            item: None,
            viewport: None,
        };
        let mut root = app.view();
        tree.diff(root.as_widget());
        let node =
            root.as_widget_mut()
                .layout(tree, renderer, &layout::Limits::new(Size::ZERO, size));
        root.as_widget_mut()
            .operate(tree, Layout::new(&node), renderer, &mut bounds);
        let (viewport, translation) = bounds.viewport.unwrap();
        (bounds.item.unwrap() - translation, viewport, translation.y)
    }
    fn key(app: &mut App, tree: &mut widget::Tree, renderer: &Renderer, size: Size, key: Named) {
        let event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(key),
            modified_key: Key::Named(key),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        });
        let mut messages = Vec::new();
        let mut root = app.view();
        tree.diff(root.as_widget());
        let node =
            root.as_widget_mut()
                .layout(tree, renderer, &layout::Limits::new(Size::ZERO, size));
        let mut shell = Shell::new(&mut messages);
        root.as_widget_mut().update(
            tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard::Null,
            &mut shell,
            &Rectangle::with_size(size),
        );
        let status = shell.event_status();
        drop(root);
        let routed = event_message(event, status, window::Id::unique());
        for message in messages.into_iter().chain(routed) {
            let task = app.update_message(message);
            widget_effects(app, tree, renderer, size, task);
        }
    }
    fn visible(item: Rectangle, viewport: Rectangle) {
        assert!(
            item.y >= viewport.y - 1.0,
            "selected item starts above menu: {item:?} / {viewport:?}"
        );
        assert!(
            item.y + item.height <= viewport.y + viewport.height + 1.0,
            "selected item ends below menu: {item:?} / {viewport:?}"
        );
    }

    let source = "Before\n\n| A | B |\n|---|---|\n| one | two |\n\nAfter";
    let mut app = app(source, EditorMode::Live);
    context_menu(&mut app, 1, 1, crate::app::TableExtent::Column);
    let selected = app.selection;
    let size = Size::new(760.0, 240.0);
    let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
        iced::Font::DEFAULT,
        18.0.into(),
    ));
    let mut tree = widget::Tree::new(app.view().as_widget());
    let (last, viewport, offset) = geometry(&app, &mut tree, &renderer, size, 5);
    assert_eq!(offset, 0.0);
    assert!(last.y + last.height > viewport.y + viewport.height);
    key(&mut app, &mut tree, &renderer, size, Named::End);
    assert_eq!(app.table.form.as_ref().unwrap().selected, 5);
    let (last, viewport, offset) = geometry(&app, &mut tree, &renderer, size, 5);
    assert!(offset > 0.0);
    visible(last, viewport);
    key(&mut app, &mut tree, &renderer, size, Named::Home);
    visible(
        geometry(&app, &mut tree, &renderer, size, 0).0,
        geometry(&app, &mut tree, &renderer, size, 0).1,
    );
    key(&mut app, &mut tree, &renderer, size, Named::End);
    key(&mut app, &mut tree, &renderer, size, Named::ArrowUp);
    key(&mut app, &mut tree, &renderer, size, Named::ArrowUp);
    key(&mut app, &mut tree, &renderer, size, Named::ArrowRight);
    assert_eq!(
        app.table
            .form
            .as_ref()
            .unwrap()
            .menu
            .as_ref()
            .unwrap()
            .scope,
        MenuScope::Alignment
    );
    let (first, viewport, offset) = geometry(&app, &mut tree, &renderer, size, 0);
    assert_eq!(offset, 0.0);
    visible(first, viewport);
    key(&mut app, &mut tree, &renderer, size, Named::Escape);
    assert_eq!(app.table.form.as_ref().unwrap().selected, 3);
    let (trigger, viewport, _) = geometry(&app, &mut tree, &renderer, size, 3);
    visible(trigger, viewport);
    assert_eq!(app.selection, selected);
    assert_eq!(app.document.source(), source);
    assert!(!app.document.can_undo());
}

#[test]
fn losing_window_focus_dismisses_only_the_context_menu_without_changing_selection() {
    let source = "Before\n\n| A | B |\n|---|---|\n| one | two |\n\nAfter";
    for mode in [EditorMode::Live, EditorMode::Source] {
        let mut app = app(source, mode);
        app.selection = Selection::caret(source.find("one").unwrap());
        app.sync_editor();
        if mode == EditorMode::Live {
            context_menu(&mut app, 1, 1, crate::app::TableExtent::Column);
        } else {
            send(&mut app, Message::Open);
        }
        let selected = app.selection;
        let message = event_message(
            Event::Window(window::Event::Unfocused),
            event::Status::Ignored,
            window::Id::unique(),
        )
        .unwrap();
        drop(app.update_message(message));
        assert_eq!(app.overlay.is_none(), mode == EditorMode::Live);
        assert_eq!(app.selection, selected);
        assert_eq!(app.document.source(), source);
        assert!(!app.document.can_undo());
    }
}

#[test]
fn spaces_stay_in_the_current_cell_until_tab_moves_to_the_next_cell() {
    for (text, value) in [
        ("one two three", "one two three"),
        ("  café 🧑‍💻  ", "&#32;&#32;café 🧑‍💻&#32;&#32;"),
    ] {
        let source = "Before\n\n| A | B |\n|---|---|\n|  | untouched |\n\nAfter";
        let mut app = app(source, EditorMode::Live);
        let point = app.projection.rich.tables[0].source_rows[1][0].start;
        app.selection = Selection::caret(point);
        app.sync_editor();
        for character in text.chars() {
            drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert(character)))));
            assert_eq!(
                table_edit::cell_at(&app.projection, app.selection)
                    .map(|(_, row, column)| (row, column)),
                Some((1, 0)),
                "after {character:?}: {:?} in {:?}",
                app.selection,
                app.document.source()
            );
        }
        let mut expected = source.to_owned();
        expected.insert_str(point, value);
        assert_eq!(app.document.source(), expected);
        send(&mut app, Message::Open);
        drop(app.update(AppMessage::CloseOverlay));
        drop(app.update(AppMessage::TableTab(false)));
        assert_eq!(
            table_edit::cell_at(&app.projection, app.selection)
                .map(|(_, row, column)| (row, column)),
            Some((1, 1))
        );
    }
}

#[test]
fn backspace_removes_a_typed_cell_space_without_deleting_text_or_moving_cells() {
    for source in [
        "| A | B |\n|---|---|\n|  | right |\n",
        "| A | B |\n|---|---|\n| word | right |\n",
        "| A | B |\n|---|---|\n| left | word |\n",
    ] {
        let mut app = app(source, EditorMode::Live);
        let column = usize::from(source.contains("| left |"));
        let point = app.projection.rich.tables[0].source_rows[1][column].end;
        app.selection = Selection::caret(point);
        app.sync_editor();
        for count in 1..=3 {
            drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert(' ')))));
            let mut expected = source.to_owned();
            expected.insert_str(point, &"&#32;".repeat(count));
            assert_eq!(app.document.source(), expected);
            assert_eq!(app.selection, Selection::caret(point + count * 5));
        }
        for count in (0..3).rev() {
            drop(app.update(AppMessage::Edit(Action::Edit(Edit::Backspace))));
            let mut expected = source.to_owned();
            expected.insert_str(point, &"&#32;".repeat(count));
            assert_eq!(app.document.source(), expected);
            assert_eq!(app.selection, Selection::caret(point + count * 5));
            assert_eq!(
                table_edit::cell_at(&app.projection, app.selection)
                    .map(|(_, row, column)| (row, column)),
                Some((1, column))
            );
        }
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, Selection::caret(point));
    }
}

#[test]
fn active_cell_displays_typed_spaces_without_revealing_existing_padding() {
    for ending in ["\n", "\r\n", "\r"] {
        for value in ["", "café 🧑‍💻"] {
            let source = format!(
                "Before{ending}{ending}| A | B |{ending}|---|---|{ending}|     {value}     | untouched |{ending}{ending}After"
            );
            let mut app = app(&source, EditorMode::Live);
            let cell = app.projection.rich.tables[0].source_rows[1][0].clone();
            app.selection = Selection::caret(cell.end);
            app.sync_editor();
            let visible = app.projection.rich.tables[0].rows[1][0].clone();
            assert_eq!(&app.projection.text[visible], value);
            for count in 1..=3 {
                drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert(' ')))));
                let display = app.projection.display_selection(app.selection);
                let visible = app.projection.rich.tables[0].rows[1][0].clone();
                assert_eq!(
                    &app.projection.text[visible.clone()],
                    format!("{value}{}", " ".repeat(count))
                );
                assert_eq!(display.head, visible.end);
                assert_eq!(app.editor.selection(), display);
                assert_synced(&app);
                assert_mappings(&app);
            }
            let mut expected = source.clone();
            expected.insert_str(cell.end, "&#32;&#32;&#32;");
            assert_eq!(app.document.source(), expected);
            drop(app.update(AppMessage::Undo));
            assert_eq!(app.document.source(), source);
            assert_eq!(app.selection, Selection::caret(cell.end));
            drop(app.update(AppMessage::Redo));
            assert_eq!(app.document.source(), expected);
            let visible = app.projection.rich.tables[0].rows[1][0].clone();
            assert_eq!(&app.projection.text[visible], format!("{value}   "));
            drop(app.update(AppMessage::TableTab(false)));
            assert_eq!(app.document.source(), expected);
            let visible = app.projection.rich.tables[0].rows[1][0].clone();
            assert_eq!(&app.projection.text[visible], format!("{value}   "));
            drop(app.update(AppMessage::Undo));
            assert_eq!(
                app.document.source(),
                source,
                "Tab and disclosure must add no history"
            );
        }
    }
}

#[test]
fn replacing_a_cell_value_keeps_original_right_padding_concealed() {
    for source in [
        "| A | B |\n|---|---|\n|     old     | right |\n",
        "A | B\n---|---\nleft |     old     ",
        "| A | B |\n|---|---|\n| left |     old     |",
    ] {
        let mut app = app(source, EditorMode::Live);
        let column = usize::from(source.contains("left"));
        let cell = app.projection.rich.tables[0].source_rows[1][column].clone();
        app.selection = Selection {
            anchor: cell.start,
            head: cell.end,
        };
        app.sync_editor();
        drop(app.update(AppMessage::Edit(Action::Edit(Edit::Backspace))));
        for count in 0..=3 {
            let visible = app.projection.rich.tables[0].rows[1][column].clone();
            assert_eq!(&app.projection.text[visible], " ".repeat(count));
            assert_synced(&app);
            if count < 3 {
                drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert(' ')))));
            }
        }
        assert_eq!(
            app.document.source(),
            source.replacen("old", "&#32;&#32;&#32;", 1)
        );
    }
}

#[test]
fn modified_tab_chords_do_not_indent_table_text_or_move_to_another_cell() {
    use crate::app::application::editor_binding;
    use iced::widget::text_editor::{KeyPress, Status};
    for modifiers in [
        Modifiers::CTRL,
        Modifiers::ALT,
        Modifiers::COMMAND | Modifiers::SHIFT,
    ] {
        assert!(
            editor_binding(KeyPress {
                key: Key::Named(Named::Tab),
                modified_key: Key::Named(Named::Tab),
                physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Tab),
                modifiers,
                text: None,
                status: Status::Focused { is_hovered: false },
            })
            .is_none()
        );
    }
}

#[test]
fn word_table_tab_replaces_contents_and_boundary_deletion_does_not_navigate() {
    let source = "Before\n\n| A | B |\n|---|---|\n| first | **last** |\n\nAfter";
    let mut app = app(source, EditorMode::Live);
    app.selection = Selection::caret(source.find("first").unwrap());
    app.sync_editor();
    drop(app.update(AppMessage::TableTab(false)));
    assert_eq!(&app.document.source()[app.selection.range()], "last");
    for character in "new".chars() {
        drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert(character)))));
    }
    assert!(app.document.source().contains("**new**"));
    drop(app.update(AppMessage::Undo));
    assert_eq!(app.document.source(), source);
    assert_eq!(&app.document.source()[app.selection.range()], "last");
    for (end, edit) in [(false, Edit::Backspace), (true, Edit::Delete)] {
        let cell = app.projection.rich.tables[0].source_rows[1][0].clone();
        let before = Selection::caret(if end { cell.end } else { cell.start });
        app.selection = before;
        app.sync_editor();
        drop(app.update(AppMessage::Edit(Action::Edit(edit))));
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, before);
    }
}

#[test]
fn cell_enter_and_shift_enter_keep_styled_text_in_the_same_cell_and_undo_once() {
    for value in ["plain", "**strong**", "`code`"] {
        for action in [Edit::Enter, Edit::Paste(std::sync::Arc::new("\n".into()))] {
            let source = format!("Before\n\n| A | B |\n|---|---|\n| {value} | right |\n\nAfter");
            let mut app = app(&source, EditorMode::Live);
            let word = if value.contains("strong") {
                "strong"
            } else if value.contains("code") {
                "code"
            } else {
                "plain"
            };
            app.selection = Selection::caret(source.find(word).unwrap() + 2);
            app.sync_editor();
            let before = app.selection;
            drop(app.update(AppMessage::Edit(Action::Edit(action))));
            let cell = app.projection.rich.tables[0].rows[1][0].clone();
            let expected = format!("{}\n{}", &word[..2], &word[2..]);
            assert_eq!(&app.projection.text[cell], expected);
            assert_eq!(app.projection.rich.tables[0].rows.len(), 2);
            assert_eq!(
                table_edit::cell_at(&app.projection, app.selection)
                    .map(|(_, row, column)| (row, column)),
                Some((1, 0))
            );
            assert_synced(&app);
            drop(app.update(AppMessage::Undo));
            assert_eq!(app.document.source(), source);
            assert_eq!(app.selection, before);
        }
    }
}

#[test]
fn typing_over_multiple_cells_is_one_undo_group_and_preserves_separator_bytes() {
    for reverse in [false, true] {
        let source = "Before\n\nA | B\n:---|---:\nleft | right\n\nAfter";
        let mut app = app(source, EditorMode::Live);
        let start = source.find('A').unwrap();
        let end = source.find("right").unwrap() + 5;
        let before = if reverse {
            Selection {
                anchor: end,
                head: start,
            }
        } else {
            Selection {
                anchor: start,
                head: end,
            }
        };
        app.selection = before;
        app.sync_editor();
        for character in "new".chars() {
            drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert(character)))));
        }
        assert_eq!(app.projection.rich.tables.len(), 1);
        assert_eq!(app.projection.rich.tables[0].rows.len(), 2);
        assert!(app.document.source().contains(":---|---:"));
        assert_synced(&app);
        drop(app.update(AppMessage::Undo));
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, before);
        assert!(!app.document.can_undo());
    }
}

#[test]
fn document_selections_cross_table_boundaries_without_absorbing_or_destroying_blocks() {
    for ending in ["\n", "\r\n", "\r"] {
        for prefix in ["", "> "] {
            let source = format!(
                "Before words.{ending}{ending}{prefix}| A | B |{ending}{prefix}|---|---|{ending}{prefix}| left | right |{ending}{ending}After words."
            );
            for (start, end, expected_before, expected_after) in [
                (2, source.find("left").unwrap() + 2, "Bex", "After words."),
                (
                    source.find('A').unwrap(),
                    source.rfind("After").unwrap() + 2,
                    "Before words.",
                    "ter words.",
                ),
            ] {
                let mut app = app(&source, EditorMode::Live);
                let before = Selection {
                    anchor: end,
                    head: start,
                };
                app.selection = before;
                app.sync_editor();
                drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert('x')))));
                assert_eq!(
                    app.projection.rich.tables.len(),
                    1,
                    "{}",
                    app.document.source()
                );
                assert_eq!(
                    app.projection.rich.tables[0].rows.len(),
                    2,
                    "{}",
                    app.document.source()
                );
                assert!(app.document.source().starts_with(expected_before));
                assert!(app.document.source().ends_with(expected_after));
                assert_synced(&app);
                drop(app.update(AppMessage::Undo));
                assert_eq!(app.document.source(), source);
                assert_eq!(app.selection, before);
            }
        }
    }
}

#[test]
fn contextual_commands_are_repeatable_and_right_click_cancel_preserves_source_and_selection() {
    let source = "Before\n\n| A | B | C |\n|---|---|---|\n| short |\n\nAfter";
    let mut app = app(source, EditorMode::Live);
    let before = Selection { anchor: 6, head: 0 };
    app.selection = before;
    app.sync_editor();
    let revision = std::sync::Arc::clone(&app.projection.source_id);
    send(
        &mut app,
        Message::FocusCell {
            revision,
            table: 0,
            row: 1,
            column: 1,
            selection: Selection::caret(source.find("short").unwrap() + 5),
            open: true,
        },
    );
    assert_eq!(app.selection, before);
    assert_eq!(app.document.source(), source);
    drop(app.update(AppMessage::CloseOverlay));
    assert!(!app.document.can_undo());
    app.selection = Selection::caret(source.find("short").unwrap());
    app.sync_editor();
    for _ in 0..2 {
        send(
            &mut app,
            Message::Context(ContextAction::Command(Command::RowAfter)),
        );
        assert!(app.overlay.is_none());
    }
    assert_eq!(app.projection.rich.tables[0].rows.len(), 4);
    drop(app.update(AppMessage::Undo));
    assert_eq!(app.projection.rich.tables[0].rows.len(), 3);
    drop(app.update(AppMessage::Undo));
    assert_eq!(app.document.source(), source);
}

#[test]
fn entering_short_row_cells_materializes_only_that_row_and_ignores_stale_pointer_targets() {
    let source = "| A | B | C |\n|---|---|---|\n| short |\n| other |\n";
    let mut app = app(source, EditorMode::Live);
    let revision = std::sync::Arc::clone(&app.projection.source_id);
    send(
        &mut app,
        Message::FocusCell {
            revision: revision.clone(),
            table: 0,
            row: 1,
            column: 1,
            selection: Selection::caret(source.find("short").unwrap() + 5),
            open: false,
        },
    );
    assert_eq!(
        app.document.source(),
        "| A | B | C |\n|---|---|---|\n| short |  |\n| other |\n"
    );
    assert_eq!(
        table_edit::cell_at(&app.projection, app.selection).map(|(_, row, column)| (row, column)),
        Some((1, 1))
    );
    let after = app.document.source().to_owned();
    send(
        &mut app,
        Message::FocusCell {
            revision,
            table: 0,
            row: 2,
            column: 2,
            selection: Selection::caret(0),
            open: false,
        },
    );
    assert_eq!(app.document.source(), after);
    drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert('x')))));
    assert!(app.document.source().contains("| short | x |"));
    assert_synced(&app);
}

#[test]
fn row_selection_clear_and_leave_table_are_explicit_undoable_actions() {
    let source = "Before\r\n\r\nA | B\r\n---|---\r\nleft | right\r\n\r\nAfter";
    let mut app = app(source, EditorMode::Live);
    app.selection = Selection::caret(source.find("left").unwrap());
    app.sync_editor();
    send(&mut app, Message::Context(ContextAction::SelectRow));
    let selected = app.selection;
    send(&mut app, Message::Context(ContextAction::Clear));
    assert_eq!(app.projection.rich.tables[0].rows.len(), 2);
    assert!(
        app.projection.rich.tables[0].rows[1]
            .iter()
            .all(|cell| cell.is_empty())
    );
    drop(app.update(AppMessage::Undo));
    assert_eq!(app.document.source(), source);
    assert_eq!(app.selection, selected);
    send(&mut app, Message::Exit);
    assert!(table_edit::cell_at(&app.projection, app.selection).is_none());
    drop(app.update(AppMessage::Undo));
    assert_eq!(app.document.source(), source);
    assert_eq!(app.selection, selected);
}

#[test]
fn active_cell_spaces_keep_incremental_updates_and_survive_unrelated_imports() {
    use std::sync::Arc;
    let source = format!(
        "{}| A | B |\n|---|---|\n| word | right |\n\n{}",
        "Before paragraph.\n\n".repeat(150),
        "After paragraph.\n\n".repeat(150)
    );
    let mut app = app(&source, EditorMode::Live);
    let point = app.projection.rich.tables[0].source_rows[1][0].end;
    app.selection = Selection::caret(point);
    app.sync_editor();
    for _ in 0..3 {
        let previous = Arc::clone(&app.projection.highlights);
        let length = app.projection.text.len();
        drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert(' ')))));
        let change = app
            .projection
            .change
            .as_ref()
            .expect("a certified table update");
        assert!(Arc::ptr_eq(&previous, &change.previous));
        assert!(
            change
                .regions
                .iter()
                .all(|region| region.old.len() < length / 4)
        );
        assert_synced(&app);
        assert_mappings(&app);
    }
    let mut peer =
        crate::document::Document::from_state(&app.document.collaboration().state()).unwrap();
    let after = peer.source().rfind("After").unwrap();
    peer.transact(
        &[
            SourceEdit {
                range: 0..6,
                text: "Changed before".into(),
            },
            SourceEdit {
                range: after..after + 5,
                text: "Changed after".into(),
            },
        ],
        EditOrigin::Agent,
        None,
        true,
    )
    .unwrap();
    let previous = Arc::clone(&app.projection.highlights);
    drop(app.update(AppMessage::ReplicaLoaded(Ok(peer.collaboration().state()))));
    assert_eq!(app.document.block_changes().len(), 2);
    assert!(Arc::ptr_eq(
        &previous,
        &app.projection.change.as_ref().unwrap().previous
    ));
    let cell = app.projection.rich.tables[0].rows[1][0].clone();
    assert_eq!(&app.projection.text[cell], "word   ");
    assert_synced(&app);
    assert_mappings(&app);
    send(&mut app, Message::Open);
    send(&mut app, Message::Apply(Command::ColumnBefore));
    let cell = app.projection.rich.tables[0].rows[1][1].clone();
    assert_eq!(&app.projection.text[cell], "word   ");
    assert_synced(&app);
}

#[test]
fn every_table_command_has_one_undo_step_and_restores_reverse_source_selection() {
    let source = include_str!("../../tests/fixtures/table-controls.md").replace('\n', "\r\n");
    for mode in [EditorMode::Live, EditorMode::Source] {
        for (command, _) in COMMANDS {
            let mut app = app(&source, mode);
            let head = source.find("café").unwrap();
            let selection = Selection {
                anchor: head + "café".len(),
                head,
            };
            app.selection = selection;
            app.sync_editor();
            send(&mut app, Message::Open);
            assert!(matches!(app.overlay, Some(Overlay::Table)));
            send(&mut app, Message::Apply(command));
            assert!(app.overlay.is_none());
            if app.document.source() == source {
                assert!(!app.document.can_undo());
                assert!(!app.dirty);
                continue;
            }
            assert!(app.dirty);
            assert_synced(&app);
            assert_mappings(&app);
            let changed = app.document.source().to_owned();
            let after = app.selection;
            drop(app.update(AppMessage::Undo));
            assert_eq!(app.document.source(), source, "{command:?}");
            assert_eq!(app.selection, selection, "{command:?}");
            assert!(!app.document.can_undo());
            assert_synced(&app);
            assert_mappings(&app);
            drop(app.update(AppMessage::Redo));
            assert_eq!(app.document.source(), changed);
            assert_eq!(app.selection, after);
            assert_synced(&app);
            assert_mappings(&app);
        }
    }
}

#[test]
fn table_dialog_rejects_changed_targets_and_tracks_unrelated_replica_edits() {
    for overlap in [false, true] {
        let source = "Before\n\n| A | B |\n|---|---|\n| café | other |\n\nAfter";
        let mut app = app(source, EditorMode::Live);
        app.selection = Selection::caret(source.find("café").unwrap());
        app.sync_editor();
        send(&mut app, Message::Open);
        let mut replica =
            crate::document::Document::from_state(&app.document.collaboration().state()).unwrap();
        let at = if overlap {
            source.find("other").unwrap()
        } else {
            0
        };
        replica
            .transact(
                &[SourceEdit {
                    range: at..at,
                    text: "new ".into(),
                }],
                EditOrigin::Human,
                None,
                true,
            )
            .unwrap();
        drop(app.update(AppMessage::ReplicaLoaded(Ok(
            replica.collaboration().state(),
        ))));
        let before = app.document.source().to_owned();
        send(&mut app, Message::Apply(Command::ColumnAfter));
        if overlap {
            assert_eq!(app.document.source(), before);
            assert!(app.table.form.as_ref().unwrap().error.is_some());
        } else {
            assert!(app.document.source().starts_with("new Before"));
            assert_eq!(app.projection.rich.tables[0].alignment.len(), 3);
        }
    }
}

#[test]
fn table_keyboard_dialog_and_guards_keep_ordinary_editing_available() {
    let source = "| A | B |\n|---|---|\n| text | other |\n";
    let mut app = app(source, EditorMode::Live);
    app.selection = Selection::caret(source.find("text").unwrap());
    app.sync_editor();
    let open = shortcut(
        &Key::Character("t".into()),
        Modifiers::COMMAND | Modifiers::SHIFT,
    )
    .unwrap();
    assert!(matches!(open, AppMessage::Table(Message::OpenMenu)));
    send(&mut app, Message::Open);
    for (key, modifiers) in [
        (Named::Tab, Modifiers::empty()),
        (Named::Tab, Modifiers::SHIFT),
        (Named::ArrowDown, Modifiers::empty()),
        (Named::ArrowUp, Modifiers::empty()),
    ] {
        let message = event_message(
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(key),
                modified_key: Key::Named(key),
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            }),
            event::Status::Captured,
            window::Id::unique(),
        )
        .unwrap();
        drop(app.update(message));
    }
    assert_eq!(app.table.form.as_ref().unwrap().selected, 1);
    send(
        &mut app,
        Message::Key {
            key: Named::Enter,
            shift: false,
        },
    );
    assert_eq!(app.projection.rich.tables[0].rows.len(), 3);
    drop(app.update(AppMessage::Undo));
    assert_eq!(app.document.source(), source);
    for (loading, overlay, mode) in [
        (true, None, EditorMode::Live),
        (false, Some(Overlay::Settings), EditorMode::Live),
        (false, None, EditorMode::Read),
    ] {
        app.loading = loading;
        app.overlay = overlay;
        app.mode = mode;
        send(&mut app, Message::Open);
        send(&mut app, Message::Apply(Command::DeleteColumn));
        assert_eq!(app.document.source(), source);
    }
    app.loading = false;
    app.overlay = None;
    app.mode = EditorMode::Live;
    drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert('!')))));
    assert!(app.document.source().contains("!text"));
}

#[test]
fn table_commands_keep_typing_separate_and_cancel_restores_focus_without_history() {
    let source = "Before\n\n| A | B |\n|---|---|\n| text | other |\n\nAfter";
    let mut app = app(source, EditorMode::Live);
    app.selection = Selection::caret(source.find("text").unwrap());
    app.sync_editor();
    drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert('!')))));
    let typed = app.document.source().to_owned();
    send(&mut app, Message::Open);
    send(&mut app, Message::Apply(Command::ColumnAfter));
    let structural = app.document.source().to_owned();
    drop(app.update(AppMessage::Edit(Action::Edit(Edit::Insert('?')))));
    drop(app.update(AppMessage::Undo));
    assert_eq!(app.document.source(), structural);
    drop(app.update(AppMessage::Undo));
    assert_eq!(app.document.source(), typed);
    send(&mut app, Message::Open);
    app.editor_focused = false;
    let selection = app.selection;
    drop(app.update(AppMessage::CloseOverlay));
    assert!(app.editor_focused);
    assert_eq!(app.selection, selection);
    assert_eq!(app.document.source(), typed);
    drop(app.update(AppMessage::Undo));
    assert_eq!(app.document.source(), source);
}

#[test]
fn table_controls_receive_real_live_and_source_widget_keyboard_events() {
    use iced::advanced::{Layout, Shell, clipboard, layout, mouse, widget};
    use iced::{Rectangle, Renderer, Size};

    fn press(key: Key, modifiers: Modifiers) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    }
    let source = "Before\n\n| A | B |\n|---|---|\n| café | other |\n\nAfter";
    for mode in [EditorMode::Live, EditorMode::Source] {
        let mut app = app(source, mode);
        app.selection = Selection::caret(source.find("café").unwrap());
        app.sync_editor();
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::DEFAULT,
            18.0.into(),
        ));
        let size = Size::new(760.0, 520.0);
        let mut tree = widget::Tree::new(app.view().as_widget());
        {
            let mut root = app.view();
            let node = root.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            root.as_widget_mut().operate(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &mut widget::operation::focusable::focus::<()>(widget::Id::from(EDITOR)),
            );
        }
        let mut dispatch = |app: &mut App, event: Event| {
            let table = matches!(app.overlay, Some(Overlay::Table));
            let mut messages = Vec::new();
            let status;
            {
                let mut root = app.view();
                tree.diff(root.as_widget());
                let node = root.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, size),
                );
                let mut shell = Shell::new(&mut messages);
                root.as_widget_mut().update(
                    &mut tree,
                    &event,
                    Layout::new(&node),
                    mouse::Cursor::Unavailable,
                    &renderer,
                    &mut clipboard::Null,
                    &mut shell,
                    &Rectangle::with_size(size),
                );
                status = shell.event_status();
            }
            let routed = if table {
                event_message(event, status, window::Id::unique())
            } else {
                super::super::event_message(event, status, window::Id::unique())
            };
            for message in messages.into_iter().chain(routed) {
                let task = app.update_message(message);
                widget_effects(app, &mut tree, &renderer, size, task);
            }
            status
        };
        dispatch(
            &mut app,
            press(
                Key::Character("t".into()),
                Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        );
        assert!(matches!(app.overlay, Some(Overlay::Table)));
        if mode == EditorMode::Live {
            assert!(app.table.is_menu());
            dispatch(
                &mut app,
                press(Key::Named(Named::ArrowDown), Modifiers::empty()),
            );
            dispatch(
                &mut app,
                press(Key::Named(Named::ArrowRight), Modifiers::empty()),
            );
            dispatch(&mut app, press(Key::Named(Named::End), Modifiers::empty()));
            assert_eq!(app.table.form.as_ref().unwrap().selected, 5);
        } else {
            for _ in 0..4 {
                let status = dispatch(
                    &mut app,
                    press(Key::Named(Named::ArrowDown), Modifiers::empty()),
                );
                assert_eq!(status, event::Status::Ignored);
            }
            assert_eq!(app.table.form.as_ref().unwrap().selected, 5);
        }
        dispatch(
            &mut app,
            press(Key::Named(Named::Enter), Modifiers::empty()),
        );
        assert!(app.overlay.is_none());
        assert_eq!(app.projection.rich.tables[0].alignment.len(), 1);
        assert_synced(&app);
        drop(app.update(AppMessage::Undo));
        assert_eq!(app.document.source(), source);
        send(&mut app, Message::Open);
        dispatch(
            &mut app,
            press(Key::Named(Named::Escape), Modifiers::empty()),
        );
        assert!(app.overlay.is_none());
        assert!(app.editor_focused);
        assert!(!app.document.can_undo());
    }
}

#[test]
fn header_moves_and_removing_the_final_row_or_column_are_single_undo_groups() {
    for source in [
        "Before\n\n| Header | Other |\n|---|---|\n| café | 中文 |\n\nAfter",
        "Before\n\n| Header |\n|---|\n\nAfter",
    ] {
        for mode in [EditorMode::Live, EditorMode::Source] {
            for command in [
                Command::RowBefore,
                Command::RowAfter,
                Command::DeleteRow,
                Command::DeleteColumn,
            ] {
                let mut app = app(source, mode);
                let head = source.find("Header").unwrap();
                let before = Selection {
                    anchor: head + 6,
                    head,
                };
                app.selection = before;
                app.sync_editor();
                send(&mut app, Message::Open);
                send(&mut app, Message::Apply(command));
                assert!(app.overlay.is_none());
                assert_synced(&app);
                assert_mappings(&app);
                let changed = app.document.source().to_owned();
                drop(app.update(AppMessage::Undo));
                assert_eq!(app.document.source(), source, "{command:?}");
                assert_eq!(app.selection, before);
                assert!(!app.document.can_undo());
                drop(app.update(AppMessage::Redo));
                assert_eq!(app.document.source(), changed);
                assert_synced(&app);
            }
        }
    }
}

#[test]
fn large_column_batches_preserve_selections_remote_cells_and_one_step_history() {
    let source = format!(
        "# Before\r\n\r\n| Name | Detail | Count |\r\n|:---|:---:|---:|\r\n{}\r\n![alt](missing.png)\r\n\r\nAfter **unchanged**.",
        "| café🧑‍💻 | **中文** | 12 |\r\n".repeat(512)
    );
    for mode in [EditorMode::Live, EditorMode::Source] {
        for command in [
            Command::ColumnBefore,
            Command::ColumnAfter,
            Command::DeleteColumn,
        ] {
            let mut app = app(&source, mode);
            let head = source.match_indices("中文").nth(256).unwrap().0;
            let before = Selection {
                anchor: head + "中文".len(),
                head,
            };
            app.selection = before;
            app.sync_editor();
            let count = source.rfind("12").unwrap();
            let stable = app.document.collaboration().selection(Selection {
                anchor: count + 2,
                head: count,
            });
            let mut peer =
                crate::document::Document::from_state(&app.document.collaboration().state())
                    .unwrap();
            peer.replace(count..count + 2, "peer").unwrap();
            send(&mut app, Message::Open);
            send(&mut app, Message::Apply(command));
            assert!(app.overlay.is_none());
            let after = app.selection;
            let selected = app
                .document
                .collaboration()
                .resolve_selection(&stable)
                .unwrap();
            assert_eq!(&app.document.source()[selected.range()], "12");
            if command != Command::DeleteColumn {
                assert!(after.anchor > after.head);
                assert_eq!(&app.document.source()[after.range()], "中文");
            }
            assert_synced(&app);
            assert_mappings(&app);
            drop(app.update(AppMessage::Undo));
            assert_eq!(app.document.source(), source);
            assert_eq!(app.selection, before);
            assert!(!app.document.can_undo());
            drop(app.update(AppMessage::Redo));
            assert_eq!(app.selection, after);
            drop(app.update(AppMessage::ReplicaLoaded(Ok(peer.collaboration().state()))));
            assert!(app.document.source().contains("peer"));
            assert!(app.document.source().starts_with("# Before\r\n\r\n"));
            assert!(
                app.document
                    .source()
                    .ends_with("![alt](missing.png)\r\n\r\nAfter **unchanged**.")
            );
            assert_synced(&app);
            drop(app.update(AppMessage::Undo));
            let mut expected = source.clone();
            expected.replace_range(count..count + 2, "peer");
            assert_eq!(app.document.source(), expected);
            assert_eq!(app.selection, before);
            assert_synced(&app);
        }
    }
}

#[test]
#[ignore = "release-only table command measurements; excludes rendering and native presentation"]
fn performance_table_controls() {
    use std::time::Instant;

    let sample_count = std::env::var("DOSTY_TABLE_SAMPLES")
        .map_or(30, |value| value.parse::<usize>().unwrap().max(1));
    for rows in [1_000, 5_000] {
        let source = format!(
            "Before\n\n| Name | Detail | Count |\n|:---|:---:|---:|\n{}\nAfter",
            "| café | **bold** | 12 |\n".repeat(rows)
        );
        for command in [
            Command::RowAfter,
            Command::ColumnAfter,
            Command::DeleteColumn,
            Command::Align(Alignment::Right),
        ] {
            let mut samples = Vec::new();
            let mut crdt = Vec::new();
            let mut source_cost = Vec::new();
            for _ in 0..sample_count {
                let mut app = app(&source, EditorMode::Live);
                app.selection = Selection::caret(source.find("café").unwrap());
                app.sync_editor();
                let start = Instant::now();
                send(&mut app, Message::Open);
                send(&mut app, Message::Apply(command));
                samples.push(start.elapsed().as_secs_f64() * 1_000.0);
                crdt.push(app.document.costs.crdt.as_secs_f64() * 1_000.0);
                source_cost.push(app.document.costs.source.as_secs_f64() * 1_000.0);
                assert!(app.overlay.is_none());
                assert_synced(&app);
                drop(app.update(AppMessage::Undo));
                assert_eq!(app.document.source(), source);
            }
            samples.sort_by(f64::total_cmp);
            crdt.sort_by(f64::total_cmp);
            source_cost.sort_by(f64::total_cmp);
            let middle = sample_count / 2;
            let tail = (sample_count * 95 / 100).min(sample_count - 1);
            println!(
                "table rows={rows} command={command:?} samples={sample_count} ms p50={:.3} p95={:.3} max={:.3} crdt_p50={:.3} source_p50={:.3}",
                samples[middle],
                samples[tail],
                samples[sample_count - 1],
                crdt[middle],
                source_cost[middle]
            );
        }
    }
}
