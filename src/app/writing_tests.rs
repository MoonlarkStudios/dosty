//! Writing workflows use the real App transaction, projection, and native mirror.

use super::super::trail::Direction;
use super::*;
use crate::document::Document;
use crate::document::projection::Projection;
use crate::io::{collaboration_store, export};

fn app(source: &str) -> App {
    let (mut app, _) = App::new();
    app.set_document(source.into(), None, "Writing.md".into());
    app
}
fn send(app: &mut App, message: AppMessage) {
    drop(app.update(message));
    assert_eq!(
        app.session.document.source(),
        app.session.document.collaboration().materialize()
    );
    let fresh = Projection::with_context(
        app.session.document.source(),
        app.editor_focused.then_some(app.session.selection.head),
    );
    assert_eq!(app.projection.text, fresh.text);
    assert_eq!(app.projection.rich, fresh.rich);
    assert_eq!(
        app.editor.snapshot(),
        if app.mode == EditorMode::Source {
            app.session.document.source()
        } else {
            &app.projection.text
        }
    );
}
fn tool(app: &mut App, message: Message) {
    send(app, AppMessage::Writing(message));
}
fn select(app: &mut App, selection: Selection) {
    send(app, AppMessage::SelectSource(selection));
}

#[tokio::test]
async fn dropped_image_import_keeps_later_reverse_selection_remote_edits_and_undo() {
    use iced::futures::StreamExt;
    let root = tempfile::tempdir().unwrap();
    let image = root.path().join("写 真.png");
    let path = root.path().join("Writing.md");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 70, 90, 255]))
        .save(&image)
        .unwrap();
    let bytes = std::fs::read(&image).unwrap();
    let original = "# Heading\r\n\r\nParagraph.\n\n| A | B |\n|---|---|\n| one | two |\n\n![alt](missing.png)\n\nTail.";
    let mut app = app(original);
    app.session.assume_saved_file(path.clone());
    let gap = original.find("Paragraph").unwrap();
    select(&mut app, Selection::caret(gap));
    // Schedule the actual import without executing native dialogs or image tasks.
    let task = app.update_message(AppMessage::Dropped(image.clone()));
    assert!(app.busy);
    assert_eq!(app.session.document.source(), original);
    let selection = Selection {
        anchor: original.len(),
        head: original.find("Heading").unwrap(),
    };
    select(&mut app, selection);
    let stable_selection = app.session.document.collaboration().selection(selection);
    let mut peer = Document::from_state(&app.session.document.collaboration().state()).unwrap();
    peer.replace(0..0, "Remote prefix.\n\n").unwrap();
    send(
        &mut app,
        AppMessage::ReplicaLoaded(Ok(peer.collaboration().state())),
    );
    let before_import = app.session.document.source().to_owned();
    let mut outputs = iced_runtime::task::into_stream(task).unwrap();
    let mut completed = 0;
    while let Some(action) = outputs.next().await {
        match action {
            iced_runtime::Action::Output(message) => {
                assert!(matches!(
                    &message,
                    AppMessage::Writing(Message::Imported { result: Ok(_), .. })
                ));
                send(&mut app, message);
                completed += 1;
            }
            action => panic!("Unexpected import action: {action:?}"),
        }
    }
    assert_eq!(completed, 1);
    assert!(!app.busy);
    assert_eq!(std::fs::read(image).unwrap(), bytes);
    let imported = app.session.document.source().to_owned();
    assert!(imported.contains("![写 真](<Writing%2Eassets/"));
    assert!(imported.contains("Remote prefix."));
    assert_eq!(
        app.session.selection,
        app.session
            .document
            .collaboration()
            .resolve_selection(&stable_selection)
            .unwrap()
    );
    assert!(app.session.selection.anchor > app.session.selection.head);
    let destination = app
        .projection
        .rich
        .images
        .iter()
        .find(|image| image.url.contains("Writing%2Eassets/"))
        .unwrap();
    let stored = assets::local_path(&destination.url, path.parent())
        .unwrap()
        .unwrap();
    assert_eq!(std::fs::read(stored).unwrap(), bytes);
    send(&mut app, AppMessage::Undo);
    assert_eq!(app.session.document.source(), before_import);
    send(&mut app, AppMessage::Redo);
    assert_eq!(app.session.document.source(), imported);
}

#[test]
fn canceling_first_save_after_image_drop_preserves_unsaved_source_selection_and_history() {
    let mut app = app("# Heading\n\nUnfinished paragraph.");
    send(
        &mut app,
        AppMessage::Edit(text_editor::Action::Edit(text_editor::Edit::Insert('!'))),
    );
    let source = app.session.document.source().to_owned();
    let selection = Selection {
        anchor: source.len(),
        head: 2,
    };
    select(&mut app, selection);
    drop(app.update_message(AppMessage::Dropped(PathBuf::from("photo.png"))));
    assert!(app.busy);
    assert!(app.writing.pending_media.is_some());
    let generation = app.session.document.collaboration().generation();
    send(
        &mut app,
        AppMessage::Saved {
            result: Err(storage::FileError::Io("Save cancelled".into())),
            source: Arc::new(source.clone()),
            version: String::new(),
            generation,
        },
    );
    assert!(!app.busy);
    assert!(app.session.path().is_none());
    assert!(app.session.is_dirty());
    assert!(app.writing.pending_media.is_none());
    assert_eq!(app.session.document.source(), source);
    assert_eq!(app.session.selection, selection);
    send(&mut app, AppMessage::Undo);
    assert_eq!(
        app.session.document.source(),
        "# Heading\n\nUnfinished paragraph."
    );
}

#[tokio::test]
async fn document_drop_keeps_unsaved_writing_reverse_selection_and_undo_for_back() {
    use iced::futures::StreamExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("another.md");
    std::fs::write(&path, "Another document.").unwrap();
    let mut app = app("# Heading\n\nParagraph.");
    send(
        &mut app,
        AppMessage::Edit(text_editor::Action::Edit(text_editor::Edit::Insert('!'))),
    );
    let source = app.session.document.source().to_owned();
    let selection = Selection {
        anchor: source.len(),
        head: 2,
    };
    select(&mut app, selection);
    let task = app.update_message(AppMessage::Dropped(path));
    assert_eq!(app.session.document.source(), source);
    assert_eq!(app.session.selection, selection);
    // Classifying the path and reading the file are separate background steps.
    // Focus and scroll operations need a live widget tree and are skipped.
    let mut tasks = vec![task];
    while let Some(task) = tasks.pop() {
        let Some(mut effects) = iced_runtime::task::into_stream(task) else {
            continue;
        };
        while let Some(effect) = effects.next().await {
            if let iced_runtime::Action::Output(message) = effect {
                tasks.push(app.update_message(message));
            }
        }
    }
    assert!(app.overlay.is_none());
    assert!(app.pending.is_none());
    assert_eq!(app.session.document.source(), "Another document.");
    send(&mut app, AppMessage::Step(Direction::Back));
    assert!(app.session.is_dirty());
    assert_eq!(app.session.document.source(), source);
    assert_eq!(app.session.selection, selection);
    send(&mut app, AppMessage::Undo);
    assert_eq!(app.session.document.source(), "# Heading\n\nParagraph.");
}

#[tokio::test]
async fn representative_document_edits_undo_merge_save_reopen_and_export() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Writing.md");
    let destination = assets::import(
        &path,
        assets::Input::Png(include_bytes!("../../assets/writing/coast.png").to_vec()),
    )
    .unwrap();
    let source = include_str!("../../assets/writing/the-quiet-hour.md")
        .replace("the-quiet-hour.assets/coast.png", &destination);
    let mut app = app(&source);
    app.session.assume_saved_file(path.clone());
    let mut peer = Document::from_state(&app.session.document.collaboration().state()).unwrap();
    let start = source.find("CommonMark reference").unwrap();
    let before = Selection {
        anchor: start + "CommonMark reference".len(),
        head: start,
    };
    select(&mut app, before);
    tool(&mut app, Message::Details(Kind::Link));
    tool(
        &mut app,
        Message::Field(Field::Label, "A clear reference".into()),
    );
    tool(
        &mut app,
        Message::Field(Field::Destination, "https://commonmark.org/".into()),
    );
    // An independent replica edits unrelated content while the details are open.
    peer.transact(
        &[SourceEdit {
            range: 0..0,
            text: "A shared note.\n\n".into(),
        }],
        EditOrigin::Human,
        None,
        true,
    )
    .unwrap();
    send(
        &mut app,
        AppMessage::ReplicaLoaded(Ok(peer.collaboration().state())),
    );
    tool(&mut app, Message::Accept);
    assert!(
        app.session
            .document
            .source()
            .contains("[A clear reference](<https://commonmark.org/>)")
    );
    assert!(app.session.document.source().starts_with("A shared note."));
    send(&mut app, AppMessage::Undo);
    assert_eq!(
        app.session.document.source(),
        format!("A shared note.\n\n{source}")
    );
    assert!(app.session.selection.anchor > app.session.selection.head);
    send(&mut app, AppMessage::Redo);
    let image = app.session.document.source().find("![").unwrap();
    select(&mut app, Selection::caret(image));
    tool(&mut app, Message::Details(Kind::Image));
    tool(
        &mut app,
        Message::Field(Field::Caption, "The first light on the water.".into()),
    );
    tool(
        &mut app,
        Message::Field(Field::Credit, "Dosty — illustrative image".into()),
    );
    tool(
        &mut app,
        Message::Field(Field::CreditUrl, "https://openai.com/".into()),
    );
    tool(&mut app, Message::Accept);
    let quote = app.session.document.source().find("> Make enough").unwrap();
    select(&mut app, Selection::caret(quote + 3));
    tool(&mut app, Message::Details(Kind::Quote));
    tool(
        &mut app,
        Message::Field(Field::Credit, "Dosty notebook".into()),
    );
    tool(&mut app, Message::Accept);
    assert!(
        app.session
            .document
            .source()
            .contains("> — Dosty notebook, [A place to begin](<#a-place-to-begin>)")
    );
    peer.receive(&app.session.document.collaboration().state())
        .unwrap();
    assert_eq!(peer.source(), app.session.document.source());
    let state = app.session.document.collaboration().state();
    collaboration_store::save_document(
        path.clone(),
        app.session.document.source().into(),
        state,
        None,
        None,
    )
    .await
    .unwrap();
    let loaded = storage::load(path).await.unwrap();
    let reopened = Document::from_state(&loaded.collaboration.unwrap()).unwrap();
    assert_eq!(reopened.source(), app.session.document.source());
    assert_ne!(
        reopened.collaboration().replica_id(),
        app.session.document.collaboration().replica_id()
    );
    let html = export::html(reopened.source(), Some(root.path())).unwrap();
    assert!(html.contains("data:image/png;base64,"));
    assert!(html.contains("The first light on the water."));
    assert!(html.contains("Dosty notebook"));
    assert_eq!(app.projection.rich.images.len(), 1);
    assert_eq!(app.projection.rich.tables.len(), 1);
}

#[test]
fn overlapping_remote_edit_leaves_details_open_and_does_not_overwrite_new_text() {
    let mut app = app("A [link](https://example.com).\n");
    let mut peer = Document::from_state(&app.session.document.collaboration().state()).unwrap();
    select(&mut app, Selection::caret(5));
    tool(&mut app, Message::Details(Kind::Link));
    peer.transact(
        &[SourceEdit {
            range: 3..7,
            text: "new words".into(),
        }],
        EditOrigin::Human,
        None,
        true,
    )
    .unwrap();
    send(
        &mut app,
        AppMessage::ReplicaLoaded(Ok(peer.collaboration().state())),
    );
    let current = app.session.document.source().to_owned();
    tool(
        &mut app,
        Message::Field(Field::Destination, "https://new.example/".into()),
    );
    tool(&mut app, Message::Accept);
    assert_eq!(app.session.document.source(), current);
    assert!(
        app.writing
            .form
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("changed")
    );
    send(&mut app, AppMessage::CloseOverlay);
    assert_eq!(app.session.document.source(), current);
}

#[test]
fn image_properties_and_quote_credits_preserve_crlf_neighbors_and_exact_undo() {
    let source = "# Heading\r\n\r\n![海](<note.assets/a.png>)\r\n\r\n*Caption.*\r\n\r\nPhoto: [Name](<https://example.com>)\r\n\r\n> A thought.\r\n>\r\n> — Writer, [Essay](<https://example.com>)\r\n\r\nTail 🦀";
    let mut app = app(source);
    select(&mut app, Selection::caret(source.find("Caption").unwrap()));
    tool(&mut app, Message::Details(Kind::Image));
    tool(
        &mut app,
        Message::Field(Field::Label, "海 at dawn 👩‍💻".into()),
    );
    tool(&mut app, Message::Accept);
    assert_eq!(
        app.session.document.source(),
        source.replace("![海]", "![海 at dawn 👩‍💻]")
    );
    send(&mut app, AppMessage::Undo);
    assert_eq!(app.session.document.source(), source);
    select(&mut app, Selection::caret(source.find("Writer").unwrap()));
    tool(&mut app, Message::Details(Kind::Quote));
    tool(
        &mut app,
        Message::Field(Field::Credit, "A different writer".into()),
    );
    tool(&mut app, Message::Accept);
    assert_eq!(
        app.session.document.source(),
        source.replace("Writer,", "A different writer,")
    );
    send(&mut app, AppMessage::Undo);
    assert_eq!(app.session.document.source(), source);
}

#[test]
fn delayed_clipboard_and_image_targets_follow_edits_and_do_not_delete_remote_content() {
    let mut app = app("Before target after.");
    let selection = Selection {
        anchor: 13,
        head: 7,
    };
    select(&mut app, selection);
    let target = DeferredTarget::new(&app, selection);
    let mut peer = Document::from_state(&app.session.document.collaboration().state()).unwrap();
    peer.transact(
        &[SourceEdit {
            range: 7..13,
            text: "remote".into(),
        }],
        EditOrigin::Human,
        None,
        true,
    )
    .unwrap();
    send(
        &mut app,
        AppMessage::ReplicaLoaded(Ok(peer.collaboration().state())),
    );
    let source = app.session.document.source().to_owned();
    tool(
        &mut app,
        Message::Clipboard {
            target,
            result: Ok(assets::Clipboard::Text("日本語 e\u{301} 🦀".into())),
        },
    );
    assert!(app.session.document.source().contains("remote"));
    assert!(app.session.document.source().contains("日本語 e\u{301} 🦀"));
    send(&mut app, AppMessage::Undo);
    assert_eq!(app.session.document.source(), source);
    let target = DeferredTarget::new(&app, Selection::caret(0));
    app.set_document("A different document".into(), None, "New.md".into());
    tool(
        &mut app,
        Message::Imported {
            target,
            label: "Old".into(),
            details: None,
            result: Ok("old.assets/a.png".into()),
        },
    );
    assert_eq!(app.session.document.source(), "A different document");
}

#[test]
fn selected_url_paste_and_ime_commit_have_separate_undo_steps() {
    let mut app = app("A café and a table.\n");
    select(&mut app, Selection { anchor: 7, head: 2 });
    let target = DeferredTarget::new(&app, app.session.selection);
    tool(
        &mut app,
        Message::Clipboard {
            target,
            result: Ok(assets::Clipboard::Text("https://example.com/".into())),
        },
    );
    let linked = app.session.document.source().to_owned();
    assert!(linked.contains("[café](<https://example.com/>)"));
    let target = app
        .session
        .document
        .collaboration()
        .composition_target(app.session.selection);
    send(
        &mut app,
        AppMessage::CommitComposition {
            target,
            text: "日本語".into(),
        },
    );
    assert_eq!(app.session.document.source().matches("日本語").count(), 1);
    send(&mut app, AppMessage::Undo);
    assert_eq!(app.session.document.source(), linked);
    send(&mut app, AppMessage::Undo);
    assert_eq!(app.session.document.source(), "A café and a table.\n");
}

#[test]
fn late_paste_precedes_later_typing_at_its_gap_and_keeps_the_current_caret() {
    let mut app = app("Before after.");
    select(&mut app, Selection::caret(7));
    let target = DeferredTarget::new(&app, app.session.selection);
    send(
        &mut app,
        AppMessage::Edit(text_editor::Action::Edit(text_editor::Edit::Insert('T'))),
    );
    tool(
        &mut app,
        Message::Clipboard {
            target,
            result: Ok(assets::Clipboard::Text("paste".into())),
        },
    );
    assert_eq!(app.session.document.source(), "Before pasteTafter.");
    assert_eq!(
        app.session.selection,
        Selection::caret("Before pasteT".len())
    );
}

#[test]
fn focusing_details_reconciles_editor_focus_and_settles_the_native_redraw_loop() {
    use iced::advanced::{Clipboard, Layout, Shell, layout, mouse, widget};
    use iced::{Event, Rectangle, Renderer, Size};
    struct NoClipboard;
    impl Clipboard for NoClipboard {
        fn read(&self, _: iced::advanced::clipboard::Kind) -> Option<String> {
            None
        }
        fn write(&mut self, _: iced::advanced::clipboard::Kind, _: String) {}
    }
    for size in [Size::new(760.0, 520.0), Size::new(1180.0, 820.0)] {
        for kind in [Kind::Link, Kind::Image, Kind::Quote] {
            let source = "# Heading\n\nA [link](https://example.com).\n\n![alt](image.png)\n\n> A thought.\n>\n> — Name\n";
            let mut app = app(source);
            let head = source
                .find(match kind {
                    Kind::Link => "link",
                    Kind::Image => "![alt]",
                    Kind::Quote => "thought",
                })
                .unwrap();
            select(&mut app, Selection::caret(head));
            let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
                iced::Font::DEFAULT,
                18.0.into(),
            ));
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
            tool(&mut app, Message::Details(kind));
            {
                let mut root = app.view();
                tree.diff(root.as_widget());
                let node = root.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, size),
                );
                root.as_widget_mut().operate(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &mut widget::operation::focusable::focus::<()>(widget::Id::from(
                        "writing-details-first",
                    )),
                );
            }
            let mut messages_on_last_redraw = 0;
            for _ in 0..4 {
                let mut messages = Vec::new();
                {
                    let mut root = app.view();
                    tree.diff(root.as_widget());
                    let node = root.as_widget_mut().layout(
                        &mut tree,
                        &renderer,
                        &layout::Limits::new(Size::ZERO, size),
                    );
                    root.as_widget_mut().update(
                        &mut tree,
                        &Event::Window(iced::window::Event::RedrawRequested(
                            std::time::Instant::now(),
                        )),
                        Layout::new(&node),
                        mouse::Cursor::Unavailable,
                        &renderer,
                        &mut NoClipboard,
                        &mut Shell::new(&mut messages),
                        &Rectangle::with_size(size),
                    );
                }
                messages_on_last_redraw = messages.len();
                for message in messages {
                    send(&mut app, message);
                }
            }
            assert_eq!(
                messages_on_last_redraw, 0,
                "{kind:?} at {size:?} must settle focus without repeated rebuilds"
            );
            assert!(!app.editor_focused);
            assert_eq!(app.session.document.source(), source);
            assert_eq!(app.session.selection, Selection::caret(head));
        }
    }
}

#[test]
fn local_link_navigation_closes_details_and_keeps_unsaved_writing() {
    let mut app = app("# A heading\n\nA [heading](#a-heading) and [file](other.md).\n");
    let dir = tempfile::tempdir().unwrap();
    app.session.assume_saved_file(dir.path().join("current.md"));
    let link = app.session.document.source().find("[heading]").unwrap();
    select(&mut app, Selection::caret(link));
    tool(&mut app, Message::Details(Kind::Link));
    send(&mut app, AppMessage::Link("#a-heading".into()));
    assert!(app.overlay.is_none());
    assert_eq!(app.session.selection.head, 0);
    let original = app.session.document.source().to_owned();
    app.session.require_save();
    let link = app.session.document.source().find("[file]").unwrap();
    select(&mut app, Selection::caret(link));
    tool(&mut app, Message::Details(Kind::Link));
    send(&mut app, AppMessage::Link("other.md".into()));
    assert!(app.loading);
    assert!(app.overlay.is_none());
    send(
        &mut app,
        AppMessage::Loaded(Ok(storage::LoadedFile {
            path: dir.path().join("other.md"),
            source: "# Another document\n".into(),
            collaboration: None,
        })),
    );
    assert!(app.overlay.is_none());
    assert_eq!(app.session.document.source(), "# Another document\n");
    assert!(
        app.background
            .dirty()
            .any(|kept| kept.document.source() == original)
    );
}

#[tokio::test]
async fn save_as_keeps_images_referenced_only_by_undo_history() {
    let old = tempfile::tempdir().unwrap();
    let new = tempfile::tempdir().unwrap();
    let path = old.path().join("essay.md");
    let copy = new.path().join("copy.md");
    let mut references = Vec::new();
    for colour in [[255, 0, 0, 255], [0, 255, 0, 255]] {
        let mut image = image::RgbaImage::new(2, 2);
        image.put_pixel(0, 0, image::Rgba(colour));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        references.push(assets::import(&path, assets::Input::Png(bytes.into_inner())).unwrap());
    }
    let source = format!("![A photograph](<{}>)\n\n*Caption.*\n", references[0]);
    let mut app = app(&source);
    app.session.assume_saved_file(path.clone());
    select(&mut app, Selection::caret(1));
    tool(&mut app, Message::Details(Kind::Image));
    tool(
        &mut app,
        Message::Field(Field::Destination, references[1].clone()),
    );
    tool(&mut app, Message::Accept);
    assert!(!app.session.document.source().contains(&references[0]));
    assets::relocate(
        app.session.document.source(),
        Some(&path),
        &copy,
        &app.images.history(),
    )
    .unwrap();
    let state = app.session.document.collaboration().state();
    let version = collaboration_store::version(&state);
    let source = Arc::new(app.session.document.source().to_owned());
    let generation = app.session.document.collaboration().generation();
    collaboration_store::save_document(copy.clone(), (*source).clone(), state, None, None)
        .await
        .unwrap();
    send(
        &mut app,
        AppMessage::Saved {
            result: Ok(copy),
            source,
            version,
            generation,
        },
    );
    send(&mut app, AppMessage::Undo);
    assert!(app.session.document.source().contains(&references[0]));
    let html = export::html(app.session.document.source(), Some(new.path())).unwrap();
    assert!(html.contains("data:image/png;base64,"));
    for reference in references {
        assert!(
            assets::local_path(&reference, Some(new.path()))
                .unwrap()
                .unwrap()
                .exists()
        );
    }
}

#[test]
fn image_completion_keeps_focus_and_transient_form_edits_in_the_open_dialog() {
    let mut app = app("A [link](https://example.com).\n");
    let target = DeferredTarget::new(&app, Selection::caret(app.session.document.source().len()));
    select(&mut app, Selection::caret(4));
    tool(&mut app, Message::Details(Kind::Link));
    send(&mut app, AppMessage::EditorFocused(false));
    tool(
        &mut app,
        Message::Field(Field::Label, "A label still being edited".into()),
    );
    let task = app.update_writing(Message::Imported {
        target,
        label: "Photograph".into(),
        details: None,
        result: Ok("note.assets/a.png".into()),
    });
    assert_eq!(
        task.units(),
        0,
        "a background completion must not issue a focus operation under the dialog"
    );
    assert!(matches!(app.overlay, Some(Overlay::Writing)));
    assert!(!app.editor_focused);
    assert_eq!(
        app.writing.form.as_ref().unwrap().details.label,
        "A label still being edited"
    );
    assert!(
        app.session
            .document
            .source()
            .contains("![Photograph](<note.assets/a.png>)")
    );
    assert_eq!(app.editor.snapshot(), app.projection.text);
}
