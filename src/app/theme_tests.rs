//! Theme integration and full-window software previews. Never opens a window.

use super::*;
use iced::advanced::{Layout, Renderer as _, layout, mouse, renderer, widget};
use iced::{Rectangle, Renderer, Size};

#[test]
fn switching_appearance_preserves_the_document_and_reverse_selection() {
    let (mut app, _) = App::new();
    app.set_document(
        "# A thought\n\nKeep **these words**.\n".into(),
        None,
        "Thought.md".into(),
    );
    app.session.selection = Selection {
        anchor: 29,
        head: 16,
    };
    drop(app.update_message(Message::Format(Format::Bold)));
    assert!(app.session.document.can_undo());
    let source = app.session.document.source().to_owned();
    let selection = app.session.selection;
    let undo = app.session.document.can_undo();
    for preset in Preset::ALL {
        drop(app.update_message(Message::Theme(preset)));
        assert_eq!(app.session.document.source(), source);
        assert_eq!(app.session.selection, selection);
        assert_eq!(app.session.document.can_undo(), undo);
        assert_eq!(
            app.theme().palette().background,
            app.resolved_theme.tokens.canvas
        );
    }
    let previous = app.resolved_theme.tokens;
    drop(app.update_message(Message::ThemeLoaded(
        app.theme_request,
        Ok(Some(("invalid.toml".into(), "this is not a theme".into()))),
    )));
    assert_eq!(app.resolved_theme.tokens, previous);
    assert_eq!(app.session.document.source(), source);
    assert_eq!(app.session.selection, selection);
    assert_eq!(app.session.document.can_undo(), undo);
}

fn render(app: &App, size: Size, name: &str) {
    render_with_cursor(app, size, name, mouse::Cursor::Unavailable);
}

fn render_with_cursor(app: &App, size: Size, name: &str, cursor: mouse::Cursor) {
    render_scaled(app, size, name, cursor, 1.0);
}

fn render_scaled(app: &App, size: Size, name: &str, cursor: mouse::Cursor, scale: f32) {
    let pixels = render_pixels(app, size, cursor, scale, false);
    let (width, height) = (pixels.width(), pixels.height());
    // Iced's software engine targets the native BGRA window surface.
    let mut rgba = pixels.take();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    image::save_buffer(
        format!("target/theme-validation/{name}.png"),
        &rgba,
        width,
        height,
        image::ColorType::Rgba8,
    )
    .unwrap();
}

fn render_pixels(
    app: &App,
    size: Size,
    cursor: mouse::Cursor,
    scale: f32,
    show_caret: bool,
) -> tiny_skia::Pixmap {
    let mut renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
        styles::BODY_FONT,
        18.0.into(),
    ));
    let mut root = app.view();
    let mut tree = widget::Tree::new(root.as_widget());
    // Responsive widgets resolve their content during layout.
    let node =
        root.as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
    let clip = Rectangle::with_size(size);
    let focus = if app.grid.draft.is_some() {
        Some(table_grid::CELL_EDITOR)
    } else {
        show_caret.then_some(EDITOR)
    };
    if let Some(id) = focus {
        let mut focus = widget::operation::focusable::focus::<()>(widget::Id::from(id));
        root.as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut focus);
    }
    let mut messages = Vec::new();
    let mut shell = iced::advanced::Shell::new(&mut messages);
    root.as_widget_mut().update(
        &mut tree,
        &Event::Window(iced::window::Event::RedrawRequested(
            iced::time::Instant::now(),
        )),
        Layout::new(&node),
        cursor,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut shell,
        &clip,
    );
    renderer.reset(clip);
    root.as_widget().draw(
        &tree,
        &mut renderer,
        &app.theme(),
        &renderer::Style {
            text_color: app.resolved_theme.tokens.text,
        },
        Layout::new(&node),
        cursor,
        &clip,
    );
    let size = Size::new(
        (size.width * scale).round() as u32,
        (size.height * scale).round() as u32,
    );
    let mut pixels = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
    let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
    if let Renderer::Secondary(renderer) = &mut renderer {
        if show_caret {
            let iced::advanced::input_method::InputMethod::Enabled { cursor, .. } =
                shell.input_method()
            else {
                panic!("the focused Write editor positions the input method");
            };
            assert!(renderer.layers().iter().any(|layer| {
                layer.quads.iter().any(|(quad, fill)| {
                    quad.bounds == *cursor
                        && *fill == iced::Background::Color(app.resolved_theme.tokens.accent)
                })
            }));
        }
        renderer.draw(
            &mut pixels.as_mut(),
            &mut mask,
            &iced_tiny_skia::graphics::Viewport::with_physical_size(size, scale),
            &[clip],
            app.resolved_theme.tokens.canvas,
        );
    }
    pixels
}

#[test]
fn write_caret_keeps_the_same_pixels_when_filling_and_clearing_a_line() {
    use iced::widget::text_editor::Edit;

    fn caret(app: &App, scale: f32) -> tiny_skia::IntRect {
        let pixels = render_pixels(
            app,
            Size::new(760.0, 520.0),
            mouse::Cursor::Unavailable,
            scale,
            true,
        );
        let (mut left, mut top, mut right, mut bottom) = (pixels.width(), pixels.height(), 0, 0);
        let width = pixels.width() as usize;
        for (y, row) in pixels.data().chunks_exact(width * 4).enumerate() {
            // Stay inside the writing area, away from the titlebar and statistics.
            if y < (70.0 * scale) as usize || y >= (225.0 * scale) as usize {
                continue;
            }
            for (x, pixel) in row.chunks_exact(4).enumerate() {
                if pixel == [255, 0, 255, 255] {
                    left = left.min(x as u32);
                    top = top.min(y as u32);
                    right = right.max(x as u32 + 1);
                    bottom = bottom.max(y as u32 + 1);
                }
            }
        }
        tiny_skia::IntRect::from_ltrb(left as i32, top as i32, right as i32, bottom as i32)
            .expect("the focused editor paints an opaque caret")
    }

    for scale in [1.0, 1.25, 2.0] {
        for source in ["", "test", "test\n\nAfter"] {
            let (mut app, _) = App::new();
            app.resolved_theme = Preset::Paper.resolve();
            app.resolved_theme.typography.font = WritingFont::SourceSans3;
            app.resolved_theme.tokens.accent = iced::Color::from_rgb(1.0, 0.0, 1.0);
            let rescaled = event_message(
                Event::Window(window::Event::Rescaled(scale)),
                iced::event::Status::Ignored,
                window::Id::unique(),
            )
            .expect("window scaling reaches the editor");
            drop(app.update_message(rescaled));
            app.set_document(source.into(), None, "Untitled.md".into());
            let head = if source.contains('\n') {
                5
            } else {
                source.len()
            };
            drop(app.update_message(Message::SelectSource(Selection::caret(head))));
            if source == "test" {
                drop(app.update_message(Message::Edit(text_editor::Action::Edit(Edit::Enter))));
            }
            let blank = app.session.document.source().to_owned();
            let head = app.session.selection.head;
            let before = caret(&app, scale);
            assert_eq!(
                before.height(),
                (app.resolved_theme.typography.body_size * scale).round() as u32
            );
            drop(app.update_message(Message::Edit(text_editor::Action::Edit(Edit::Insert('T')))));
            assert_eq!(app.session.selection, Selection::caret(head + 1));
            let typed = caret(&app, scale);
            drop(app.update_message(Message::Edit(text_editor::Action::Edit(Edit::Backspace))));
            assert_eq!(app.session.document.source(), blank);
            assert_eq!(before, caret(&app, scale));
            assert!(typed.left() > before.left());
            assert_eq!(
                (before.top(), before.height(), before.width()),
                (typed.top(), typed.height(), typed.width()),
                "{blank:?} at {scale}×"
            );
        }
    }
}

#[test]
fn write_placeholder_uses_the_same_position_as_typed_text() {
    let mut fonts = iced_tiny_skia::graphics::text::font_system()
        .write()
        .unwrap();
    for font in crate::theme::BUNDLED_WRITING_FONTS {
        fonts.load_font(std::borrow::Cow::Borrowed(font));
    }
    drop(fonts);

    for font in WritingFont::ALL {
        let (mut app, _) = App::new();
        app.resolved_theme = Preset::Paper.resolve();
        app.resolved_theme.typography.font = font;
        app.resolved_theme.tokens.faint = app.resolved_theme.tokens.text;
        app.preferences.writing_zoom = 1.15;
        let mut render_text = |source: &str| {
            app.set_document(source.into(), None, "Untitled.md".into());
            render_pixels(
                &app,
                Size::new(760.0, 520.0),
                mouse::Cursor::Unavailable,
                2.0,
                false,
            )
        };
        let placeholder = render_text("");
        let typed = render_text(crate::editor::WRITING_PLACEHOLDER);
        let stride = placeholder.width() as usize * 4;
        let writing_band = 172 * stride..250 * stride;
        assert!(
            placeholder.data()[writing_band.clone()] == typed.data()[writing_band],
            "placeholder and typed glyphs differ for {font}"
        );
    }
}

#[test]
#[ignore = "render a pasted spreadsheet range and its quiet reading presentation"]
fn table_clipboard_previews() {
    let mut fonts = iced_tiny_skia::graphics::text::font_system()
        .write()
        .unwrap();
    for font in crate::theme::BUNDLED_WRITING_FONTS {
        fonts.load_font(std::borrow::Cow::Borrowed(font));
    }
    drop(fonts);
    std::fs::create_dir_all("target/theme-validation").unwrap();
    for preset in Preset::ALL {
        let (mut app, task) = App::new();
        drop(task);
        app.set_document(
            include_str!("../../tests/fixtures/table-grid.md").into(),
            None,
            "Launch plan.md".into(),
        );
        app.resolved_theme = preset.resolve();
        app.chrome.files.open = false;
        app.notification = None;
        app.editor_focused = true;
        drop(app.update_grid(table_grid::Message::Select {
            revision: Arc::clone(&app.projection.source_id),
            table: 0,
            row: 0,
            column: 0,
            extend: false,
            extent: table_grid::Extent::Cell,
        }));
        drop(app.update_grid(table_grid::Message::Paste(
            "Milestone\tOwner\tStatus\tNotes\r\nDraft proposal\tMaya\tIn progress\t\"First draft\nReview Friday\"\r\nReview together\tNoah\tReady\tBring the team\r\nPublish the plan\tAri\tPlanned\t\"Say \"\"hello\"\"\"\r\nShare the results\tLéa\tPlanned\t".into(),
        )));
        assert_eq!(app.grid_view().unwrap().head, (4, 3));
        assert!(app.notification.is_none());
        for (size, suffix) in [
            (Size::new(1180.0, 820.0), ""),
            (Size::new(760.0, 520.0), "-narrow"),
        ] {
            render(
                &app,
                size,
                &format!("{}-clipboard-range{suffix}", preset.name()),
            );
        }
        drop(app.update_grid(table_grid::Message::Cancel));
        drop(app.update_grid(table_grid::Message::Cancel));
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("{}-clipboard-quiet", preset.name()),
        );
    }
}

#[test]
#[ignore = "render document table editing and its contextual footer in every theme"]
fn document_table_previews() {
    let mut fonts = iced_tiny_skia::graphics::text::font_system()
        .write()
        .unwrap();
    for font in [
        include_bytes!("../../assets/fonts/Newsreader-Regular.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-Bold.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-Italic.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-BoldItalic.ttf").as_slice(),
    ] {
        fonts.load_font(std::borrow::Cow::Borrowed(font));
    }
    drop(fonts);
    std::fs::create_dir_all("target/theme-validation").unwrap();
    let (mut app, _) = App::new();
    let source = include_str!("../../tests/fixtures/document-tables.md");
    app.set_document(source.into(), None, "Document tables.md".into());
    app.chrome.files.open = false;
    app.notification = None;
    app.mode = EditorMode::Live;
    app.editor_focused = true;
    app.session.selection = Selection::caret(source.find("Second").unwrap() + 3);
    app.sync_editor();
    for preset in Preset::ALL {
        app.resolved_theme = preset.resolve();
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("{}-document-table", preset.name()),
        );
        render(
            &app,
            Size::new(760.0, 520.0),
            &format!("{}-document-table-narrow", preset.name()),
        );
        app.mode = EditorMode::Read;
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("{}-document-table-read", preset.name()),
        );
        app.mode = EditorMode::Live;
    }
}

#[test]
#[ignore = "render spreadsheet cell/range selection and native draft editing in all themes"]
fn table_grid_previews() {
    let mut fonts = iced_tiny_skia::graphics::text::font_system()
        .write()
        .unwrap();
    for font in [
        include_bytes!("../../assets/fonts/Newsreader-Regular.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-Bold.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-Italic.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-BoldItalic.ttf").as_slice(),
    ] {
        fonts.load_font(std::borrow::Cow::Borrowed(font));
    }
    drop(fonts);
    std::fs::create_dir_all("target/theme-validation").unwrap();
    let (mut app, task) = App::new();
    drop(task);
    app.set_document(
        include_str!("../../tests/fixtures/table-grid.md").into(),
        None,
        "Launch plan.md".into(),
    );
    app.chrome.files.open = false;
    app.notification = None;
    app.mode = EditorMode::Live;
    app.editor_focused = true;
    for preset in Preset::ALL {
        app.resolved_theme = preset.resolve();
        app.clear_grid();
        app.session.selection =
            Selection::caret(app.session.document.source().find("Keep the team").unwrap());
        app.sync_editor();
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("{}-table-quiet", preset.name()),
        );
        render(
            &app,
            Size::new(760.0, 520.0),
            &format!("{}-table-quiet-narrow", preset.name()),
        );
        for (row, column, extend) in [(1, 1, false), (3, 2, true)] {
            drop(app.update_grid(table_grid::Message::Select {
                revision: Arc::clone(&app.projection.source_id),
                table: 0,
                row,
                column,
                extend,
                extent: table_grid::Extent::Cell,
            }));
        }
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("{}-table-range", preset.name()),
        );
        render(
            &app,
            Size::new(760.0, 520.0),
            &format!("{}-table-range-narrow", preset.name()),
        );
        drop(app.update_grid(table_grid::Message::Select {
            revision: Arc::clone(&app.projection.source_id),
            table: 0,
            row: 1,
            column: 1,
            extend: false,
            extent: table_grid::Extent::Cell,
        }));
        drop(app.update_grid(table_grid::Message::Begin(Some("Maya\nDesign lead".into()))));
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("{}-table-draft", preset.name()),
        );
        render(
            &app,
            Size::new(760.0, 520.0),
            &format!("{}-table-draft-narrow", preset.name()),
        );
        drop(app.update_grid(table_grid::Message::Cancel));
    }
}

#[test]
#[ignore = "render Dosty themes, narrow windows, overlays, and focus mode for visual inspection"]
fn theme_ui_previews() {
    let mut fonts = iced_tiny_skia::graphics::text::font_system()
        .write()
        .unwrap();
    for font in crate::theme::BUNDLED_WRITING_FONTS {
        fonts.load_font(std::borrow::Cow::Borrowed(font));
    }
    drop(fonts);
    std::fs::create_dir_all("target/theme-validation").unwrap();
    let (mut app, _) = App::new();
    app.preferences.custom_theme = None;
    app.chrome.files.open = true;
    app.chrome.sidebars_hidden = false;
    app.notification = None;
    app.workspace
        .remember_for_preview(&["/writing/Field notes", "/writing/Studio essays"]);
    app.chrome.files.open = false;
    render(&app, Size::new(760.0, 520.0), "project-home-narrow");
    drop(app.update_projects(super::projects::Message::New));
    drop(app.update_projects(super::projects::Message::Name("Field Guide".into())));
    drop(
        app.update_projects(super::projects::Message::LocationPicked(Some(
            "/writing".into(),
        ))),
    );
    render(&app, Size::new(760.0, 520.0), "new-project-narrow");
    drop(app.update_message(Message::CloseOverlay));
    app.chrome.files.open = true;
    for preset in Preset::ALL {
        app.resolved_theme = preset.resolve();
        render(&app, Size::new(1180.0, 820.0), preset.name());
        render(
            &app,
            Size::new(760.0, 520.0),
            &format!("{}-narrow", preset.name()),
        );
        app.overlay = Some(Overlay::Settings);
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("{}-settings", preset.name()),
        );
        render(
            &app,
            Size::new(760.0, 520.0),
            &format!("{}-settings-narrow", preset.name()),
        );
        app.overlay = None;
    }
    app.resolved_theme = Preset::Paper.resolve();
    app.session.display_name = "Notes for a very long and unbroken document title that should never push controls out of the window.md".into();
    render(&app, Size::new(760.0, 520.0), "long-title-narrow");
    app.session.display_name = "Welcome.md".into();
    app.chrome.sidebars_hidden = true;
    render(&app, Size::new(1180.0, 820.0), "focus");
    app.chrome.sidebars_hidden = false;
    app.search_open = true;
    render(&app, Size::new(760.0, 520.0), "search-narrow");
    app.search_open = false;
    drop(app.update_message(Message::ToggleSource));
    render(&app, Size::new(1180.0, 820.0), "source");
    drop(app.update_message(Message::ToggleReading));
    render(&app, Size::new(1180.0, 820.0), "read");
    drop(app.update_message(Message::ToggleReading));
    app.session.dirty = true;
    app.overlay = Some(Overlay::Unsaved);
    render(&app, Size::new(760.0, 520.0), "unsaved-narrow");
    app.overlay = None;
    app.set_document(
        include_str!("../../tests/fixtures/theming.md").into(),
        None,
        "Code.md".into(),
    );
    for preset in Preset::ALL {
        app.resolved_theme = preset.resolve();
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("{}-code", preset.name()),
        );
    }
}

#[test]
#[ignore = "render Native quiet titlebar, save states, settings, and navigation for visual inspection"]
fn native_quiet_previews() {
    let mut fonts = iced_tiny_skia::graphics::text::font_system()
        .write()
        .unwrap();
    for font in crate::theme::BUNDLED_WRITING_FONTS {
        fonts.load_font(std::borrow::Cow::Borrowed(font));
    }
    drop(fonts);
    std::fs::create_dir_all("target/theme-validation").unwrap();
    let (mut app, initial) = App::new();
    drop(initial);
    app.preferences = Preferences::default();
    app.resolved_theme = Preset::Paper.resolve();
    app.resolved_theme.typography.font = WritingFont::SourceSans3;
    for (suffix, name) in [
        ("", "heading-marker"),
        (" A clear heading", "heading-title"),
    ] {
        let source = format!("A line of prose.\n\n#{suffix}");
        let end = source.len();
        app.set_document(source, None, "Untitled.md".into());
        drop(app.update_message(Message::SelectSource(Selection::caret(end))));
        render_scaled(
            &app,
            Size::new(760.0, 520.0),
            name,
            mouse::Cursor::Unavailable,
            2.0,
        );
    }
    app.set_document("# The quiet hour\n\n*A small field note on making room to notice.*\n\nAt the edge of the morning, the coast is almost colourless. Water folds around the rocks. A path disappears into the grass. For a few minutes, there is nothing to answer and nowhere else to be.\n\n## Making room\n\nWe came here to write about attention. We stayed because the light kept changing.\n".into(), Some("/tmp/The quiet hour.md".into()), "The quiet hour.md".into());
    for preset in Preset::ALL {
        app.resolved_theme = preset.resolve();
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("native-quiet-{}", preset.name()),
        );
        render(
            &app,
            Size::new(760.0, 520.0),
            &format!("native-quiet-{}-narrow", preset.name()),
        );
    }
    app.resolved_theme = Preset::Paper.resolve();
    app.session.dirty = true;
    render(&app, Size::new(1180.0, 820.0), "native-quiet-typing");
    app.chrome.outline.open = true;
    render(&app, Size::new(1180.0, 820.0), "native-quiet-navigation");
    render(&app, Size::new(760.0, 520.0), "native-quiet-outline-narrow");
    app.session
        .set_path("/writing/Field notes/The quiet hour.md".into());
    drop(
        app.update_folder(folder::Message::Scanned {
            generation: 0,
            result: Ok(crate::io::folder::Snapshot {
                root: "/writing/Field notes".into(),
                entries: [
                    ("The quiet hour.md", false),
                    ("Launch notes.md", false),
                    ("Research", true),
                    ("Research/Source notes.md", false),
                    ("Research/A clearer thought.md", false),
                ]
                .into_iter()
                .map(|(name, is_directory)| crate::io::folder::Entry {
                    path: std::path::Path::new("/writing/Field notes").join(name),
                    is_directory,
                })
                .collect(),
                limited: false,
                warnings: Vec::new(),
            }),
        }),
    );
    app.chrome.files.open = true;
    render(&app, Size::new(1180.0, 820.0), "native-quiet-both-sidebars");
    render(&app, Size::new(760.0, 520.0), "native-quiet-both-narrow");
    drop(app.show_sidebar(Sidebar::Outline));
    render(
        &app,
        Size::new(760.0, 520.0),
        "native-quiet-narrow-outline-active",
    );
    drop(app.show_sidebar(Sidebar::Files));
    app.workspace.remember_for_preview(&[
        "/writing/Field notes",
        "/Users/writer/Essays",
        "/Volumes/Archive/Essays",
        "/Users/writer/Letters",
    ]);
    drop(app.update_quick_open(quick_open::Message::Open));
    render(&app, Size::new(1180.0, 820.0), "workspace-quick-open");
    render(&app, Size::new(760.0, 520.0), "workspace-quick-open-narrow");
    drop(app.update_quick_open(quick_open::Message::Query("research".into())));
    render(
        &app,
        Size::new(1180.0, 820.0),
        "workspace-quick-open-filtered",
    );
    drop(app.cancel_quick_open());
    drop(app.update_chrome(chrome::Message::Menu(chrome::Menu::Workspace)));
    render(&app, Size::new(1180.0, 820.0), "workspace-menu");
    drop(app.update_message(Message::CloseOverlay));
    drop(app.update_chrome(chrome::Message::Menu(chrome::Menu::Projects)));
    render(&app, Size::new(1180.0, 820.0), "projects-menu");
    drop(app.update_message(Message::CloseOverlay));
    for font in WritingFont::ALL {
        app.resolved_theme.typography.font = font;
        app.preferences.writing_font = font;
        render(&app, Size::new(1180.0, 820.0), &format!("typeface-{font}"));
        render_scaled(
            &app,
            Size::new(1180.0, 820.0),
            &format!("typeface-{font}-2x"),
            mouse::Cursor::Unavailable,
            2.0,
        );
        app.mode = EditorMode::Read;
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("typeface-{font}-read"),
        );
        app.mode = EditorMode::Live;
    }
    app.resolved_theme.typography.font = WritingFont::SourceSans3;
    app.preferences.writing_font = WritingFont::SourceSans3;
    app.chrome.files.open = false;
    app.chrome.outline.open = false;
    app.overlay = Some(Overlay::Settings);
    render(
        &app,
        Size::new(760.0, 520.0),
        "native-quiet-settings-narrow",
    );
    render(&app, Size::new(1180.0, 1000.0), "typeface-settings");
    for (section, name) in [
        (SettingsSection::Appearance, "appearance-settings"),
        (SettingsSection::Shortcuts, "shortcut-settings"),
    ] {
        drop(app.update_message(Message::SettingsSection(section)));
        render(&app, Size::new(760.0, 520.0), name);
    }
    app.resolved_theme = Preset::Midnight.resolve();
    drop(app.update_message(Message::SettingsSection(SettingsSection::Writing)));
    render(&app, Size::new(760.0, 520.0), "writing-settings-midnight");
    app.resolved_theme = Preset::Paper.resolve();
    app.overlay = None;
    drop(app.update_chrome(chrome::Message::Menu(chrome::Menu::Format)));
    render(&app, Size::new(760.0, 520.0), "format-narrow");
    drop(app.update_message(Message::CloseOverlay));
    app.session.display_name =
        "A very long document name that must never push the save indicator outside the titlebar.md"
            .into();
    app.chrome.save_error = Some("This file changed outside Dosty.".into());
    render(
        &app,
        Size::new(760.0, 520.0),
        "native-quiet-save-failed-narrow",
    );
}

#[test]
#[ignore = "render the sidebar without a place and with unsaved documents for visual inspection"]
fn sidebar_unsaved_previews() {
    let (mut app, _) = App::new();
    app.set_document(
        "# A thought\n\nUnfinished.".into(),
        None,
        "A thought.md".into(),
    );
    app.session.dirty = true;
    drop(app.update(Message::New));
    app.chrome.files.open = true;
    render(&app, Size::new(1180.0, 820.0), "sidebar-library-empty");

    let root = std::path::Path::new("/writing/Field notes");
    drop(
        app.update_folder(folder::Message::Scanned {
            generation: 0,
            result: Ok(crate::io::folder::Snapshot {
                root: root.to_owned(),
                entries: [
                    ("The quiet hour.md", false),
                    ("Launch notes.md", false),
                    ("Research", true),
                    ("Research/Source notes.md", false),
                    ("Drafts", true),
                    ("Drafts/Opening.md", false),
                ]
                .into_iter()
                .map(|(name, is_directory)| crate::io::folder::Entry {
                    path: root.join(name),
                    is_directory,
                })
                .collect(),
                limited: false,
                warnings: Vec::new(),
            }),
        }),
    );
    for name in [
        "Research/Source notes.md",
        "Launch notes.md",
        "The quiet hour.md",
    ] {
        app.set_document(
            "Text".into(),
            Some(root.join(name)),
            name.rsplit('/').next().unwrap().into(),
        );
        app.session.dirty = true;
    }
    app.notification = None;
    app.chrome.files.open = true;
    render(&app, Size::new(1180.0, 820.0), "sidebar-unsaved");
}

#[tokio::test]
#[ignore = "render the image preview and image rows for visual inspection"]
async fn image_previews() {
    let (mut app, _) = App::new();
    app.set_document(
        include_str!("../../assets/writing/the-quiet-hour.md").into(),
        Some("/writing/Field notes/The quiet hour.md".into()),
        "The quiet hour.md".into(),
    );
    drop(
        app.update_folder(folder::Message::Scanned {
            generation: 0,
            result: Ok(crate::io::folder::Snapshot {
                root: "/writing/Field notes".into(),
                entries: [
                    ("The quiet hour.md", false),
                    ("coast.png", false),
                    ("Research", true),
                ]
                .into_iter()
                .map(|(name, is_directory)| crate::io::folder::Entry {
                    path: std::path::Path::new("/writing/Field notes").join(name),
                    is_directory,
                })
                .collect(),
                limited: false,
                warnings: Vec::new(),
            }),
        }),
    );
    app.chrome.files.open = true;
    drop(app.update_quick_open(quick_open::Message::Open));
    render(&app, Size::new(1180.0, 820.0), "image-switcher");
    drop(app.update_quick_open(quick_open::Message::Cancel));
    let coast = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/writing/coast.png");
    drop(
        app.update(Message::ImagePreview(super::image_preview::Message::Show(
            coast.clone(),
        ))),
    );
    render(&app, Size::new(1180.0, 820.0), "image-preview-opening");
    let result = crate::io::image_store::load_path(coast).await;
    assert!(result.is_ok());
    drop(app.update(Message::ImagePreview(
        super::image_preview::Message::Loaded { request: 1, result },
    )));
    render(&app, Size::new(1180.0, 820.0), "image-preview");
}

#[test]
#[ignore = "render the home surface with and without places for visual inspection"]
fn home_previews() {
    let (mut app, _) = App::new();
    app.workspace.remember_for_preview(&[]);
    render(&app, Size::new(1180.0, 820.0), "home-first-launch");
    app.workspace.remember_for_preview(&[
        "/writing/Field notes",
        "/Users/writer/Essays",
        "/Volumes/Archive/Essays",
    ]);
    render(&app, Size::new(1180.0, 820.0), "home-places");
    render(&app, Size::new(560.0, 520.0), "home-narrow");
    app.resolved_theme = Preset::Midnight.resolve();
    render(&app, Size::new(1180.0, 820.0), "home-midnight");
}

#[test]
#[ignore = "render table controls in every preset and narrow windows without opening native documents"]
fn table_controls_previews() {
    let mut fonts = iced_tiny_skia::graphics::text::font_system()
        .write()
        .unwrap();
    for font in [
        include_bytes!("../../assets/fonts/Newsreader-Regular.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-Bold.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-Italic.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-BoldItalic.ttf").as_slice(),
    ] {
        fonts.load_font(std::borrow::Cow::Borrowed(font));
    }
    drop(fonts);
    std::fs::create_dir_all("target/theme-validation").unwrap();
    let source = include_str!("../../tests/fixtures/table-controls.md");
    let (mut app, task) = App::new();
    drop(task);
    app.set_document(source.into(), None, "Tables.md".into());
    app.session.selection = Selection::caret(source.find("Name").unwrap());
    app.editor_focused = true;
    app.sync_editor();
    drop(app.update(Message::Table(table::Message::Open)));
    for preset in Preset::ALL {
        app.resolved_theme = preset.resolve();
        render(
            &app,
            Size::new(1180.0, 820.0),
            &format!("{}-table-controls", preset.name()),
        );
        render(
            &app,
            Size::new(760.0, 520.0),
            &format!("{}-table-controls-narrow", preset.name()),
        );
    }
}

#[test]
#[ignore = "render contextual table menus in all themes and at viewport edges"]
fn table_context_previews() {
    let mut fonts = iced_tiny_skia::graphics::text::font_system()
        .write()
        .unwrap();
    for font in [
        include_bytes!("../../assets/fonts/Newsreader-Regular.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-Bold.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-Italic.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-BoldItalic.ttf").as_slice(),
    ] {
        fonts.load_font(std::borrow::Cow::Borrowed(font));
    }
    drop(fonts);
    std::fs::create_dir_all("target/theme-validation").unwrap();
    let source = include_str!("../../tests/fixtures/table-grid.md");
    for preset in Preset::ALL {
        for (size, suffix) in [
            (Size::new(1180.0, 820.0), ""),
            (Size::new(760.0, 520.0), "-narrow"),
        ] {
            for (extent, label, row, column) in [
                (table_grid::Extent::Cell, "cell", 1, 1),
                (table_grid::Extent::Row, "row", 1, 0),
                (table_grid::Extent::Column, "column", 0, 1),
            ] {
                let (mut app, task) = App::new();
                drop(task);
                app.set_document(source.into(), None, "Launch plan.md".into());
                app.resolved_theme = preset.resolve();
                app.chrome.files.open = false;
                app.editor_focused = true;
                app.session.selection = Selection::caret(source.find("Keep the team").unwrap());
                app.sync_editor();
                drop(
                    app.update_message(Message::Table(table::Message::OpenContext {
                        revision: std::sync::Arc::clone(&app.projection.source_id),
                        table: 0,
                        row,
                        column,
                        extent,
                        position: if suffix.is_empty() {
                            iced::Point::new(
                                if extent == table_grid::Extent::Row {
                                    216.0
                                } else {
                                    476.0
                                },
                                if extent == table_grid::Extent::Column {
                                    276.0
                                } else {
                                    354.0
                                },
                            )
                        } else {
                            iced::Point::new(700.0, 480.0)
                        },
                    })),
                );
                render(
                    &app,
                    size,
                    &format!("{}-table-menu-{label}{suffix}", preset.name()),
                );
            }
        }
    }
}

#[test]
#[ignore = "render hover-only table controls without opening native windows"]
fn table_hover_previews() {
    let mut fonts = iced_tiny_skia::graphics::text::font_system()
        .write()
        .unwrap();
    for font in [
        include_bytes!("../../assets/fonts/Newsreader-Regular.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-Bold.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-Italic.ttf").as_slice(),
        include_bytes!("../../assets/fonts/Newsreader-BoldItalic.ttf").as_slice(),
    ] {
        fonts.load_font(std::borrow::Cow::Borrowed(font));
    }
    drop(fonts);
    std::fs::create_dir_all("target/theme-validation").unwrap();
    let source = include_str!("../../tests/fixtures/table-grid.md");
    for preset in Preset::ALL {
        for (size, suffix) in [
            (Size::new(1180.0, 820.0), ""),
            (Size::new(760.0, 520.0), "-narrow"),
        ] {
            let (mut app, task) = App::new();
            drop(task);
            app.set_document(source.into(), None, "Launch plan.md".into());
            app.resolved_theme = preset.resolve();
            app.chrome.files.open = false;
            app.editor_focused = true;
            drop(app.update_grid(table_grid::Message::Select {
                revision: std::sync::Arc::clone(&app.projection.source_id),
                table: 0,
                row: 1,
                column: 1,
                extend: false,
                extent: table_grid::Extent::Cell,
            }));
            for (label, point) in [
                (
                    "row",
                    iced::Point::new(
                        if suffix.is_empty() { 216.0 } else { 18.0 },
                        if suffix.is_empty() { 372.0 } else { 305.0 },
                    ),
                ),
                (
                    "column",
                    iced::Point::new(
                        if suffix.is_empty() { 692.0 } else { 478.0 },
                        if suffix.is_empty() { 275.0 } else { 207.0 },
                    ),
                ),
            ] {
                render_with_cursor(
                    &app,
                    size,
                    &format!("{}-table-hover-{label}{suffix}", preset.name()),
                    mouse::Cursor::Available(point),
                );
            }
        }
    }
}
