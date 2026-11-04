use super::*;
use crate::io::table_clipboard::{Producer, Snapshot};

fn command(h: &mut Harness, key: &str) {
    h.key(Key::Character(key.into()), Modifiers::COMMAND, None);
}

fn range(h: &mut Harness, anchor: (usize, usize), head: (usize, usize)) {
    h.focus_cell(0, anchor.0, anchor.1);
    let task = h.app.update_message(AppMessage::TableGrid(Message::Select {
        revision: Arc::clone(&h.app.projection.source_id),
        table: 0,
        row: head.0,
        column: head.1,
        extend: true,
        extent: Extent::Cell,
    }));
    h.effects(task);
    h.frame();
}

fn rectangle(h: &Harness, start: (usize, usize), height: usize, width: usize) -> Vec<Vec<String>> {
    (start.0..start.0 + height)
        .map(|row| {
            (start.1..start.1 + width)
                .map(|column| value(&h.app, row, column).into())
                .collect()
        })
        .collect()
}

fn deliver(h: &mut Harness, message: Message) {
    let task = h.app.update_message(AppMessage::TableGrid(message));
    h.effects(task);
    h.frame();
}

fn batched_commands(h: &mut Harness, keys: &[&str]) {
    let keys = keys
        .iter()
        .map(|key| (Key::Character((*key).into()), Modifiers::COMMAND))
        .collect::<Vec<_>>();
    batched_keys(h, &keys);
}

fn batched_keys(h: &mut Harness, keys: &[(Key, Modifiers)]) {
    let mut messages = Vec::new();
    {
        let mut root = h.app.editor_view(false);
        h.tree.diff(root.as_widget());
        let node = root.as_widget_mut().layout(
            &mut h.tree,
            &h.renderer,
            &layout::Limits::new(Size::ZERO, h.size),
        );
        // Iced delivers a queued event batch against one view, then updates
        // the app with the resulting messages after every event is handled.
        for (key, modifiers) in keys {
            let event = Event::Keyboard(keyboard::Event::KeyPressed {
                modified_key: key.clone(),
                key: key.clone(),
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers: *modifiers,
                text: None,
                repeat: false,
            });
            let mut shell = Shell::new(&mut messages);
            root.as_widget_mut().update(
                &mut h.tree,
                &event,
                Layout::new(&node),
                mouse::Cursor::Available(Point::new(620.0, 540.0)),
                &h.renderer,
                &mut h.clipboard,
                &mut shell,
                &Rectangle::with_size(h.size),
            );
            assert_eq!(shell.event_status(), iced::event::Status::Captured);
        }
    }
    let tasks = messages
        .into_iter()
        .map(|message| h.app.update_message(message))
        .collect::<Vec<_>>();
    for task in tasks {
        h.effects(task);
    }
    h.frame();
}

#[test]
fn batched_copy_move_and_paste_read_the_new_clipboard_at_the_new_cell() {
    let source = "| A | B |\n|---|---|\n| new copy | destination |\n";
    let mut h = Harness::new(source);
    h.focus_cell(0, 1, 0);
    h.clipboard.standard = Some("old clipboard".into());
    batched_keys(
        &mut h,
        &[
            (Key::Character("c".into()), Modifiers::COMMAND),
            (Key::Named(Named::ArrowRight), Modifiers::empty()),
            (Key::Character("v".into()), Modifiers::COMMAND),
        ],
    );
    assert_eq!(h.clipboard.standard.as_deref(), Some("new copy"));
    assert_eq!(value(&h.app, 1, 0), "new copy");
    assert_eq!(value(&h.app, 1, 1), "new copy");
    let view = h.app.grid_view().unwrap();
    assert_eq!(
        (view.anchor, view.head, view.active),
        ((1, 1), (1, 1), (1, 1))
    );
    synced(&h.app);
    command(&mut h, "z");
    assert_eq!(h.app.document.source(), source);
    assert!(!h.app.document.can_undo());
    synced(&h.app);
}

#[test]
fn batched_select_all_and_copy_or_cut_use_the_new_table_selection() {
    for cut in [false, true] {
        let mut h = Harness::new(SOURCE);
        h.focus_cell(0, 0, 0);
        let table = &h.app.projection.rich.tables[0];
        let (rows, columns) = (table.rows.len(), table.alignment.len());
        let expected =
            crate::document::table_clipboard::encode(&rectangle(&h, (0, 0), rows, columns));
        batched_commands(&mut h, &["a", if cut { "x" } else { "c" }]);
        assert_eq!(h.clipboard.standard.as_deref(), Some(expected.as_str()));
        let selected = h.app.grid_view().unwrap();
        assert_eq!(
            (selected.anchor, selected.head),
            ((0, 0), (rows - 1, columns - 1))
        );
        if cut {
            assert_eq!(
                rectangle(&h, (0, 0), rows, columns),
                vec![vec![""; columns]; rows]
            );
            synced(&h.app);
            command(&mut h, "z");
            assert_eq!(h.app.document.source(), SOURCE);
        } else {
            assert_eq!(h.app.document.source(), SOURCE);
        }
        assert!(!h.app.document.can_undo());
        synced(&h.app);
    }
}

#[test]
fn native_paste_delivery_preserves_captured_sheets_values_and_standard_export() {
    let captured = include_str!("../../tests/fixtures/google-sheets-clipboard.tsv");
    let expected: Vec<Vec<String>> = [
        ["plain", "say \"hello\"", "\"literal\"", "end"],
        ["a\tb", "line1\nline2", "top\n\nbottom", ""],
        ["日本語 café", "é 🧑‍💻", "**literal** | &amp; \\", "spaced"],
        ["123", "12345678901234567890", "2", "2026-11-03"],
        ["", "", "", ""],
    ]
    .into_iter()
    .map(|row| row.into_iter().map(String::from).collect())
    .collect();
    let source = "Before\n\n| Header |\n|---|\n| original |\n\nAfter\n";
    for (producer, text) in [
        (Producer::Sheets, captured.into()),
        (
            Producer::Standard,
            crate::document::table_clipboard::encode(&expected),
        ),
    ] {
        let mut h = Harness::new(source);
        h.focus_cell(0, 1, 0);
        let before = h.app.grid_view().unwrap();
        let revision = Arc::clone(&h.app.projection.source_id);
        deliver(
            &mut h,
            Message::PasteReady {
                revision,
                view: before,
                text,
                producer,
                values: None,
            },
        );
        assert_eq!(rectangle(&h, (1, 0), 5, 4), expected, "{producer:?}");
        let table = &h.app.projection.rich.tables[0];
        assert_eq!(table.rows.len(), 6);
        assert!(table.rows.iter().all(|row| row.len() == 4));
        assert_eq!(value(&h.app, 0, 0), "Header");
        let after = h.app.grid_view().unwrap();
        assert_eq!((after.anchor, after.head), ((1, 0), (5, 3)));
        synced(&h.app);
        command(&mut h, "z");
        assert_eq!(h.app.document.source(), source);
        assert_eq!(h.app.grid_view(), Some(before));
        assert!(!h.app.document.can_undo());
        synced(&h.app);
    }
}

#[test]
fn native_sheets_single_column_delivery_keeps_the_final_empty_row() {
    let source = "Before\n\n| A |\n|---|\n| old first |\n| old second |\n| untouched |\n\nAfter\n";
    for (text, first) in [("end\n", "end"), ("\n", "")] {
        let mut h = Harness::new(source);
        h.focus_cell(0, 1, 0);
        let before = h.app.grid_view().unwrap();
        let revision = Arc::clone(&h.app.projection.source_id);
        deliver(
            &mut h,
            Message::PasteReady {
                revision,
                view: before,
                text: text.into(),
                producer: Producer::Sheets,
                values: None,
            },
        );
        assert_eq!(rectangle(&h, (1, 0), 2, 1), vec![vec![first], vec![""]]);
        assert_eq!(value(&h.app, 3, 0), "untouched");
        assert_eq!(h.app.projection.rich.tables[0].rows.len(), 4);
        let after = h.app.grid_view().unwrap();
        assert_eq!((after.anchor, after.head), ((1, 0), (2, 0)));
        assert_eq!(after.active, (1, 0));
        synced(&h.app);
        command(&mut h, "z");
        assert_eq!(h.app.document.source(), source);
        assert_eq!(h.app.grid_view(), Some(before));
        assert!(!h.app.document.can_undo());
        synced(&h.app);
    }
}

#[test]
fn native_sheets_decoded_values_preserve_quotes_across_adjacent_cells() {
    let source = "| A | B |\n|---|---|\n| old first | old second |\n";
    let mut h = Harness::new(source);
    h.focus_cell(0, 1, 0);
    let before = h.app.grid_view().unwrap();
    let revision = Arc::clone(&h.app.projection.source_id);
    deliver(
        &mut h,
        Message::PasteReady {
            revision,
            view: before,
            text: include_str!("../../tests/fixtures/google-sheets-ambiguous.tsv").into(),
            producer: Producer::Sheets,
            values: Some(vec![vec!["\"open".into(), "close\"".into()]]),
        },
    );
    assert_eq!(rectangle(&h, (1, 0), 1, 2), vec![vec!["\"open", "close\""]]);
    let after = h.app.grid_view().unwrap();
    assert_eq!((after.anchor, after.head), ((1, 0), (1, 1)));
    assert_eq!(after.active, (1, 0));
    synced(&h.app);
    command(&mut h, "z");
    assert_eq!(h.app.document.source(), source);
    assert_eq!(h.app.grid_view(), Some(before));
    assert!(!h.app.document.can_undo());
    synced(&h.app);
}

#[test]
fn native_excel_cells_preserve_displayed_values_growth_and_source_undo() {
    let source = "Before\n\n| A | B |\n|---|---|\n| original | keep |\n\nTAIL_LITERAL";
    let expected: Vec<Vec<String>> = [
        ["plain", "00123", "12345678901234567890", "2026-11-03"],
        ["white", "  spaced  ", "literal", "end"],
    ]
    .into_iter()
    .map(|row| row.into_iter().map(String::from).collect())
    .collect();
    let values = crate::document::table_clipboard_html::decode_excel(include_str!(
        "../../tests/fixtures/excel-web-clipboard.html"
    ))
    .unwrap();
    assert_eq!(values, expected);
    let mut h = Harness::new(source);
    h.focus_cell(0, 0, 0);
    let before = h.app.grid_view().unwrap();
    let revision = Arc::clone(&h.app.projection.source_id);
    deliver(
        &mut h,
        Message::PasteReady {
            revision,
            view: before,
            text: crate::document::table_clipboard::encode(&expected),
            producer: Producer::Excel,
            values: Some(values),
        },
    );
    assert_eq!(rectangle(&h, (0, 0), 2, 4), expected);
    assert!(h.app.document.source().starts_with("Before\n\n"));
    assert!(h.app.document.source().ends_with("\n\nTAIL_LITERAL"));
    synced(&h.app);
    command(&mut h, "z");
    assert_eq!(h.app.document.source(), source);
    assert_eq!(h.app.grid_view(), Some(before));
    assert!(!h.app.document.can_undo());
    synced(&h.app);
}

#[test]
fn verified_empty_native_cells_clear_only_their_footprint_and_undo() {
    let source = "| A | B |\n|---|---|\n| original | keep |\n\nTAIL_LITERAL";
    for values in [vec![vec![String::new()]], vec![vec![String::new()]; 2]] {
        let mut h = Harness::new(source);
        h.focus_cell(0, 0, 0);
        let before = h.app.grid_view().unwrap();
        let revision = Arc::clone(&h.app.projection.source_id);
        let height = values.len();
        deliver(
            &mut h,
            Message::EmptyPasteReady {
                revision,
                view: before,
                result: Ok(Some(Snapshot {
                    producer: Producer::Excel,
                    values: Some(values),
                })),
            },
        );
        assert_eq!(rectangle(&h, (0, 0), height, 1), vec![vec![""]; height]);
        assert_eq!(value(&h.app, 0, 1), "B");
        assert_eq!(value(&h.app, 1, 1), "keep");
        if height == 1 {
            assert_eq!(value(&h.app, 1, 0), "original");
        }
        synced(&h.app);
        command(&mut h, "z");
        assert_eq!(h.app.document.source(), source);
        assert_eq!(h.app.grid_view(), Some(before));
        assert!(!h.app.document.can_undo());
    }
}

#[test]
fn unavailable_nonempty_or_stale_empty_clipboards_cannot_clear_cells() {
    for result in [
        Ok(None),
        Ok(Some(Snapshot {
            producer: Producer::Standard,
            values: None,
        })),
        Ok(Some(Snapshot {
            producer: Producer::Excel,
            values: Some(vec![vec!["not empty".into()]]),
        })),
        Err("Clipboard unavailable".into()),
    ] {
        let mut h = Harness::new(SOURCE);
        h.focus_cell(0, 0, 0);
        let view = h.app.grid_view().unwrap();
        let revision = Arc::clone(&h.app.projection.source_id);
        deliver(
            &mut h,
            Message::EmptyPasteReady {
                revision,
                view,
                result,
            },
        );
        assert_eq!(h.app.document.source(), SOURCE);
        assert_eq!(h.app.grid_view(), Some(view));
        assert!(!h.app.document.can_undo());
    }
    let mut h = Harness::new(SOURCE);
    h.focus_cell(0, 0, 0);
    let view = h.app.grid_view().unwrap();
    let revision = Arc::clone(&h.app.projection.source_id);
    h.focus_cell(0, 1, 1);
    deliver(
        &mut h,
        Message::EmptyPasteReady {
            revision,
            view,
            result: Ok(Some(Snapshot {
                producer: Producer::Excel,
                values: Some(vec![vec![String::new()]]),
            })),
        },
    );
    assert_eq!(h.app.document.source(), SOURCE);
    assert!(!h.app.document.can_undo());
}

#[test]
fn native_paste_and_cut_deliveries_reject_stale_revisions_and_selections() {
    for source_changed in [false, true] {
        for cut in [false, true] {
            let mut h = Harness::new(SOURCE);
            h.focus_cell(0, 1, 1);
            let revision = Arc::clone(&h.app.projection.source_id);
            let view = h.app.grid_view().unwrap();
            if source_changed {
                h.app
                    .document
                    .transact(
                        &[SourceEdit {
                            range: 0..0,
                            text: "New introduction\n\n".into(),
                        }],
                        EditOrigin::Agent,
                        None,
                        true,
                    )
                    .unwrap();
                h.app.sync_editor();
                assert_eq!(h.app.grid_view(), Some(view));
            } else {
                h.focus_cell(0, 2, 2);
                assert!(Arc::ptr_eq(&revision, &h.app.projection.source_id));
            }
            let source = h.app.document.source().to_owned();
            let current = h.app.grid_view();
            let selection = h.app.selection;
            let can_undo = h.app.document.can_undo();
            let message = if cut {
                Message::CopyReady {
                    revision,
                    view,
                    cut: true,
                    result: Ok(()),
                }
            } else {
                Message::PasteReady {
                    revision,
                    view,
                    text: "\"literal\"\treplacement".into(),
                    producer: Producer::Sheets,
                    values: None,
                }
            };
            deliver(&mut h, message);
            assert_eq!(h.app.document.source(), source);
            assert_eq!(h.app.grid_view(), current);
            assert_eq!(h.app.selection, selection);
            assert_eq!(h.app.document.can_undo(), can_undo);
            assert!(h.app.notification.is_some());
            synced(&h.app);
        }
    }
}

#[test]
fn failed_native_cut_delivery_preserves_the_source_selection_and_history() {
    let mut h = Harness::new(SOURCE);
    range(&mut h, (3, 1), (1, 1));
    let revision = Arc::clone(&h.app.projection.source_id);
    let view = h.app.grid_view().unwrap();
    let selection = h.app.selection;
    deliver(
        &mut h,
        Message::CopyReady {
            revision,
            view,
            cut: true,
            result: Err("Clipboard verification failed".into()),
        },
    );
    assert_eq!(h.app.document.source(), SOURCE);
    assert_eq!(h.app.grid_view(), Some(view));
    assert_eq!(h.app.selection, selection);
    assert!(!h.app.document.can_undo());
    assert_eq!(
        h.app.notification.as_deref(),
        Some("Clipboard verification failed")
    );
    synced(&h.app);
}

#[test]
fn real_widget_copy_and_paste_round_trip_visible_quoted_values_and_empty_edges() {
    let source = "Before\n\n| A | B | C |\n|---|---|---|\n| tab&#9;inside | first<br>second | &quot;quoted&quot; |\n| café 🧑‍💻 | &#42;literal&#42; | |\n| | | |\n\nAfter\n";
    let expected = vec![
        vec!["tab\tinside", "first\nsecond", "\"quoted\""],
        vec!["café 🧑‍💻", "*literal*", ""],
        vec!["", "", ""],
    ];
    let mut from = Harness::new(source);
    range(&mut from, (3, 2), (1, 0));
    let grid = from.app.grid_view();
    let selection = from.app.selection;
    command(&mut from, "c");
    let copied = from.clipboard.standard.as_deref().unwrap();
    assert_eq!(
        copied,
        "\"tab\tinside\"\t\"first\nsecond\"\t\"\"\"quoted\"\"\"\r\ncafé 🧑‍💻\t*literal*\t\r\n\t\t"
    );
    assert_eq!(from.app.document.source(), source);
    assert_eq!(from.app.grid_view(), grid);
    assert_eq!(from.app.selection, selection);
    assert!(!from.app.document.can_undo());

    let destination = "Before\n\n| Header |\n|---|\n| original |\n\nAfter\n";
    let mut to = Harness::new(destination);
    to.clipboard.standard = Some(copied.into());
    to.focus_cell(0, 1, 0);
    command(&mut to, "v");
    assert_eq!(rectangle(&to, (1, 0), 3, 3), expected);
    assert_eq!(to.app.projection.rich.tables[0].rows.len(), 4);
    let pasted = to.app.grid_view().unwrap();
    assert_eq!((pasted.anchor, pasted.head), ((1, 0), (3, 2)));
    synced(&to.app);
    command(&mut to, "z");
    assert_eq!(to.app.document.source(), destination);
    assert!(!to.app.document.can_undo());
    synced(&to.app);
}

#[test]
fn real_widget_paste_grows_both_axes_without_wrapping_and_is_one_source_undo() {
    let source = "Before\n\n| A | B |\n|:---|---:|\n| keep | old |\n\nAfter\n";
    let mut h = Harness::new(source);
    let keep = h
        .app
        .document
        .collaboration()
        .position(source.find("keep").unwrap(), Assoc::After);
    h.focus_cell(0, 1, 1);
    h.clipboard.standard = Some("one\ttwo\tthree\r\nfour\tfive\tsix\r\n".into());
    command(&mut h, "v");
    assert_eq!(
        rectangle(&h, (0, 0), 3, 4),
        vec![
            vec!["A", "B", "", ""],
            vec!["keep", "one", "two", "three"],
            vec!["", "four", "five", "six"],
        ]
    );
    let view = h.app.grid_view().unwrap();
    assert_eq!((view.anchor, view.head), ((1, 1), (2, 3)));
    assert_eq!(view.active, (1, 1));
    assert!(h.app.document.source().starts_with("Before\n\n"));
    assert!(h.app.document.source().ends_with("\n\nAfter\n"));
    let keep = h.app.document.collaboration().resolve(&keep).unwrap();
    assert!(h.app.document.source()[keep..].starts_with("keep"));
    synced(&h.app);
    command(&mut h, "z");
    assert_eq!(h.app.document.source(), source);
    assert!(!h.app.document.can_undo());
    h.key(
        Key::Character("z".into()),
        Modifiers::COMMAND | Modifiers::SHIFT,
        None,
    );
    assert_eq!(value(&h.app, 2, 3), "six");
    synced(&h.app);
}

#[test]
fn real_widget_paste_undo_and_redo_restore_the_single_cell_and_rectangle() {
    for source in [
        "| A | B | C |\n|---|---|---|\n| keep1 | old1 | old2 |\n| keep2 | old3 | old4 |\n",
        "| A | B |\n|---|---|\n| keep | old |\n",
    ] {
        let mut h = Harness::new(source);
        h.focus_cell(0, 1, 1);
        let before = h.app.grid_view();
        h.clipboard.standard = Some("one\ttwo\r\nthree\tfour".into());
        command(&mut h, "v");
        let after = h.app.grid_view();
        let pasted = h.app.document.source().to_owned();
        let rectangle = after.unwrap();
        assert_eq!((rectangle.anchor, rectangle.head), ((1, 1), (2, 2)));
        assert_eq!(rectangle.active, (1, 1));

        command(&mut h, "z");
        assert_eq!(h.app.document.source(), source);
        assert_eq!(h.app.grid_view(), before);
        assert!(!h.app.document.can_undo());
        synced(&h.app);
        h.key(
            Key::Character("z".into()),
            Modifiers::COMMAND | Modifiers::SHIFT,
            None,
        );
        assert_eq!(h.app.document.source(), pasted);
        assert_eq!(h.app.grid_view(), after);
        synced(&h.app);
    }
}

#[test]
fn real_widget_reverse_range_paste_history_retains_direction_and_tabbed_active_cell() {
    let source = "| A | B | C |\n|---|---|---|\n| keep1 | old | old |\n| keep2 | old | old |\n| keep3 | old | old |\n| keep4 | old | old |\n";
    let mut h = Harness::new(source);
    range(&mut h, (4, 2), (1, 1));
    h.key(Key::Named(Named::Tab), Modifiers::empty(), None);
    h.key(Key::Named(Named::Tab), Modifiers::empty(), None);
    let before = h.app.grid_view().unwrap();
    assert_eq!((before.anchor, before.head), ((4, 2), (1, 1)));
    assert_ne!(before.active, before.anchor);
    assert_ne!(before.active, before.head);
    h.clipboard.standard = Some("one\r\ntwo".into());
    command(&mut h, "v");
    let pasted = h.app.document.source().to_owned();
    assert_eq!(h.app.grid_view(), Some(before));
    command(&mut h, "z");
    assert_eq!(h.app.document.source(), source);
    assert_eq!(h.app.grid_view(), Some(before));
    assert!(!h.app.document.can_undo());
    synced(&h.app);
    h.key(
        Key::Character("z".into()),
        Modifiers::COMMAND | Modifiers::SHIFT,
        None,
    );
    assert_eq!(h.app.document.source(), pasted);
    assert_eq!(h.app.grid_view(), Some(before));
    synced(&h.app);
}

#[test]
fn real_widget_paste_history_survives_a_later_cell_edit_and_its_undo() {
    let source = "| A | B |\n|---|---|\n| keep | old |\n";
    let mut h = Harness::new(source);
    h.focus_cell(0, 1, 1);
    let before = h.app.grid_view();
    h.clipboard.standard = Some("one\ttwo\r\nthree\tfour".into());
    command(&mut h, "v");
    let after = h.app.grid_view();
    let pasted = h.app.document.source().to_owned();
    h.key(Key::Character("X".into()), Modifiers::empty(), Some("X"));
    h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
    assert_eq!(value(&h.app, 1, 1), "X");
    command(&mut h, "z");
    assert_eq!(h.app.document.source(), pasted);
    assert!(h.app.document.can_undo());
    command(&mut h, "z");
    assert_eq!(h.app.document.source(), source);
    assert_eq!(h.app.grid_view(), before);
    assert!(!h.app.document.can_undo());
    synced(&h.app);
    h.key(
        Key::Character("z".into()),
        Modifiers::COMMAND | Modifiers::SHIFT,
        None,
    );
    assert_eq!(h.app.document.source(), pasted);
    assert_eq!(h.app.grid_view(), after);
    synced(&h.app);
}

#[test]
fn real_widget_range_paste_fills_tiles_and_rejects_incompatible_shapes_atomically() {
    let source = "| A | B | C |\n|---|---|---|\n| keep1 | old | old |\n| keep2 | old | old |\n| keep3 | old | old |\n| keep4 | old | old |\n";
    let mut h = Harness::new(source);
    range(&mut h, (4, 2), (1, 1));
    h.clipboard.standard = Some(" **literal** | café ".into());
    command(&mut h, "v");
    assert_eq!(
        rectangle(&h, (1, 1), 4, 2),
        vec![vec![" **literal** | café "; 2]; 4]
    );
    for row in 1..=4 {
        assert_eq!(value(&h.app, row, 0), format!("keep{row}"));
    }
    command(&mut h, "z");
    assert_eq!(h.app.document.source(), source);
    assert!(!h.app.document.can_undo());

    range(&mut h, (4, 2), (1, 1));
    h.clipboard.standard = Some("x\r\ny\r\n".into());
    command(&mut h, "v");
    assert_eq!(
        rectangle(&h, (1, 1), 4, 2),
        vec![
            vec!["x", "x"],
            vec!["y", "y"],
            vec!["x", "x"],
            vec!["y", "y"]
        ]
    );
    synced(&h.app);
    command(&mut h, "z");
    assert_eq!(h.app.document.source(), source);
    assert!(!h.app.document.can_undo());

    range(&mut h, (2, 2), (1, 1));
    let grid = h.app.grid_view();
    let selection = h.app.selection;
    h.clipboard.standard = Some("one\ttwo\tthree".into());
    command(&mut h, "v");
    assert_eq!(h.app.document.source(), source);
    assert_eq!(h.app.grid_view(), grid);
    assert_eq!(h.app.selection, selection);
    assert!(!h.app.document.can_undo());
    assert!(h.app.notification.is_some());
    synced(&h.app);
}

#[test]
fn no_op_range_paste_preserves_reverse_selection_formatting_and_history() {
    let source = "| A | B | C |\n|---|---|---|\n| keep1 | **old** | old |\n| keep2 | old | *old* |\n| keep3 | `old` | old |\n";
    let mut h = Harness::new(source);
    range(&mut h, (3, 2), (1, 1));
    let revision = Arc::clone(&h.app.projection.source_id);
    let grid = h.app.grid_view();
    let selection = h.app.selection;
    h.clipboard.standard = Some("old".into());
    command(&mut h, "v");
    assert_eq!(h.app.document.source(), source);
    assert_eq!(h.app.grid_view(), grid);
    assert_eq!(h.app.selection, selection);
    assert!(Arc::ptr_eq(&revision, &h.app.projection.source_id));
    assert!(!h.app.document.can_undo());
    synced(&h.app);
}

#[test]
fn real_widget_paste_preserves_literal_values_source_endings_and_container_prefixes() {
    for ending in ["\n", "\r\n", "\r"] {
        for (preamble, prefix) in [("", "> "), ("- Outer\n", "  > ")] {
            let source = format!(
                "Before\n\n{preamble}{prefix}| A | B | C |\n{prefix}|:---|---:|:---:|\n{prefix}| keep | old |\n\nAfter\n"
            )
            .replace('\n', ending);
            let mut h = Harness::new(&source);
            h.focus_cell(0, 1, 1);
            h.clipboard.standard = Some(
                " **literal** | café \t\"line\r\nbreak\"\r\n=SUM(1,2)\t\"a\\b \"\"quote\"\"\"\r\n"
                    .into(),
            );
            command(&mut h, "v");
            assert_eq!(
                rectangle(&h, (1, 1), 2, 2),
                vec![
                    vec![" **literal** | café ", "line\nbreak"],
                    vec!["=SUM(1,2)", "a\\b \"quote\""]
                ],
                "{ending:?}, {prefix:?}"
            );
            assert_eq!(value(&h.app, 1, 0), "keep");
            assert!(
                h.app
                    .document
                    .source()
                    .starts_with(&format!("Before{ending}{ending}"))
            );
            assert!(
                h.app
                    .document
                    .source()
                    .ends_with(&format!("{ending}{ending}After{ending}"))
            );
            let table = &h.app.projection.rich.tables[0];
            let line_start = h.app.document.source()[..table.source_range.start]
                .rfind(['\r', '\n'])
                .map_or(0, |point| point + 1);
            for line in h.app.document.source()[line_start..table.source_range.end].split(ending) {
                if !line.is_empty() {
                    assert!(line.starts_with(prefix), "{ending:?}, {prefix:?}: {line:?}");
                }
            }
            let without_endings = h.app.document.source().replace(ending, "");
            assert!(!without_endings.contains(['\r', '\n']));
            synced(&h.app);
            command(&mut h, "z");
            assert_eq!(h.app.document.source(), source);
            assert!(!h.app.document.can_undo());
            synced(&h.app);
        }
    }
}

#[test]
fn real_widget_cut_only_clears_after_a_verified_clipboard_write() {
    let mut h = Harness::new(SOURCE);
    range(&mut h, (3, 1), (1, 1));
    let grid = h.app.grid_view();
    let selection = h.app.selection;
    h.clipboard.standard = Some("previous clipboard".into());
    h.clipboard.refuse_writes = true;
    command(&mut h, "x");
    assert_eq!(h.clipboard.standard.as_deref(), Some("previous clipboard"));
    assert_eq!(h.app.document.source(), SOURCE);
    assert_eq!(h.app.grid_view(), grid);
    assert_eq!(h.app.selection, selection);
    assert!(!h.app.document.can_undo());
    assert!(h.app.notification.is_some());

    h.clipboard.refuse_writes = false;
    command(&mut h, "c");
    assert!(h.app.notification.is_none());
    assert_eq!(h.app.document.source(), SOURCE);
    command(&mut h, "x");
    assert!(h.app.notification.is_none());
    assert_eq!(h.clipboard.standard.as_deref(), Some("Maya\r\nNoah\r\nAri"));
    assert_eq!(rectangle(&h, (1, 1), 3, 1), vec![vec![""]; 3]);
    assert_eq!(value(&h.app, 1, 0), "Draft proposal");
    assert_eq!(value(&h.app, 3, 2), "Planned");
    synced(&h.app);
    command(&mut h, "z");
    assert_eq!(h.app.document.source(), SOURCE);
    assert!(!h.app.document.can_undo());
    synced(&h.app);
}

#[test]
fn delayed_cut_rejects_stale_source_revisions_and_changed_selections() {
    for source_changed in [false, true] {
        let mut h = Harness::new(SOURCE);
        range(&mut h, (3, 1), (1, 1));
        let revision = Arc::clone(&h.app.projection.source_id);
        let view = h.app.grid_view().unwrap();
        if source_changed {
            h.app
                .document
                .transact(
                    &[SourceEdit {
                        range: 0..0,
                        text: "New introduction\n\n".into(),
                    }],
                    EditOrigin::Agent,
                    None,
                    true,
                )
                .unwrap();
            h.app.sync_editor();
            assert_eq!(h.app.grid_view(), Some(view));
        } else {
            h.focus_cell(0, 2, 2);
            assert!(Arc::ptr_eq(&revision, &h.app.projection.source_id));
        }
        let source = h.app.document.source().to_owned();
        let current = h.app.grid_view();
        let selection = h.app.selection;
        let can_undo = h.app.document.can_undo();
        send(&mut h.app, Message::Cut { revision, view });
        assert_eq!(h.app.document.source(), source);
        assert_eq!(h.app.grid_view(), current);
        assert_eq!(h.app.selection, selection);
        assert_eq!(h.app.document.can_undo(), can_undo);
        assert!(h.app.notification.is_some());
        synced(&h.app);
    }
}

#[test]
fn real_widget_pending_draft_paste_stays_literal_and_uses_local_undo() {
    let mut h = Harness::new(SOURCE);
    h.focus_cell(0, 1, 1);
    h.key(Key::Named(Named::Enter), Modifiers::empty(), None);
    h.clipboard.standard = Some("one\ttwo\r\nthree\tfour".into());
    command(&mut h, "v");
    assert_eq!(
        h.app.grid.draft.as_ref().unwrap().buffer.snapshot(),
        "Mayaone\ttwo\nthree\tfour"
    );
    assert_eq!(h.app.document.source(), SOURCE);
    assert!(!h.app.document.can_undo());
    command(&mut h, "z");
    assert_eq!(h.app.grid.draft.as_ref().unwrap().buffer.snapshot(), "Maya");
    assert_eq!(h.app.document.source(), SOURCE);
    assert!(!h.app.document.can_undo());
}

#[test]
#[ignore = "release-only table clipboard measurements"]
fn bulk_widget_paste_measurements() {
    use std::time::Instant;

    let source = "Before\n\n| A | B | C |\n|---|---|---|\n| original | | |\n\nAfter\n";
    for rows in [1_000, 5_000] {
        let clipboard = (0..rows)
            .map(|row| format!("row {row}\tcafé 🧑‍💻\t **literal** | "))
            .collect::<Vec<_>>()
            .join("\r\n");
        let mut samples = Vec::new();
        for _ in 0..5 {
            let mut h = Harness::new(source);
            h.focus_cell(0, 1, 0);
            h.clipboard.standard = Some(clipboard.clone());
            let start = Instant::now();
            command(&mut h, "v");
            samples.push(start.elapsed());

            assert_eq!(h.app.projection.rich.tables[0].rows.len(), rows + 1);
            for row in [1, rows / 2, rows] {
                assert_eq!(value(&h.app, row, 0), format!("row {}", row - 1));
                assert_eq!(value(&h.app, row, 1), "café 🧑‍💻");
                assert_eq!(value(&h.app, row, 2), " **literal** | ");
            }
            let view = h.app.grid_view().unwrap();
            assert_eq!((view.anchor, view.head), ((1, 0), (rows, 2)));
            synced(&h.app);
            command(&mut h, "z");
            assert_eq!(h.app.document.source(), source);
            assert!(!h.app.document.can_undo());
            synced(&h.app);
        }
        samples.sort();
        eprintln!(
            "bulk-widget-paste rows={rows} columns=3 samples={} min_ms={:.3} median_ms={:.3} max_ms={:.3}; real keyboard command through app update, widget focus, and layout; setup, correctness, and undo excluded",
            samples.len(),
            samples[0].as_secs_f64() * 1_000.0,
            samples[2].as_secs_f64() * 1_000.0,
            samples[4].as_secs_f64() * 1_000.0,
        );
    }
}
