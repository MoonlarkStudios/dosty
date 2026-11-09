//! One focus, selection, clipboard, and input-method surface over rich layout.
//!
//! Editing actions still go through Iced's native buffer and the lossless
//! projection. Geometry-dependent actions use the exact paragraphs we draw.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::advanced::image::{self, Renderer as _};
use iced::advanced::text::{self, Renderer as _};
use iced::advanced::widget::{self, operation};
use iced::advanced::{Clipboard, Layout, Shell, Widget, input_method, layout, mouse, renderer};
use iced::keyboard;
use iced::widget::text_editor::{Action, Binding, Content, Edit, KeyPress, Motion, Status};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector, window};
use unicode_segmentation::UnicodeSegmentation;

use crate::app::{
    FootnoteMessage, Message, TableExtent, TableGridMessage, TableGridView, TableMessage,
    WritingMessage,
};
use crate::document::collaboration::{Collaboration, CompositionTarget, StablePosition};
use crate::document::projection::{HighlightLines, Projection};
use crate::document::selection::Selection;
use crate::editor::rich_layout::{Decoration, FragmentKind, LayoutInput};
use crate::editor::virtual_layout::VirtualLayout;
use crate::io::image_store::ImageStore;
use crate::theme::{ResolvedTheme, Tokens, Typography};
use yrs::Assoc;

pub struct RichEditor<'a> {
    content: &'a Content,
    collaboration: Option<&'a Collaboration>,
    projection: &'a Projection,
    source: &'a str,
    theme: &'a ResolvedTheme,
    images: &'a ImageStore,
    base: Option<&'a Path>,
    zoom: f32,
    source_selection: Selection,
    app_focused: bool,
    preserve_viewport: bool,
    key_binding: fn(KeyPress) -> Option<Binding<Message>>,
    grid: Option<TableGridView>,
    grid_resume: Option<TableGridView>,
    cell_editor: Option<Element<'a, Message>>,
}

/// Locate the cell in the live widget so keyboard and toolbar menus share the
/// pointer menu's measured, window-relative anchor without storing UI geometry.
pub(crate) fn open_table_menu(
    revision: Arc<()>,
    table: usize,
    row: usize,
    column: usize,
) -> iced::Task<Message> {
    struct Locate {
        revision: Arc<()>,
        table: usize,
        row: usize,
        column: usize,
        position: Option<Point>,
    }
    impl widget::Operation<Message> for Locate {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation<Message>)) {
            operate(self);
        }
        fn custom(
            &mut self,
            _id: Option<&widget::Id>,
            bounds: Rectangle,
            state: &mut dyn std::any::Any,
        ) {
            if let Some(state) = state.downcast_mut::<State>() {
                let cell = state
                    .rich
                    .resident()
                    .cell_bounds(self.table, self.row, self.column);
                self.position = Some(
                    bounds.position()
                        + Vector::new(
                            cell.map_or(0.0, |cell| cell.x),
                            cell.map_or(0.0, |cell| cell.y + cell.height - state.scroll)
                                .clamp(0.0, bounds.height),
                        ),
                );
            }
        }
        fn finish(&self) -> operation::Outcome<Message> {
            self.position.map_or(operation::Outcome::None, |position| {
                operation::Outcome::Some(Message::Table(TableMessage::OpenContext {
                    revision: Arc::clone(&self.revision),
                    table: self.table,
                    row: self.row,
                    column: self.column,
                    position,
                    extent: TableExtent::Cell,
                }))
            })
        }
    }
    widget::operate(Locate {
        revision,
        table,
        row,
        column,
        position: None,
    })
}

impl<'a> RichEditor<'a> {
    #[cfg(test)]
    pub(crate) fn table_cell(
        tree: &widget::Tree,
        table: usize,
        row: usize,
        column: usize,
    ) -> Option<Rectangle> {
        let state = tree.state.downcast_ref::<State>();
        state
            .rich
            .resident()
            .cell_bounds(table, row, column)
            .map(|rect| rect - Vector::new(0.0, state.scroll))
    }
    #[cfg(test)]
    pub(crate) fn profile_stats(tree: &widget::Tree) -> (usize, usize) {
        let state = tree.state.downcast_ref::<State>();
        (
            state.rich.resident().fragments.len(),
            state.rich.resident().rows.len(),
        )
    }

    #[cfg(test)]
    pub(crate) fn intent_stats(tree: &widget::Tree) -> (f32, bool, Option<usize>) {
        let state = tree.state.downcast_ref::<State>();
        (state.scroll, state.preedit.is_some(), state.drag_anchor)
    }
    #[cfg(test)]
    pub(crate) fn visible_anchor(tree: &widget::Tree) -> Option<usize> {
        let state = tree.state.downcast_ref::<State>();
        state
            .rich
            .resident()
            .viewport_anchor(state.scroll)
            .map(|(offset, _)| offset)
    }
    #[cfg(test)]
    pub(crate) fn screen_y(tree: &widget::Tree, source: usize) -> f32 {
        let state = tree.state.downcast_ref::<State>();
        state.caret(source).y - state.scroll
    }
    pub fn new(
        content: &'a Content,
        projection: &'a Projection,
        source: &'a str,
        theme: &'a ResolvedTheme,
        images: &'a ImageStore,
        key_binding: fn(KeyPress) -> Option<Binding<Message>>,
    ) -> Self {
        Self {
            content,
            collaboration: None,
            projection,
            source,
            theme,
            images,
            base: None,
            zoom: 1.0,
            source_selection: Selection::default(),
            app_focused: false,
            preserve_viewport: false,
            key_binding,
            grid: None,
            grid_resume: None,
            cell_editor: None,
        }
    }

    pub fn preserve_viewport(mut self, preserve: bool) -> Self {
        self.preserve_viewport = preserve;
        self
    }
    pub(crate) fn table_grid(
        mut self,
        grid: Option<TableGridView>,
        editor: Option<Element<'a, Message>>,
    ) -> Self {
        self.grid = grid;
        self.cell_editor = editor;
        self
    }
    pub fn collaboration(mut self, collaboration: &'a Collaboration) -> Self {
        self.collaboration = Some(collaboration);
        self
    }
    pub(crate) fn table_resume(mut self, grid: Option<TableGridView>) -> Self {
        self.grid_resume = grid;
        self
    }
    pub fn base(mut self, base: Option<&'a Path>) -> Self {
        self.base = base;
        self
    }
    pub fn zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom;
        self
    }

    pub fn selection(mut self, selection: Selection) -> Self {
        self.source_selection = selection;
        self
    }
    pub fn focused(mut self, focused: bool) -> Self {
        self.app_focused = focused;
        self
    }
    fn layout_input(&self, width: f32) -> LayoutInput<'_> {
        LayoutInput {
            projection: self.projection,
            theme: self.theme,
            zoom: self.zoom,
            width: (width - 12.0).max(1.0),
            base: self.base,
            images: self.images,
        }
    }
    fn refresh_viewport(&self, state: &mut State, bounds: Size) {
        state.scroll =
            state
                .rich
                .show_viewport(self.layout_input(bounds.width), state.scroll, bounds.height);
        state.clamp_scroll();
    }

    fn raw_selection(&self, selection: Selection) -> Selection {
        self.projection.normalize_caret(
            self.projection
                .source_selection(selection, self.source_selection),
        )
    }

    fn display_selection(&self) -> Selection {
        self.projection.display_selection(self.source_selection)
    }

    fn word_selection(&self, offset: usize) -> Selection {
        let range = self
            .projection
            .text
            .split_word_bound_indices()
            .find(|(start, word)| *start <= offset && offset < *start + word.len())
            .map_or(offset..offset, |(start, word)| start..start + word.len());
        Selection {
            anchor: range.start,
            head: range.end,
        }
    }

    fn line_selection(&self, offset: usize) -> Selection {
        let text = &self.projection.text;
        Selection {
            anchor: text[..offset].rfind('\n').map_or(0, |n| n + 1),
            head: text[offset..]
                .find('\n')
                .map_or(text.len(), |n| offset + n + 1),
        }
    }

    fn apply_binding(
        &self,
        binding: Binding<Message>,
        state: &mut State,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        bounds: Size,
    ) {
        if !matches!(binding, Binding::Move(_) | Binding::Select(_)) {
            state.preferred_x = None;
        }
        match binding {
            Binding::Unfocus => {
                state.unfocus();
                shell.publish(Message::EditorFocused(false));
            }
            Binding::Copy | Binding::Cut => {
                let selected = self.display_selection().range();
                let has_object = self.projection.rich.images.iter().any(|image| {
                    selected.start <= image.range.start && selected.end >= image.range.end
                }) || self.projection.rich.tables.iter().any(|table| {
                    selected.start <= table.range.start && selected.end >= table.range.end
                }) || self.projection.rich.code_blocks.iter().any(|block| {
                    selected.start <= block.range.start && selected.end >= block.range.end
                }) || self.projection.footnotes().references.iter().any(
                    |reference| {
                        selected.start <= self.projection.to_display(reference.range.start)
                            && selected.end >= self.projection.to_display(reference.range.end)
                    },
                );
                let copied = if has_object && !selected.is_empty() {
                    let mut range = if selected == (0..self.projection.text.len()) {
                        0..self.source.len()
                    } else {
                        self.projection.source_range(selected.clone())
                    };
                    for table in &self.projection.rich.tables {
                        if selected.start <= table.range.start && selected.end >= table.range.end {
                            range.start = range.start.min(table.source_range.start);
                            range.end = range.end.max(table.source_range.end);
                        }
                    }
                    for block in &self.projection.rich.code_blocks {
                        if selected.start <= block.range.start && selected.end >= block.range.end {
                            range.start = range.start.min(block.source_range.start);
                            range.end = range.end.max(block.source_range.end);
                        }
                    }
                    Some(self.source[range].to_owned())
                } else {
                    self.content.selection()
                };
                if let Some(text) = copied {
                    clipboard.write(iced::advanced::clipboard::Kind::Standard, text);
                    if matches!(binding, Binding::Cut) {
                        shell.publish(Message::Edit(Action::Edit(Edit::Delete)));
                    }
                }
            }
            Binding::Paste => {
                shell.publish(Message::Writing(WritingMessage::Paste(
                    clipboard.read(iced::advanced::clipboard::Kind::Standard),
                )));
            }
            Binding::Move(motion) | Binding::Select(motion) => {
                let selecting = matches!(binding, Binding::Select(_));
                if !state.rich.contains_source(self.source_selection.head) {
                    state.scroll = state.rich.show_source(
                        self.layout_input(bounds.width),
                        self.source_selection.head,
                        bounds.height,
                    );
                }
                let caret = state.caret(self.source_selection.head);
                let target = match motion {
                    Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown => {
                        let x = *state.preferred_x.get_or_insert(caret.x);
                        let direction = if matches!(motion, Motion::Up | Motion::PageUp) {
                            -1.0
                        } else {
                            1.0
                        };
                        let page = matches!(motion, Motion::PageUp | Motion::PageDown)
                            .then_some(bounds.height * 0.85);
                        let (head, y) = state
                            .rich
                            .resident()
                            .vertical_source(caret, x, direction, page)
                            .unwrap_or((self.source_selection.head, caret.y));
                        if state.modifiers.command() {
                            shell.publish(Message::Writing(WritingMessage::OpenAt(head)));
                            shell.capture_event();
                            return;
                        }
                        if !selecting
                            && let Some((table, row, column)) = state
                                .rich
                                .resident()
                                .cell_at(Point::new(x, y + caret.height / 2.0))
                        {
                            shell.publish(Message::TableGrid(TableGridMessage::Select {
                                revision: Arc::clone(&self.projection.source_id),
                                table,
                                row,
                                column,
                                extend: false,
                                extent: TableExtent::Cell,
                            }));
                            return;
                        }
                        state.set_visual_cursor(head, y);
                        Some(head)
                    }
                    Motion::Home | Motion::End => {
                        state.preferred_x = None;
                        let head = state.rich.resident().source_line_edge(
                            self.source_selection.head,
                            caret,
                            motion == Motion::End,
                        );
                        state.set_visual_cursor(head, caret.y);
                        Some(head)
                    }
                    _ => {
                        state.preferred_x = None;
                        state.visual_cursor = None;
                        None
                    }
                };
                if let Some(head) = target {
                    let head = self.projection.normalize_caret(Selection::caret(head)).head;
                    shell.publish(Message::SelectSource(Selection {
                        anchor: if selecting {
                            self.source_selection.anchor
                        } else {
                            head
                        },
                        head,
                    }));
                } else {
                    shell.publish(Message::Edit(if selecting {
                        Action::Select(motion)
                    } else {
                        Action::Move(motion)
                    }));
                }
            }
            Binding::SelectWord => shell.publish(Message::SelectSource(
                self.raw_selection(self.word_selection(self.display_selection().head)),
            )),
            Binding::SelectLine => shell.publish(Message::SelectSource(
                self.raw_selection(self.line_selection(self.display_selection().head)),
            )),
            Binding::SelectAll => shell.publish(Message::Edit(Action::SelectAll)),
            Binding::Insert(c) => shell.publish(Message::Edit(Action::Edit(Edit::Insert(c)))),
            Binding::Enter => shell.publish(Message::Edit(Action::Edit(Edit::Enter))),
            Binding::Backspace => shell.publish(Message::Edit(Action::Edit(Edit::Backspace))),
            Binding::Delete => shell.publish(Message::Edit(Action::Edit(Edit::Delete))),
            Binding::Sequence(sequence) => {
                for binding in sequence {
                    self.apply_binding(binding, state, clipboard, shell, bounds);
                }
            }
            Binding::Custom(Message::Writing(WritingMessage::Paste(None))) => {
                shell.publish(Message::Writing(WritingMessage::Paste(
                    clipboard.read(iced::advanced::clipboard::Kind::Standard),
                )))
            }
            Binding::Custom(message) => shell.publish(message),
        }
    }

    fn keyboard_grid(&self) -> Option<TableGridView> {
        self.grid.or(self.grid_resume).or_else(|| {
            if !self.source_selection.is_caret() {
                return None;
            }
            let (table, row, column) =
                crate::document::table_edit::cell_at(self.projection, self.source_selection)?;
            let table = self
                .projection
                .rich
                .tables
                .iter()
                .position(|t| std::ptr::eq(t, table))?;
            Some(TableGridView {
                table,
                anchor: (row, column),
                head: (row, column),
                active: (row, column),
                editing: false,
            })
        })
    }

    fn activate_grid(&self, grid: TableGridView, shell: &mut Shell<'_, Message>) {
        shell.publish(Message::TableGrid(TableGridMessage::Select {
            revision: Arc::clone(&self.projection.source_id),
            table: grid.table,
            row: grid.active.0,
            column: grid.active.1,
            extend: false,
            extent: TableExtent::Cell,
        }));
    }

    fn leave_table_vertically(
        &self,
        state: &mut State,
        bounds: Rectangle,
        up: bool,
        shell: &mut Shell<'_, Message>,
    ) -> bool {
        let Some(grid) = self.keyboard_grid() else {
            return false;
        };
        let table = &self.projection.rich.tables[grid.table];
        if grid.active.0 != if up { 0 } else { table.rows.len() - 1 } {
            return false;
        }
        // Prefer an existing outside destination. At the document edge, ask the
        // App for a revision-checked paragraph insertion instead of leaving a
        // caret that shares the last cell's source position.
        let visible_outside = |index| match crate::document::table_edit::cell_at(
            self.projection,
            Selection::caret(index),
        ) {
            Some((candidate, _, _)) => !std::ptr::eq(candidate, table),
            None => self.projection.text[self.projection.to_display(index)..]
                .chars()
                .next()
                .is_some_and(|character| !character.is_whitespace()),
        };
        let outside = if up {
            let prefix = &self.source[..table.source_range.start];
            prefix
                .char_indices()
                .rev()
                .find(|(index, c)| {
                    !c.is_whitespace()
                        && !self.projection.is_quote_marker(*index)
                        && visible_outside(*index)
                })
                .map(|(index, _)| index)
                .or_else(|| {
                    if !prefix.chars().all(char::is_whitespace) {
                        return None;
                    }
                    // Two existing line endings leave a blank separator even
                    // after typing at the beginning of an otherwise empty page.
                    let breaks = prefix
                        .bytes()
                        .enumerate()
                        .filter(|&(i, byte)| {
                            byte == b'\r'
                                || byte == b'\n' && (i == 0 || prefix.as_bytes()[i - 1] != b'\r')
                        })
                        .take(2)
                        .count();
                    (breaks == 2).then_some(0)
                })
        } else {
            let suffix = &self.source[table.source_range.end..];
            suffix
                .char_indices()
                .find(|(index, c)| {
                    !c.is_whitespace()
                        && !self
                            .projection
                            .is_quote_marker(table.source_range.end + index)
                        && visible_outside(table.source_range.end + index)
                })
                .map(|(index, _)| table.source_range.end + index)
                .or_else(|| {
                    (suffix.chars().all(char::is_whitespace) && suffix.contains(['\r', '\n']))
                        .then_some(self.source.len())
                })
        };
        let Some(outside) = outside else {
            shell.publish(Message::TableGrid(TableGridMessage::ExitBoundary {
                revision: Arc::clone(&self.projection.source_id),
                table: grid.table,
                row: grid.active.0,
                column: grid.active.1,
                up,
            }));
            state.preferred_x = None;
            state.visual_cursor = None;
            return true;
        };
        let blank_paragraph = if up {
            self.source[..table.source_range.start]
                .chars()
                .all(char::is_whitespace)
        } else {
            self.source[table.source_range.end..]
                .chars()
                .all(char::is_whitespace)
        };
        if state
            .rich
            .resident()
            .cell_bounds(grid.table, grid.active.0, grid.active.1)
            .is_none()
        {
            state.scroll = state.rich.show_source(
                self.layout_input(bounds.width),
                table.source_rows[grid.active.0][grid.active.1].start,
                bounds.height,
            );
        }
        let Some(cell) =
            state
                .rich
                .resident()
                .cell_bounds(grid.table, grid.active.0, grid.active.1)
        else {
            return false;
        };
        let padding = (self.theme.typography.body_size * self.zoom * 0.65).min(cell.width / 4.0);
        let x = state
            .preferred_x
            .unwrap_or(cell.x + padding)
            .clamp(cell.x + padding, cell.x + cell.width - padding);
        if !state.rich.contains_source(outside) {
            state.scroll =
                state
                    .rich
                    .show_source(self.layout_input(bounds.width), outside, bounds.height);
        }
        if let Some((target, _, _)) =
            crate::document::table_edit::cell_at(self.projection, Selection::caret(outside))
        {
            let Some(index) = self
                .projection
                .rich
                .tables
                .iter()
                .position(|candidate| std::ptr::eq(candidate, target))
            else {
                return false;
            };
            if index == grid.table {
                return false;
            }
            let row = if up { target.rows.len() - 1 } else { 0 };
            if state.rich.resident().cell_bounds(index, row, 0).is_none() {
                state.scroll = state.rich.show_source(
                    self.layout_input(bounds.width),
                    target.source_rows[row][0].start,
                    bounds.height,
                );
            }
            let Some(target_cell) = state.rich.resident().cell_bounds(index, row, 0) else {
                return false;
            };
            let Some((_, _, column)) = state
                .rich
                .resident()
                .cell_at(Point::new(x, target_cell.center_y()))
            else {
                return false;
            };
            shell.publish(Message::TableGrid(TableGridMessage::Select {
                revision: Arc::clone(&self.projection.source_id),
                table: index,
                row,
                column,
                extend: false,
                extent: TableExtent::Cell,
            }));
            state.visual_cursor = None;
        } else {
            let Some((head, y)) = state.rich.resident().source_row_target(outside, x) else {
                return false;
            };
            let head = if blank_paragraph {
                outside
            } else {
                self.projection.normalize_caret(Selection::caret(head)).head
            };
            if crate::document::table_edit::cell_at(self.projection, Selection::caret(head))
                .is_some()
            {
                return false;
            }
            state.set_visual_cursor(head, y);
            shell.publish(Message::SelectSource(Selection::caret(head)));
        }
        state.preferred_x = Some(x);
        true
    }

    fn grid_key(
        &self,
        key: &keyboard::Key,
        modifiers: &keyboard::Modifiers,
        text: Option<&str>,
        shell: &mut Shell<'_, Message>,
    ) -> bool {
        use keyboard::key::Named;
        let grid = self.keyboard_grid();
        let adopt = |shell: &mut Shell<'_, Message>| {
            if self.grid.is_none()
                && let Some(grid) = grid
            {
                self.activate_grid(grid, shell);
            }
        };
        let message = match key.as_ref() {
            keyboard::Key::Named(
                Named::ArrowUp | Named::ArrowDown | Named::ArrowLeft | Named::ArrowRight,
            ) if !modifiers.alt() => {
                let (rows, columns) = match key {
                    keyboard::Key::Named(Named::ArrowUp) => (-1, 0),
                    keyboard::Key::Named(Named::ArrowDown) => (1, 0),
                    keyboard::Key::Named(Named::ArrowLeft) => (0, -1),
                    _ => (0, 1),
                };
                if modifiers.command() || modifiers.control() {
                    Some(TableGridMessage::Edge {
                        rows,
                        columns,
                        extend: modifiers.shift(),
                    })
                } else {
                    Some(TableGridMessage::Move {
                        rows,
                        columns,
                        extend: modifiers.shift(),
                    })
                }
            }
            keyboard::Key::Named(Named::Enter) if modifiers.is_empty() => {
                Some(TableGridMessage::Begin(None))
            }
            keyboard::Key::Named(Named::Enter) if modifiers.command() => {
                Some(TableGridMessage::Commit(None))
            }
            keyboard::Key::Named(Named::Enter) if *modifiers == keyboard::Modifiers::SHIFT => {
                Some(TableGridMessage::Commit(Some((-1, 0))))
            }
            keyboard::Key::Named(Named::Tab)
                if modifiers.is_empty() || *modifiers == keyboard::Modifiers::SHIFT =>
            {
                Some(TableGridMessage::Tab(modifiers.shift()))
            }
            keyboard::Key::Named(Named::Delete | Named::Backspace) if modifiers.is_empty() => {
                Some(TableGridMessage::Clear)
            }
            keyboard::Key::Named(Named::Escape) if self.grid.is_some() => {
                Some(TableGridMessage::Cancel)
            }
            keyboard::Key::Named(Named::Home | Named::End) => Some(TableGridMessage::Boundary {
                end: matches!(key, keyboard::Key::Named(Named::End)),
                whole: modifiers.command() || modifiers.control(),
                extend: modifiers.shift(),
            }),
            keyboard::Key::Named(Named::Space) if *modifiers == keyboard::Modifiers::SHIFT => {
                Some(TableGridMessage::SelectExtent(TableExtent::Row))
            }
            keyboard::Key::Named(Named::Space) if *modifiers == keyboard::Modifiers::CTRL => {
                Some(TableGridMessage::SelectExtent(TableExtent::Column))
            }
            keyboard::Key::Character(character)
                if modifiers.command() && character.eq_ignore_ascii_case("a") =>
            {
                if let Some(grid) = grid {
                    let table = &self.projection.rich.tables[grid.table];
                    if grid.contains(0, 0)
                        && grid.contains(table.rows.len() - 1, table.alignment.len() - 1)
                    {
                        shell.publish(Message::Edit(Action::SelectAll));
                        return true;
                    }
                }
                Some(TableGridMessage::SelectExtent(TableExtent::Table))
            }
            keyboard::Key::Character(character)
                if modifiers.command()
                    && (character.eq_ignore_ascii_case("c")
                        || character.eq_ignore_ascii_case("x")) =>
            {
                adopt(shell);
                // App messages from earlier keys in this event batch must
                // update selection before the copied rectangle is captured.
                shell.publish(Message::TableGrid(TableGridMessage::CopySelection {
                    cut: character.eq_ignore_ascii_case("x"),
                }));
                return true;
            }
            keyboard::Key::Character(character)
                if modifiers.command() && character.eq_ignore_ascii_case("v") =>
            {
                adopt(shell);
                shell.publish(Message::TableGrid(TableGridMessage::PasteSelection));
                return true;
            }
            _ if !modifiers.command() && !modifiers.control() => text
                .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
                .map(|s| TableGridMessage::Begin(Some(s.to_owned()))),
            _ => None,
        };
        if let Some(message) = message {
            adopt(shell);
            shell.publish(Message::TableGrid(message));
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
struct State {
    rich: VirtualLayout,
    layout_key: Option<LayoutKey>,
    focused: bool,
    window_focused: bool,
    preedit: Option<input_method::Preedit>,
    last_click: Option<mouse::Click>,
    drag_anchor: Option<usize>,
    stable_drag: Option<StablePosition>,
    pending_table_drag: Option<Arc<()>>,
    grid_drag: Option<(usize, TableExtent, f32)>,
    grid_drag_revision: Option<Arc<()>>,
    hovered_handle: Option<(usize, usize, usize, TableExtent)>,
    hovered_menu: bool,
    stable_viewport: Option<(StablePosition, f32)>,
    stable_wrap: Option<(StablePosition, bool)>,
    composition_target: Option<CompositionTarget>,
    causal_generation: Option<(uuid::Uuid, u64, u64)>,
    drag_position: Option<Point>,
    modifiers: keyboard::Modifiers,
    preferred_x: Option<f32>,
    visual_cursor: Option<(usize, bool)>,
    scroll: f32,
    scroll_drag: Option<f32>,
    viewport_height: f32,
    last_selection: Option<Selection>,
    blink_started: Option<Instant>,
    now: Option<Instant>,
    copied_code: Option<(usize, Instant)>,
    pending_task: Option<TaskPress>,
    layout_epoch: u64,
}

struct TaskPress {
    marker: Range<usize>,
    revision: Arc<()>,
    position: Point,
    scroll: f32,
    layout_epoch: u64,
    cancelled: bool,
}

struct LayoutKey {
    lines: Arc<HighlightLines>,
    source: Arc<()>,
    tokens: Tokens,
    typography: Typography,
    width: f32,
    zoom: f32,
    images: u64,
    base: Option<PathBuf>,
}

impl State {
    fn table_menu_available(&self) -> bool {
        self.modifiers.is_empty()
            && self
                .preedit
                .as_ref()
                .is_none_or(|preedit| preedit.content.is_empty())
            && self.grid_drag.is_none()
            && self.drag_anchor.is_none()
            && self.scroll_drag.is_none()
    }
    fn caret(&self, source: usize) -> Rectangle {
        if let Some((_, upstream)) = self.visual_cursor.filter(|(head, _)| *head == source) {
            self.rich.source_caret_on_side(source, upstream)
        } else {
            self.rich.source_caret(source, None)
        }
    }
    fn set_visual_cursor(&mut self, source: usize, y: f32) {
        self.visual_cursor = Some((source, self.rich.resident().upstream_affinity(source, y)));
    }
    fn reset_blink(&mut self) {
        let now = Instant::now();
        self.blink_started = Some(now);
        self.now = Some(now);
    }
    fn caret_visible(&self) -> bool {
        self.focused
            && self.window_focused
            && self.blink_started.zip(self.now).is_none_or(|(start, now)| {
                (now.saturating_duration_since(start).as_millis() / 500).is_multiple_of(2)
            })
    }
    fn unfocus(&mut self) {
        self.focused = false;
        self.drag_anchor = None;
        self.stable_drag = None;
        self.preedit = None;
        self.composition_target = None;
        self.pending_task = None;
        self.grid_drag = None;
        self.grid_drag_revision = None;
    }
    fn clamp_scroll(&mut self) {
        self.scroll = self
            .scroll
            .clamp(0.0, (self.rich.height() - self.viewport_height).max(0.0));
    }
    fn scrollbar(&self) -> (f32, f32) {
        let height = (self.viewport_height * self.viewport_height / self.rich.height().max(1.0))
            .max(24.0)
            .min(self.viewport_height);
        let y = self.scroll / (self.rich.height() - self.viewport_height).max(1.0)
            * (self.viewport_height - height);
        (y, height)
    }

    fn drag_scrollbar(&mut self, y: f32, anchor: f32) {
        let (_, height) = self.scrollbar();
        self.scroll = (y - anchor) / (self.viewport_height - height).max(1.0)
            * (self.rich.height() - self.viewport_height);
        self.clamp_scroll();
    }
    fn drag(&mut self, projection: &Projection, shell: &mut Shell<'_, Message>) {
        if let (Some((table, extent, zoom)), Some(position)) = (self.grid_drag, self.drag_position)
        {
            let point = Point::new(
                position.x,
                position.y.clamp(0.0, self.viewport_height) + self.scroll,
            );
            let target = table_hit(&self.rich, point, zoom, Some(table))
                .filter(|hit| hit.0 == table)
                .or_else(|| {
                    self.rich
                        .resident()
                        .cells()
                        .filter(|cell| cell.0 == table)
                        .min_by(|a, b| {
                            let distance = |r: Rectangle| {
                                (r.center_x() - point.x).abs() + (r.center_y() - point.y).abs()
                            };
                            distance(a.3).total_cmp(&distance(b.3))
                        })
                        .map(|(t, r, c, _)| (t, r, c, extent))
                });
            if let Some((table, row, column, _)) = target {
                shell.publish(Message::TableGrid(TableGridMessage::Select {
                    revision: Arc::clone(&projection.source_id),
                    table,
                    row,
                    column,
                    extend: true,
                    extent,
                }));
            }
            return;
        }
        if let (Some(anchor), Some(position)) = (self.drag_anchor, self.drag_position) {
            let point = Point::new(
                position.x,
                position.y.clamp(0.0, self.viewport_height) + self.scroll,
            );
            let (head, y) = self.rich.resident().hit_source_with_row(point);
            self.set_visual_cursor(head, y);
            let head = projection.normalize_caret(Selection::caret(head)).head;
            shell.publish(Message::SelectSource(Selection { anchor, head }));
        }
    }
}

impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
        self.window_focused = true;
        self.last_selection = None;
        self.reset_blink();
    }
    fn unfocus(&mut self) {
        self.unfocus();
    }
}

impl Widget<Message, Theme, Renderer> for RichEditor<'_> {
    fn children(&self) -> Vec<widget::Tree> {
        self.cell_editor
            .iter()
            .map(|child| widget::Tree::new(child.as_widget()))
            .collect()
    }
    fn diff(&self, tree: &mut widget::Tree) {
        if let Some(child) = &self.cell_editor {
            tree.diff_children(std::slice::from_ref(child));
        } else {
            tree.children.clear();
        }
    }
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        let bounds = limits.width(Length::Fill).height(Length::Fill).max();
        let causal = self
            .collaboration
            .map(|document| (document.id(), document.replica_id(), document.generation()));
        let causal_changed = state.causal_generation != causal;
        if causal_changed && let Some(document) = self.collaboration {
            if state
                .causal_generation
                .is_some_and(|(id, owner, _)| id != document.id() || owner != document.replica_id())
            {
                state.stable_drag = None;
                state.stable_viewport = None;
                state.stable_wrap = None;
                state.composition_target = None;
                state.preedit = None;
            }
            if let Some(anchor) = state
                .stable_drag
                .as_ref()
                .and_then(|anchor| document.resolve(anchor))
            {
                state.drag_anchor = Some(anchor);
            }
            if let Some((position, upstream)) = &state.stable_wrap {
                state.visual_cursor = document.resolve(position).map(|offset| (offset, *upstream));
            }
        }
        let source_changed = state
            .layout_key
            .as_ref()
            .is_some_and(|key| !Arc::ptr_eq(&key.source, &self.projection.source_id));
        if state
            .grid_drag_revision
            .as_ref()
            .is_some_and(|revision| !Arc::ptr_eq(revision, &self.projection.source_id))
        {
            state.grid_drag = None;
            state.grid_drag_revision = None;
            state.drag_position = None;
        }
        let content_changed = state.layout_key.as_ref().is_none_or(|key| {
            !Arc::ptr_eq(&key.source, &self.projection.source_id)
                || key.tokens != self.theme.tokens
                || key.typography != self.theme.typography
                || key.width != bounds.width
                || key.zoom != self.zoom
                || key.images != self.images.revision()
                || key.base.as_deref() != self.base
        });
        let changed = content_changed
            || state
                .layout_key
                .as_ref()
                .is_none_or(|key| !Arc::ptr_eq(&key.lines, &self.projection.highlights));
        let input = self.layout_input(bounds.width);
        let selection_changed = state.last_selection != Some(self.source_selection)
            && !(self.preserve_viewport && causal_changed);
        let previous_caret = state.caret(if self.preserve_viewport && causal_changed {
            state.last_selection.unwrap_or(self.source_selection).head
        } else {
            self.source_selection.head
        });
        let was_visible = previous_caret.y >= state.scroll
            && previous_caret.y + previous_caret.height <= state.scroll + state.viewport_height;
        let disclosure_only = changed && !content_changed;
        let mut viewport_anchor = changed
            .then(|| {
                if was_visible {
                    Some((self.source_selection.head, previous_caret.y))
                } else {
                    state.rich.resident().viewport_anchor(state.scroll)
                }
            })
            .flatten();
        if causal_changed
            && changed
            && let Some(document) = self.collaboration
            && let Some((position, screen_y)) = &state.stable_viewport
            && let Some(offset) = document.resolve(position)
        {
            viewport_anchor = Some((offset, state.scroll + *screen_y));
        }
        let affinity = state.visual_cursor;
        if changed {
            state.layout_epoch = state.layout_epoch.wrapping_add(1);
            state.visual_cursor = None;
            state.rich.rebuild(input);
            state.layout_key = Some(LayoutKey {
                lines: Arc::clone(&self.projection.highlights),
                source: Arc::clone(&self.projection.source_id),
                tokens: self.theme.tokens,
                typography: self.theme.typography,
                width: bounds.width,
                zoom: self.zoom,
                images: self.images.revision(),
                base: self.base.map(Path::to_path_buf),
            });
        }
        state.viewport_height = bounds.height;
        // A source jump measures its destination, never the intervening blocks.
        let jump = (selection_changed || source_changed)
            && !was_visible
            && !(causal_changed && state.stable_viewport.is_some());
        let anchored_screen_y = viewport_anchor.map(|(_, y)| y - state.scroll);
        state.scroll = if jump {
            state
                .rich
                .show_source(input, self.source_selection.head, bounds.height)
        } else if let Some((source, _)) = viewport_anchor {
            state.rich.show_source(input, source, bounds.height)
        } else {
            state.rich.show_viewport(input, state.scroll, bounds.height)
        };
        if changed {
            state.visual_cursor = affinity;
            if !jump && let Some((source, _)) = viewport_anchor {
                state.scroll =
                    state.caret(source).y - anchored_screen_y.expect("anchor has screen position");
            }
        }
        if selection_changed && !(disclosure_only && was_visible)
            || source_changed && (!self.preserve_viewport || was_visible)
            || content_changed && state.focused && was_visible
        {
            let caret = state.caret(self.source_selection.head);
            if caret.y < state.scroll + 8.0 {
                state.scroll = (caret.y - 8.0).max(0.0);
            }
            if caret.y + caret.height > state.scroll + bounds.height - 12.0 {
                state.scroll = caret.y + caret.height - bounds.height + 12.0;
            }
        }
        self.refresh_viewport(state, bounds);
        state.last_selection = Some(self.source_selection);
        state.clamp_scroll();
        state.causal_generation = causal;
        // Wrap affinity belongs to this caret. A programmatic jump must not
        // retain (or repeatedly anchor) an unrelated old visual position.
        if state
            .visual_cursor
            .is_some_and(|(head, _)| head != self.source_selection.head)
        {
            state.visual_cursor = None;
        }
        if let Some(document) = self.collaboration {
            let caret = state.caret(self.source_selection.head);
            let anchor = if caret.y >= state.scroll
                && caret.y + caret.height <= state.scroll + bounds.height
            {
                Some((self.source_selection.head, caret.y))
            } else {
                state.rich.resident().viewport_anchor(state.scroll)
            };
            state.stable_viewport = anchor
                .map(|(offset, y)| (document.position(offset, Assoc::After), y - state.scroll));
            state.stable_wrap = state
                .visual_cursor
                .map(|(offset, side)| (document.position(offset, Assoc::After), side));
        }
        if let Some(child) = &mut self.cell_editor {
            // A concurrently removed table must not hide a retained draft. Its
            // checked target will reject commit, while copy and Escape remain
            // available in a temporary editor at the top of the viewport.
            let rect = self
                .grid
                .and_then(|grid| {
                    state
                        .rich
                        .resident()
                        .cell_bounds(grid.table, grid.active.0, grid.active.1)
                })
                .unwrap_or(Rectangle::new(
                    Point::new(0.0, state.scroll + 8.0),
                    Size::new((bounds.width - 12.0).max(1.0), 64.0),
                ));
            let padding = (self.theme.typography.body_size * self.zoom * 0.65).min(12.0) * 2.0;
            let mut node = child.as_widget_mut().layout(
                &mut tree.children[0],
                renderer,
                &layout::Limits::new(
                    Size::new(rect.width, (rect.height - padding).max(0.0)),
                    Size::new(rect.width, bounds.height.max(rect.height)),
                ),
            );
            if node.size().height > rect.height + 0.5 {
                let bottom = rect.y + node.size().height;
                let height = self
                    .grid
                    .and_then(|grid| {
                        state
                            .rich
                            .resident()
                            .cells()
                            .filter(|(table, _, column, r)| {
                                *table == grid.table
                                    && *column == grid.active.1
                                    && r.y + r.height >= bottom
                            })
                            .map(|(_, _, _, r)| r.y + r.height - rect.y)
                            .min_by(f32::total_cmp)
                    })
                    .unwrap_or(node.size().height);
                node = child.as_widget_mut().layout(
                    &mut tree.children[0],
                    renderer,
                    &layout::Limits::new(
                        Size::new(rect.width, (height - padding).max(0.0)),
                        Size::new(rect.width, bounds.height.max(height)),
                    ),
                );
            }
            node.move_to_mut(Point::new(rect.x, rect.y - state.scroll));
            layout::Node::with_children(bounds, vec![node])
        } else {
            layout::Node::new(bounds)
        }
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.focusable(
            Some(&widget::Id::from("document-editor")),
            layout.bounds(),
            tree.state.downcast_mut::<State>(),
        );
        operation.custom(None, layout.bounds(), tree.state.downcast_mut::<State>());
        if let (Some(child), Some(child_layout)) = (&mut self.cell_editor, layout.children().next())
        {
            child.as_widget_mut().operate(
                &mut tree.children[0],
                child_layout,
                _renderer,
                operation,
            );
        }
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let previous_scroll = state.scroll;
        let bounds = layout.bounds();
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            let available = state.table_menu_available();
            state.modifiers = *modifiers;
            if available != state.table_menu_available() && state.hovered_handle.is_some() {
                shell.request_redraw();
            }
        }
        if matches!(
            event,
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
        ) {
            let hovered = if matches!(event, Event::Mouse(mouse::Event::CursorLeft)) {
                None
            } else {
                cursor
                    .position()
                    .map(|point| point - (bounds.position() - Point::ORIGIN))
                    .filter(|point| point.y >= 0.0 && point.y <= bounds.height)
                    .and_then(|point| {
                        table_hit(
                            &state.rich,
                            point + Vector::new(0.0, state.scroll),
                            self.zoom,
                            self.grid.map(|grid| grid.table),
                        )
                    })
                    .filter(|hit| matches!(hit.3, TableExtent::Row | TableExtent::Column))
            };
            let menu_hovered = hovered.is_some()
                && cursor
                    .position()
                    .map(|point| point - (bounds.position() - Point::ORIGIN))
                    .is_some_and(|point| {
                        table_menu_hit(
                            &state.rich,
                            point + Vector::new(0.0, state.scroll),
                            self.zoom,
                            self.grid.map(|grid| grid.table),
                        )
                        .is_some()
                    });
            if state.hovered_handle != hovered || state.hovered_menu != menu_hovered {
                state.hovered_handle = hovered;
                state.hovered_menu = menu_hovered;
                shell.request_redraw();
            }
        }
        if let (Some(child), Some(child_layout)) = (&mut self.cell_editor, layout.children().next())
        {
            let outside_click = matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Left | mouse::Button::Right
                ))
            ) && (!cursor.is_over(child_layout.bounds())
                || state.table_menu_available()
                    && cursor
                        .position()
                        .map(|point| point - (bounds.position() - Point::ORIGIN))
                        .filter(|point| point.y >= 0.0 && point.y <= bounds.height)
                        .is_some_and(|point| {
                            table_menu_hit(
                                &state.rich,
                                point + Vector::new(0.0, state.scroll),
                                self.zoom,
                                self.grid.map(|grid| grid.table),
                            )
                            .is_some()
                        })
                || matches!(
                    event,
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                ));
            let wheel = matches!(event, Event::Mouse(mouse::Event::WheelScrolled { .. }));
            if !outside_click && !wheel {
                if matches!(event, Event::Keyboard(keyboard::Event::KeyPressed { .. }))
                    && state
                        .preedit
                        .as_ref()
                        .is_some_and(|p| !p.content.is_empty())
                {
                    shell.capture_event();
                    return;
                }
                if let Event::InputMethod(event) = event {
                    match event {
                        input_method::Event::Preedit(content, selection) => {
                            state.preedit = Some(input_method::Preedit {
                                content: content.clone(),
                                selection: selection.clone(),
                                text_size: None,
                            })
                        }
                        input_method::Event::Closed | input_method::Event::Commit(_) => {
                            state.preedit = None
                        }
                        _ => {}
                    }
                }
                child.as_widget_mut().update(
                    &mut tree.children[0],
                    event,
                    child_layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    _viewport,
                );
                return;
            }
        }
        if state
            .pending_table_drag
            .as_ref()
            .is_some_and(|revision| !Arc::ptr_eq(revision, &self.projection.source_id))
        {
            state.pending_table_drag = None;
            state.drag_anchor = Some(self.source_selection.anchor);
            state.stable_drag = self
                .collaboration
                .map(|document| document.position(self.source_selection.anchor, Assoc::After));
        }
        if matches!(event, Event::Window(window::Event::RedrawRequested(_)))
            && self.cell_editor.is_none()
            && state.focused != self.app_focused
        {
            shell.publish(Message::EditorFocused(state.focused));
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if state
                    .preedit
                    .as_ref()
                    .is_none_or(|preedit| preedit.content.is_empty()) =>
            {
                if let Some(position) = cursor
                    .position()
                    .map(|point| point - (bounds.position() - Point::ORIGIN))
                    .filter(|point| point.y >= 0.0 && point.y <= bounds.height)
                    && let Some((table, row, column, extent)) = table_hit(
                        &state.rich,
                        position + Vector::new(0.0, state.scroll),
                        self.zoom,
                        self.grid.map(|grid| grid.table),
                    )
                {
                    shell.publish(Message::Table(TableMessage::OpenContext {
                        revision: Arc::clone(&self.projection.source_id),
                        table,
                        row,
                        column,
                        position: position + (bounds.position() - Point::ORIGIN),
                        extent,
                    }));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor
                    .position()
                    .map(|point| point - (bounds.position() - Point::ORIGIN))
                    .filter(|point| point.y >= 0.0 && point.y <= bounds.height);
                if let Some(position) = position.filter(|point| {
                    Rectangle::with_size(bounds.size()).contains(*point)
                        || table_hit(
                            &state.rich,
                            *point + Vector::new(0.0, state.scroll),
                            self.zoom,
                            self.grid.map(|grid| grid.table),
                        )
                        .is_some()
                }) {
                    let point = position + Vector::new(0.0, state.scroll);
                    if state.table_menu_available()
                        && let Some((table, row, column, extent, menu)) = table_menu_hit(
                            &state.rich,
                            point,
                            self.zoom,
                            self.grid.map(|grid| grid.table),
                        )
                    {
                        state.grid_drag = None;
                        state.grid_drag_revision = None;
                        shell.publish(Message::Table(TableMessage::OpenContext {
                            revision: Arc::clone(&self.projection.source_id),
                            table,
                            row,
                            column,
                            extent,
                            position: Point::new(menu.x + menu.width, menu.y + menu.height)
                                + (bounds.position() - Point::ORIGIN)
                                - Vector::new(0.0, state.scroll),
                        }));
                        shell.capture_event();
                        return;
                    }
                    if state.modifiers.command()
                        && !state.modifiers.shift()
                        && let Some(reference) = {
                            let hit = state.rich.resident().hit_source_with_row(point).0;
                            self.projection
                                .footnotes()
                                .reference_at(hit)
                                .into_iter()
                                .chain(self.projection.footnotes().reference_before(hit))
                                .find(|reference| {
                                    state
                                        .rich
                                        .resident()
                                        .source_selection_rects_in(
                                            Selection {
                                                anchor: reference.range.start,
                                                head: reference.range.end,
                                            },
                                            point.y..point.y,
                                        )
                                        .iter()
                                        .any(|bounds| bounds.contains(point))
                                })
                        }
                    {
                        shell.publish(Message::Footnote(FootnoteMessage::OpenAt(
                            reference.range.start,
                        )));
                        shell.capture_event();
                        return;
                    }
                    if let Some(block) =
                        code_copy_at(state, position + Vector::new(0.0, state.scroll), self.zoom)
                    {
                        if let Some(block) = self.projection.rich.code_blocks.get(block) {
                            clipboard.write(
                                iced::advanced::clipboard::Kind::Standard,
                                block.text.as_ref().clone(),
                            );
                            state.copied_code = Some((block.source_range.start, Instant::now()));
                            shell.request_redraw();
                            shell.request_redraw_at(Instant::now() + Duration::from_secs(2));
                        }
                        shell.capture_event();
                        return;
                    }
                    if let Some(marker) = task_at(
                        state,
                        self.projection,
                        position + Vector::new(0.0, state.scroll),
                    ) {
                        let click =
                            mouse::Click::new(position, mouse::Button::Left, state.last_click);
                        state.pending_task =
                            (click.kind() == mouse::click::Kind::Single).then(|| TaskPress {
                                marker,
                                revision: Arc::clone(&self.projection.source_id),
                                position,
                                scroll: state.scroll,
                                layout_epoch: state.layout_epoch,
                                cancelled: false,
                            });
                        state.last_click = Some(click);
                        shell.capture_event();
                        return;
                    }
                    if state.preedit.as_ref().is_none_or(|p| p.content.is_empty()) {
                        state.composition_target = None;
                    }
                    if position.x >= bounds.width - 10.0 && state.rich.height() > bounds.height {
                        let (y, height) = state.scrollbar();
                        let anchor = if y <= position.y && position.y <= y + height {
                            position.y - y
                        } else {
                            height / 2.0
                        };
                        state.scroll_drag = Some(anchor);
                        state.drag_scrollbar(position.y, anchor);
                        self.refresh_viewport(state, bounds.size());
                        shell.capture_event();
                        shell.request_redraw();
                        return;
                    }
                    let click = mouse::Click::new(position, mouse::Button::Left, state.last_click);
                    if let Some((table, row, column, extent)) = table_hit(
                        &state.rich,
                        point,
                        self.zoom,
                        self.grid.map(|grid| grid.table),
                    ) && !state.modifiers.command()
                    {
                        shell.publish(Message::TableGrid(TableGridMessage::Select {
                            revision: Arc::clone(&self.projection.source_id),
                            table,
                            row,
                            column,
                            extend: state.modifiers.shift(),
                            extent,
                        }));
                        if click.kind() == mouse::click::Kind::Double && extent == TableExtent::Cell
                        {
                            shell.publish(Message::TableGrid(TableGridMessage::Begin(None)));
                        }
                        state.focused = true;
                        state.window_focused = true;
                        state.last_click = Some(click);
                        state.grid_drag = (click.kind() == mouse::click::Kind::Single)
                            .then_some((table, extent, self.zoom));
                        state.grid_drag_revision = state
                            .grid_drag
                            .map(|_| Arc::clone(&self.projection.source_id));
                        state.drag_anchor = None;
                        state.stable_drag = None;
                        state.drag_position = Some(position);
                        state.preferred_x = None;
                        state.visual_cursor = None;
                        state.reset_blink();
                        shell.capture_event();
                        shell.request_redraw();
                        return;
                    }
                    let (mut head, y) = state
                        .rich
                        .resident()
                        .hit_source_with_row(position + Vector::new(0.0, state.scroll));
                    let point = position + Vector::new(0.0, state.scroll);
                    if let Some(block) = state
                        .rich
                        .resident()
                        .visible_fragments(point.y..point.y)
                        .find_map(|fragment| {
                            self.projection
                                .rich
                                .code_line(&fragment.range)
                                .map(|(_, block)| block)
                                .filter(|block| {
                                    block.fenced
                                        && block.source_body.is_empty()
                                        && fragment.range.start == block.body.start
                                        && fragment.bounds.contains(point)
                                        && matches!(fragment.kind, FragmentKind::Text { .. })
                                })
                        })
                    {
                        head = block.source_body.start;
                    }
                    state.set_visual_cursor(head, y);
                    let anchor = if state.modifiers.shift() {
                        self.source_selection.anchor
                    } else {
                        head
                    };
                    let display = self.projection.to_display(head);
                    if click.kind() == mouse::click::Kind::Double
                        && self
                            .projection
                            .rich
                            .images
                            .iter()
                            .any(|image| image.range.start <= display && display <= image.range.end)
                    {
                        shell.publish(Message::Writing(WritingMessage::ImageAt(head)));
                        shell.capture_event();
                        return;
                    }
                    let source_selection = match click.kind() {
                        mouse::click::Kind::Double => {
                            self.raw_selection(self.word_selection(display))
                        }
                        mouse::click::Kind::Triple => {
                            self.raw_selection(self.line_selection(display))
                        }
                        _ => self.projection.normalize_caret(Selection { anchor, head }),
                    };
                    if click.kind() == mouse::click::Kind::Single
                        && !state.modifiers.shift()
                        && let Some((table, row, column)) = state.rich.resident().cell_at(point)
                    {
                        if crate::document::table_edit::materialize_cell(
                            self.source,
                            row,
                            column,
                            &self.projection.rich.tables[table],
                        )
                        .is_some()
                        {
                            state.pending_table_drag = Some(Arc::clone(&self.projection.source_id));
                        }
                        shell.publish(Message::Table(TableMessage::FocusCell {
                            revision: Arc::clone(&self.projection.source_id),
                            table,
                            row,
                            column,
                            selection: source_selection,
                            open: false,
                        }));
                    } else {
                        shell.publish(Message::SelectSource(source_selection));
                    }
                    state.focused = true;
                    state.window_focused = true;
                    state.last_click = Some(click);
                    state.drag_anchor = (click.kind() == mouse::click::Kind::Single)
                        .then_some(source_selection.anchor);
                    state.stable_drag = state.drag_anchor.and_then(|anchor| {
                        self.collaboration
                            .map(|document| document.position(anchor, Assoc::After))
                    });
                    state.drag_position = Some(position);
                    state.preferred_x = None;
                    state.reset_blink();
                    shell.capture_event();
                    shell.request_redraw();
                } else if state.focused {
                    state.unfocus();
                    shell.publish(Message::EditorFocused(false));
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                if let Some(press) = &mut state.pending_task {
                    press.cancelled = true;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.pending_task.is_some() => {
                if let Some(press) = &mut state.pending_task {
                    press.cancelled |= cursor.position_in(bounds).is_none_or(|position| {
                        let movement = position - press.position;
                        movement.x * movement.x + movement.y * movement.y > 16.0
                    });
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.scroll_drag.is_some() => {
                if let Some(position) = cursor.position() {
                    state.drag_scrollbar(position.y - bounds.y, state.scroll_drag.unwrap_or(0.0));
                    self.refresh_viewport(state, bounds.size());
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. })
                if state.drag_anchor.is_some() || state.grid_drag.is_some() =>
            {
                state.drag_position = cursor
                    .position()
                    .map(|point| point - (bounds.position() - Point::ORIGIN));
                state.drag(self.projection, shell);
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.pending_table_drag = None;
                state.grid_drag = None;
                state.grid_drag_revision = None;
                if let Some(press) = state.pending_task.take() {
                    if !press.cancelled
                        && press.scroll == state.scroll
                        && press.layout_epoch == state.layout_epoch
                        && Arc::ptr_eq(&press.revision, &self.projection.source_id)
                        && cursor.position_in(bounds).is_some_and(|position| {
                            task_at(
                                state,
                                self.projection,
                                position + Vector::new(0.0, state.scroll),
                            ) == Some(press.marker.clone())
                        })
                    {
                        shell.publish(Message::ToggleTaskSource {
                            marker: press.marker,
                            revision: press.revision,
                        });
                    }
                    shell.capture_event();
                }
                state.drag_anchor = None;
                state.stable_drag = None;
                state.drag_position = None;
                state.scroll_drag = None;
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                if let Some(press) = &mut state.pending_task {
                    press.cancelled = true;
                }
                let amount = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => {
                        -*y * self.theme.typography.body_size * 3.0
                    }
                    mouse::ScrollDelta::Pixels { y, .. } => -*y,
                };
                state.scroll += amount;
                state.clamp_scroll();
                self.refresh_viewport(state, bounds.size());
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                ..
            }) if state.focused => {
                if state.preedit.as_ref().is_none_or(|p| p.content.is_empty()) {
                    state.composition_target = None;
                }
                state.modifiers = *modifiers;
                let press = KeyPress {
                    key: key.clone(),
                    modified_key: modified_key.clone(),
                    physical_key: *physical_key,
                    modifiers: *modifiers,
                    text: text.clone(),
                    status: Status::Focused {
                        is_hovered: cursor.is_over(bounds),
                    },
                };
                let in_table = crate::document::table_edit::cell_at(
                    self.projection,
                    Selection::caret(self.source_selection.head),
                )
                .is_some();
                if in_table
                    && state
                        .preedit
                        .as_ref()
                        .is_some_and(|preedit| !preedit.content.is_empty())
                {
                    shell.capture_event();
                    return;
                }
                if (self.grid.is_some() || in_table && self.source_selection.is_caret())
                    && modifiers.is_empty()
                    && matches!(
                        key,
                        keyboard::Key::Named(
                            keyboard::key::Named::ArrowUp | keyboard::key::Named::ArrowDown
                        )
                    )
                    && self.leave_table_vertically(
                        state,
                        bounds,
                        matches!(key, keyboard::Key::Named(keyboard::key::Named::ArrowUp)),
                        shell,
                    )
                {
                    state.reset_blink();
                    shell.capture_event();
                    shell.request_redraw();
                    return;
                }
                if (self.grid.is_some() || in_table && self.source_selection.is_caret())
                    && self.grid_key(key, modifiers, text.as_deref(), shell)
                {
                    state.preferred_x = None;
                    state.visual_cursor = None;
                    state.reset_blink();
                    shell.capture_event();
                    shell.request_redraw();
                    return;
                }
                if in_table
                    && matches!(key, keyboard::Key::Named(keyboard::key::Named::Enter))
                    && modifiers.command()
                {
                    shell.publish(Message::Edit(Action::Edit(Edit::Enter)));
                    shell.capture_event();
                } else if in_table
                    && matches!(key, keyboard::Key::Named(keyboard::key::Named::Tab))
                    && *modifiers == keyboard::Modifiers::CTRL
                {
                    shell.publish(Message::Table(TableMessage::InsertTab));
                    shell.capture_event();
                } else if in_table
                    && matches!(key, keyboard::Key::Named(keyboard::key::Named::F10))
                    && *modifiers == keyboard::Modifiers::SHIFT
                {
                    shell.publish(Message::Table(TableMessage::OpenMenu));
                    shell.capture_event();
                } else if matches!(key, keyboard::Key::Named(keyboard::key::Named::Tab))
                    && in_table
                    && (modifiers.is_empty() || *modifiers == keyboard::Modifiers::SHIFT)
                {
                    state.preferred_x = None;
                    state.visual_cursor = None;
                    shell.publish(Message::TableTab(modifiers.shift()));
                    shell.capture_event();
                } else if let Some(binding) = (self.key_binding)(press) {
                    self.apply_binding(binding, state, clipboard, shell, bounds.size());
                    shell.capture_event();
                }
                state.reset_blink();
                shell.request_redraw();
            }
            Event::InputMethod(event) if state.focused => {
                match event {
                    input_method::Event::Opened => {
                        state.preedit = Some(input_method::Preedit::new());
                        state.composition_target = None;
                    }
                    input_method::Event::Closed => {
                        state.preedit = None;
                        state.composition_target = None;
                    }
                    input_method::Event::Preedit(content, selection) => {
                        // Empty preedit may cancel composition or precede Commit.
                        // Keep its target for Commit, but recapture for a new run.
                        if !content.is_empty()
                            && (state.composition_target.is_none()
                                || state.preedit.as_ref().is_none_or(|p| p.content.is_empty()))
                        {
                            state.composition_target = self
                                .collaboration
                                .map(|document| document.composition_target(self.source_selection));
                        }
                        state.preedit = Some(input_method::Preedit {
                            content: content.clone(),
                            selection: selection.clone(),
                            text_size: Some((self.theme.typography.body_size * self.zoom).into()),
                        })
                    }
                    input_method::Event::Commit(text) => {
                        state.preferred_x = None;
                        state.preedit =
                            state.preedit.as_ref().map(|_| input_method::Preedit::new());
                        if let Some(grid) = self.grid.or(self.grid_resume) {
                            if self.grid.is_none() {
                                self.activate_grid(grid, shell);
                            }
                            shell.publish(Message::TableGrid(TableGridMessage::Begin(Some(
                                text.clone(),
                            ))));
                        } else if let Some(target) = state.composition_target.take().or_else(|| {
                            self.collaboration
                                .map(|document| document.composition_target(self.source_selection))
                        }) {
                            shell.publish(Message::CommitComposition {
                                target,
                                text: text.clone(),
                            });
                        } else {
                            shell.publish(Message::Edit(Action::Edit(Edit::Paste(Arc::new(
                                text.clone(),
                            )))));
                        }
                    }
                }
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Window(window::Event::Unfocused) => {
                state.window_focused = false;
                state.pending_task = None;
                state.drag_anchor = None;
                state.stable_drag = None;
                state.scroll_drag = None;
                shell.request_redraw();
            }
            Event::Window(window::Event::Focused) => {
                state.window_focused = true;
                state.reset_blink();
                shell.request_redraw();
            }
            Event::Window(window::Event::RedrawRequested(now))
                if state.focused && state.window_focused =>
            {
                state.now = Some(*now);
                if let Some(position) = state.drag_position
                    && (state.drag_anchor.is_some() || state.grid_drag.is_some())
                {
                    let overflow = if position.y < 0.0 {
                        position.y
                    } else {
                        (position.y - bounds.height).max(0.0)
                    };
                    if overflow != 0.0 {
                        state.scroll += overflow.clamp(-24.0, 24.0);
                        state.clamp_scroll();
                        self.refresh_viewport(state, bounds.size());
                        state.drag(self.projection, shell);
                        shell.request_redraw_at(*now + Duration::from_millis(16));
                    }
                }
                let elapsed = state
                    .blink_started
                    .map_or(0, |start| now.saturating_duration_since(start).as_millis());
                shell.request_redraw_at(*now + Duration::from_millis((500 - elapsed % 500) as u64));
                let caret = state.caret(self.source_selection.head)
                    + (bounds.position() - Point::ORIGIN)
                    - Vector::new(0.0, state.scroll);
                shell.request_input_method(&input_method::InputMethod::Enabled {
                    cursor: caret,
                    purpose: input_method::Purpose::Normal,
                    preedit: state.preedit.as_ref().map(input_method::Preedit::as_ref),
                });
            }
            _ => {}
        }
        if state.scroll != previous_scroll
            && let Some(document) = self.collaboration
        {
            state.stable_viewport = state
                .rich
                .resident()
                .viewport_anchor(state.scroll)
                .map(|(offset, y)| (document.position(offset, Assoc::After), y - state.scroll));
        }
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        let translation = bounds.position() - Point::ORIGIN - Vector::new(0.0, state.scroll);
        let tokens = self.theme.tokens;
        use iced::advanced::Renderer as _;
        renderer.with_layer(clip, |renderer| {
            let visible = clip.y - translation.y..clip.y + clip.height - translation.y;
            for (rect, kind) in state.rich.resident().visible_decorations(visible.clone()) {
                let rect = *rect + translation;
                if !rect.intersects(&clip) {
                    continue;
                }
                let (background, border) = match kind {
                    Decoration::Code { first, last } => (
                        tokens.code,
                        iced::Border {
                            radius: iced::border::Radius {
                                top_left: if *first { 8.0 } else { 0.0 },
                                top_right: if *first { 8.0 } else { 0.0 },
                                bottom_left: if *last { 8.0 } else { 0.0 },
                                bottom_right: if *last { 8.0 } else { 0.0 },
                            },
                            ..iced::Border::default()
                        },
                    ),
                    Decoration::InlineCode => (
                        tokens.code,
                        iced::Border {
                            radius: 3.0.into(),
                            ..iced::Border::default()
                        },
                    ),
                    Decoration::Strikethrough(_) => continue,
                    Decoration::TableRow { header } => {
                        if *header {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: rect,
                                    ..renderer::Quad::default()
                                },
                                crate::theme::styles::table_header(tokens),
                            );
                        }
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle::new(
                                    Point::new(rect.x, rect.y + rect.height - 1.0),
                                    Size::new(rect.width, 1.0),
                                ),
                                ..renderer::Quad::default()
                            },
                            crate::theme::styles::table_divider(tokens, *header),
                        );
                        continue;
                    }
                };
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: rect,
                        border,
                        ..renderer::Quad::default()
                    },
                    background,
                );
            }
            if state.focused
                && self.source_selection.is_caret()
                && self
                    .projection
                    .rich
                    .source_code_at(self.source_selection.head)
                    .is_some_and(|block| block.source_body.contains(&self.source_selection.head))
            {
                let caret = state.caret(self.source_selection.head);
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle::new(
                            Point::new(8.0 * self.zoom, caret.y),
                            Size::new((bounds.width - 16.0 * self.zoom).max(1.0), caret.height),
                        ) + translation,
                        border: iced::Border {
                            radius: 3.0.into(),
                            ..iced::Border::default()
                        },
                        ..renderer::Quad::default()
                    },
                    tokens.accent_soft.scale_alpha(0.5),
                );
            }
            draw_table_grid(renderer, state, self.grid, tokens, translation);
            if state.focused && self.grid.is_none() {
                for rect in state
                    .rich
                    .resident()
                    .source_selection_rects_in(self.source_selection, visible.clone())
                {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: rect + translation,
                            ..renderer::Quad::default()
                        },
                        tokens.selection,
                    );
                }
            }
            for fragment in state.rich.resident().visible_fragments(visible.clone()) {
                let rect = fragment.bounds + translation;
                if !rect.intersects(&clip) {
                    continue;
                }
                match &fragment.kind {
                    FragmentKind::CodeHeader { block } => {
                        if let Some(block) = self.projection.rich.code_blocks.get(*block) {
                            let copied = state.copied_code.is_some_and(|(at, time)| {
                                at == block.source_range.start
                                    && time.elapsed() < Duration::from_secs(2)
                            });
                            let button = code_copy_bounds(rect, self.zoom);
                            let hovered = cursor
                                .position()
                                .is_some_and(|point| button.contains(point));
                            if hovered || copied {
                                renderer.fill_quad(
                                    renderer::Quad {
                                        bounds: button,
                                        border: iced::Border {
                                            radius: 5.0.into(),
                                            ..iced::Border::default()
                                        },
                                        ..renderer::Quad::default()
                                    },
                                    tokens.accent_soft,
                                );
                            }
                            for (label, point, color) in [
                                (
                                    if block.language.is_empty() {
                                        "Plain text"
                                    } else {
                                        &block.language
                                    },
                                    Point::new(rect.x + 16.0 * self.zoom, rect.y + 9.0 * self.zoom),
                                    tokens.muted,
                                ),
                                (
                                    if copied { "Copied" } else { "Copy" },
                                    Point::new(
                                        button.x + 12.0 * self.zoom,
                                        rect.y + 9.0 * self.zoom,
                                    ),
                                    if hovered || copied {
                                        tokens.accent
                                    } else {
                                        tokens.muted
                                    },
                                ),
                            ] {
                                renderer.fill_text(
                                    text::Text {
                                        content: label.into(),
                                        bounds: Size::new(
                                            (button.x - rect.x - 24.0 * self.zoom).max(1.0),
                                            rect.height,
                                        ),
                                        size: (11.0 * self.zoom).into(),
                                        font: iced::Font::MONOSPACE,
                                        line_height: text::LineHeight::Relative(1.2),
                                        align_x: text::Alignment::Default,
                                        align_y: iced::alignment::Vertical::Top,
                                        shaping: text::Shaping::Advanced,
                                        wrapping: text::Wrapping::None,
                                    },
                                    point,
                                    color,
                                    clip,
                                );
                            }
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: Rectangle::new(
                                        Point::new(
                                            rect.x + 12.0 * self.zoom,
                                            rect.y + rect.height - 1.0,
                                        ),
                                        Size::new((rect.width - 24.0 * self.zoom).max(1.0), 1.0),
                                    ),
                                    ..renderer::Quad::default()
                                },
                                tokens.border.scale_alpha(0.5),
                            );
                        }
                    }
                    FragmentKind::Text {
                        paragraph, origin, ..
                    } => {
                        renderer.fill_paragraph(
                            paragraph,
                            *origin + translation,
                            tokens.text,
                            clip,
                        );
                        if self.projection.rich.code_line(&fragment.range).is_some_and(
                            |(_, block)| {
                                block.fenced
                                    && block.source_body.is_empty()
                                    && fragment.range.start == block.body.start
                            },
                        ) {
                            renderer.fill_text(
                                text::Text {
                                    content: "Start coding…".into(),
                                    bounds: rect.size(),
                                    size: crate::theme::styles::code_size(
                                        self.theme.typography.body_size * self.zoom,
                                    )
                                    .into(),
                                    font: iced::Font::MONOSPACE,
                                    line_height: text::LineHeight::Relative(
                                        crate::theme::styles::CODE_LINE_HEIGHT,
                                    ),
                                    align_x: text::Alignment::Default,
                                    align_y: iced::alignment::Vertical::Top,
                                    shaping: text::Shaping::Advanced,
                                    wrapping: text::Wrapping::None,
                                },
                                *origin + translation,
                                tokens.muted,
                                clip,
                            );
                        }
                    }
                    FragmentKind::Image { handle, label } => {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: rect,
                                border: iced::Border {
                                    color: tokens.border,
                                    width: 1.0,
                                    radius: 6.0.into(),
                                },
                                ..renderer::Quad::default()
                            },
                            tokens.surface,
                        );
                        if let Some(handle) = handle {
                            let mut image = image::Image::new(handle.clone());
                            image.border_radius = 6.0.into();
                            renderer.draw_image(image, rect, clip);
                        } else {
                            renderer.fill_text(
                                text::Text {
                                    content: label.clone(),
                                    bounds: rect.size(),
                                    size: 14.0.into(),
                                    font: iced::Font::DEFAULT,
                                    line_height: text::LineHeight::Relative(1.5),
                                    align_x: text::Alignment::Center,
                                    align_y: iced::alignment::Vertical::Center,
                                    shaping: text::Shaping::Advanced,
                                    wrapping: text::Wrapping::WordOrGlyph,
                                },
                                rect.center(),
                                tokens.muted,
                                clip,
                            );
                        }
                        let selected = self.display_selection().range();
                        if selected.start <= fragment.range.start
                            && selected.end >= fragment.range.end
                            && state.focused
                        {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: rect,
                                    border: iced::Border {
                                        color: tokens.accent,
                                        width: 2.0,
                                        radius: 6.0.into(),
                                    },
                                    ..renderer::Quad::default()
                                },
                                tokens.selection.scale_alpha(0.35),
                            );
                        }
                    }
                }
            }
            // Strikes belong above the selection and glyphs. An opaque theme
            // selection must not conceal formatting while text is selected.
            for (rect, kind) in state.rich.resident().visible_decorations(visible) {
                if let Decoration::Strikethrough(color) = kind {
                    let rect = *rect + translation;
                    if !rect.intersects(&clip) {
                        continue;
                    }
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: rect,
                            ..renderer::Quad::default()
                        },
                        *color,
                    );
                }
            }
            if self.source.is_empty() {
                renderer.fill_text(
                    text::Text {
                        content: "Begin anywhere…".into(),
                        bounds: bounds.size(),
                        size: (self.theme.typography.body_size * self.zoom).into(),
                        font: self.theme.typography.font.font(),
                        line_height: text::LineHeight::Relative(self.theme.typography.line_height),
                        align_x: text::Alignment::Default,
                        align_y: iced::alignment::Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: text::Wrapping::WordOrGlyph,
                    },
                    bounds.position() + Vector::new(0.0, 5.0),
                    tokens.faint,
                    clip,
                );
            }
            if self.grid.is_none() && self.display_selection().is_caret() && state.caret_visible() {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: state.caret(self.source_selection.head) + translation,
                        ..renderer::Quad::default()
                    },
                    tokens.accent,
                );
            }
            if state.rich.height() > bounds.height {
                let (y, height) = state.scrollbar();
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle::new(
                            Point::new(bounds.x + bounds.width - 4.0, bounds.y + y),
                            Size::new(3.0, height),
                        ),
                        border: iced::Border {
                            radius: 2.0.into(),
                            ..iced::Border::default()
                        },
                        ..renderer::Quad::default()
                    },
                    tokens.border,
                );
            }
        });
        // Only the handle layer may reach into the page margin. The document
        // and draft remain clipped to the editor's original content bounds.
        let handle_bounds = Rectangle::new(
            Point::new(bounds.x - 28.0, bounds.y),
            Size::new(bounds.width + 28.0, bounds.height),
        );
        if let Some(handle_clip) = handle_bounds.intersection(viewport) {
            renderer.with_layer(handle_clip, |renderer| {
                draw_table_handles(
                    renderer,
                    state,
                    self,
                    translation,
                    handle_clip,
                    cursor.position().map(|point| point - translation),
                )
            });
        }
        if let (Some(child), Some(child_layout)) = (&self.cell_editor, layout.children().next()) {
            renderer.with_layer(clip, |renderer| {
                child.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    _theme,
                    _style,
                    child_layout,
                    cursor,
                    &clip,
                )
            });
        }
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.table_menu_available()
            && cursor
                .position()
                .map(|point| point - (layout.bounds().position() - Point::ORIGIN))
                .filter(|point| point.y >= 0.0 && point.y <= layout.bounds().height)
                .is_some_and(|position| {
                    table_menu_hit(
                        &state.rich,
                        position + Vector::new(0.0, state.scroll),
                        self.zoom,
                        self.grid.map(|grid| grid.table),
                    )
                    .is_some()
                })
        {
            return mouse::Interaction::Pointer;
        }
        if let (Some(child), Some(child_layout)) = (&self.cell_editor, layout.children().next())
            && cursor.is_over(child_layout.bounds())
        {
            return child.as_widget().mouse_interaction(
                &tree.children[0],
                child_layout,
                cursor,
                _viewport,
                _renderer,
            );
        }
        if cursor
            .position()
            .map(|point| point - (layout.bounds().position() - Point::ORIGIN))
            .filter(|point| point.y >= 0.0 && point.y <= layout.bounds().height)
            .is_some_and(|position| {
                table_hit(
                    &state.rich,
                    position + Vector::new(0.0, state.scroll),
                    self.zoom,
                    self.grid.map(|grid| grid.table),
                )
                .is_some_and(|hit| {
                    hit.3 != TableExtent::Cell || self.grid.is_some_and(|grid| grid.table == hit.0)
                })
            })
        {
            return mouse::Interaction::Crosshair;
        }
        if cursor.position_in(layout.bounds()).is_some_and(|position| {
            let point = position + Vector::new(0.0, state.scroll);
            code_copy_at(state, point, self.zoom).is_some()
                || task_at(state, self.projection, point).is_some()
        }) || state.scroll_drag.is_some()
            || cursor
                .position_in(layout.bounds())
                .is_some_and(|p| p.x >= layout.bounds().width - 10.0)
                && state.rich.height() > layout.bounds().height
        {
            mouse::Interaction::Pointer
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

// Handles are overlays: entering a table never changes cell or text geometry.
fn row_handle(bounds: Rectangle) -> Rectangle {
    Rectangle::new(
        Point::new(bounds.x - 28.0, bounds.y),
        Size::new(26.0, bounds.height),
    )
}

fn column_handle(bounds: Rectangle, zoom: f32) -> Rectangle {
    let half = crate::theme::styles::TABLE_TOP_SPACING * zoom;
    Rectangle::new(
        Point::new(bounds.x, bounds.y - half),
        Size::new(bounds.width, half * 2.0),
    )
}

fn row_badge(bounds: Rectangle, zoom: f32) -> Rectangle {
    let height = (22.0 * zoom).min(bounds.height);
    Rectangle::new(
        Point::new(bounds.x - 28.0, bounds.center_y() - height / 2.0),
        Size::new(26.0, height),
    )
}

// Menu targets occupy existing handle space. The row arrow sits below its
// number; column arrows sit at the trailing edge, leaving labels stationary.
fn table_menu_bounds(bounds: Rectangle, extent: TableExtent, zoom: f32) -> Rectangle {
    if extent == TableExtent::Row {
        let badge = row_badge(bounds, zoom);
        let height = (bounds.y + bounds.height - badge.y - badge.height).min(14.0 * zoom);
        Rectangle::new(
            Point::new(badge.x, badge.y + badge.height),
            Size::new(badge.width, height),
        )
    } else {
        let header = column_handle(bounds, zoom);
        let width = (20.0 * zoom).min(header.width * 0.4);
        Rectangle::new(
            Point::new(header.x + header.width - width, header.y),
            Size::new(width, header.height),
        )
    }
}

fn table_menu_hit(
    rich: &VirtualLayout,
    point: Point,
    zoom: f32,
    active: Option<usize>,
) -> Option<(usize, usize, usize, TableExtent, Rectangle)> {
    let (table, row, column, extent) = table_hit(rich, point, zoom, active)?;
    if !matches!(extent, TableExtent::Row | TableExtent::Column) {
        return None;
    }
    let cell = rich.resident().cell_bounds(table, row, column)?;
    let bounds = table_menu_bounds(cell, extent, zoom);
    (bounds.width >= 6.0 && bounds.height >= 6.0 && bounds.contains(point))
        .then_some((table, row, column, extent, bounds))
}

fn table_hit(
    rich: &VirtualLayout,
    point: Point,
    zoom: f32,
    active: Option<usize>,
) -> Option<(usize, usize, usize, TableExtent)> {
    for (table, row, column, bounds) in rich
        .resident()
        .cells()
        .filter(|cell| Some(cell.0) == active)
    {
        if column == 0 && row_handle(bounds).contains(point) {
            return Some((table, row, 0, TableExtent::Row));
        }
        if row == 0 {
            let header = column_handle(bounds, zoom);
            if header.contains(point) {
                return Some((table, 0, column, TableExtent::Column));
            }
            let corner = Rectangle::new(
                Point::new(bounds.x - 28.0, header.y),
                Size::new(26.0, header.height),
            );
            if column == 0 && corner.contains(point) {
                return Some((table, 0, 0, TableExtent::Table));
            }
        }
    }
    rich.resident()
        .cell_at(point)
        .map(|(table, row, column)| (table, row, column, TableExtent::Cell))
}

fn draw_table_grid(
    renderer: &mut Renderer,
    state: &State,
    grid: Option<TableGridView>,
    tokens: Tokens,
    translation: Vector,
) {
    use iced::advanced::Renderer as _;
    let Some(grid) = grid else {
        return;
    };
    let quad =
        |renderer: &mut Renderer, rect: Rectangle, color: iced::Color, border: iced::Border| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: rect + translation,
                    border,
                    ..renderer::Quad::default()
                },
                color,
            );
        };
    for (_, row, column, rect) in state
        .rich
        .resident()
        .cells()
        .filter(|cell| cell.0 == grid.table)
    {
        quad(
            renderer,
            rect,
            iced::Color::TRANSPARENT,
            iced::Border {
                color: tokens.border,
                width: 0.5,
                ..iced::Border::default()
            },
        );
        if grid.contains(row, column) {
            if (row, column) != grid.active {
                quad(
                    renderer,
                    rect,
                    tokens.selection.scale_alpha(0.6),
                    iced::Border::default(),
                );
            }
            let width = 1.5;
            for (edge, show) in [
                (
                    Rectangle::new(rect.position(), Size::new(rect.width, width)),
                    row == grid.anchor.0.min(grid.head.0),
                ),
                (
                    Rectangle::new(
                        Point::new(rect.x, rect.y + rect.height - width),
                        Size::new(rect.width, width),
                    ),
                    row == grid.anchor.0.max(grid.head.0),
                ),
                (
                    Rectangle::new(rect.position(), Size::new(width, rect.height)),
                    column == grid.anchor.1.min(grid.head.1),
                ),
                (
                    Rectangle::new(
                        Point::new(rect.x + rect.width - width, rect.y),
                        Size::new(width, rect.height),
                    ),
                    column == grid.anchor.1.max(grid.head.1),
                ),
            ] {
                if show {
                    quad(renderer, edge, tokens.accent, iced::Border::default());
                }
            }
            if (row, column) == grid.active {
                quad(
                    renderer,
                    rect,
                    iced::Color::TRANSPARENT,
                    iced::Border {
                        color: tokens.accent,
                        width: 2.0,
                        ..iced::Border::default()
                    },
                );
            }
        }
    }
}

fn draw_table_handles(
    renderer: &mut Renderer,
    state: &State,
    editor: &RichEditor<'_>,
    translation: Vector,
    clip: Rectangle,
    pointer: Option<Point>,
) {
    use iced::advanced::Renderer as _;
    let Some(grid) = editor.grid else {
        return;
    };
    let tokens = editor.theme.tokens;
    let zoom = editor.zoom;
    let hover = pointer
        .filter(|point| (clip - translation).contains(*point))
        .filter(|_| state.table_menu_available())
        .and_then(|point| table_hit(&state.rich, point, zoom, Some(grid.table)));
    let label = |renderer: &mut Renderer, rect: Rectangle, content: String, selected: bool| {
        let rect = rect + translation;
        if !rect.intersects(&clip) {
            return;
        }
        renderer.fill_quad(
            renderer::Quad {
                bounds: rect,
                border: iced::Border {
                    color: tokens.border,
                    width: 0.5,
                    radius: 3.0.into(),
                },
                ..renderer::Quad::default()
            },
            if selected {
                tokens.accent_soft
            } else {
                tokens.surface
            },
        );
        renderer.fill_text(
            text::Text {
                content,
                bounds: rect.size(),
                size: (11.0 * zoom.min(1.0)).into(),
                font: iced::Font::DEFAULT,
                line_height: text::LineHeight::Relative(1.2),
                align_x: text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                shaping: text::Shaping::Advanced,
                wrapping: text::Wrapping::None,
            },
            rect.center(),
            if selected {
                tokens.accent
            } else {
                tokens.muted
            },
            clip,
        );
    };
    for (_, row, column, rect) in state
        .rich
        .resident()
        .cells()
        .filter(|cell| cell.0 == grid.table)
    {
        if column == 0 {
            let badge = row_badge(rect, zoom);
            label(
                renderer,
                badge,
                (row + 1).to_string(),
                (grid.anchor.0.min(grid.head.0)..=grid.anchor.0.max(grid.head.0)).contains(&row),
            );
        }
        if row == 0 {
            let header = column_handle(rect, zoom);
            label(
                renderer,
                header,
                column_name(column),
                (grid.anchor.1.min(grid.head.1)..=grid.anchor.1.max(grid.head.1)).contains(&column),
            );
            if column == 0 {
                label(
                    renderer,
                    Rectangle::new(
                        Point::new(rect.x - 28.0, header.y),
                        Size::new(26.0, header.height),
                    ),
                    "◩".into(),
                    false,
                );
            }
        }
        let extent = if hover == Some((grid.table, row, column, TableExtent::Row)) && column == 0 {
            Some(TableExtent::Row)
        } else if hover == Some((grid.table, row, column, TableExtent::Column)) && row == 0 {
            Some(TableExtent::Column)
        } else {
            None
        };
        if let Some(extent) = extent {
            let menu = table_menu_bounds(rect, extent, zoom);
            if menu.height >= 6.0 && menu.width >= 6.0 {
                label(
                    renderer,
                    menu,
                    "▾".into(),
                    pointer.is_some_and(|point| menu.contains(point)),
                );
            }
        }
    }
}

pub(crate) fn column_name(mut column: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push((b'A' + (column % 26) as u8) as char);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    letters.into_iter().rev().collect()
}

/// The same measured row stops used to paint selections bound the checkbox.
/// Looking up its cached parser marker avoids reparsing or guessing glyph widths.
fn task_at(state: &State, projection: &Projection, point: Point) -> Option<Range<usize>> {
    let (source, _) = state.rich.resident().hit_source_with_row(point);
    let marker = projection.task_marker_at(source)?;
    state
        .rich
        .resident()
        .source_selection_rects_in(
            Selection {
                anchor: marker.start,
                head: marker.end,
            },
            point.y..point.y,
        )
        .iter()
        .any(|bounds| bounds.contains(point))
        .then_some(marker)
}

fn code_copy_bounds(header: Rectangle, zoom: f32) -> Rectangle {
    Rectangle::new(
        Point::new(header.x + header.width - 78.0 * zoom, header.y + 4.0 * zoom),
        Size::new(66.0 * zoom, header.height - 8.0 * zoom),
    )
}

fn code_copy_at(state: &State, point: Point, zoom: f32) -> Option<usize> {
    state
        .rich
        .resident()
        .visible_fragments(point.y..point.y)
        .find_map(|fragment| {
            if let FragmentKind::CodeHeader { block } = fragment.kind
                && code_copy_bounds(fragment.bounds, zoom).contains(point)
            {
                Some(block)
            } else {
                None
            }
        })
}

impl<'a> From<RichEditor<'a>> for Element<'a, Message> {
    fn from(editor: RichEditor<'a>) -> Self {
        Element::new(editor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::native_buffer;

    #[test]
    fn table_handles_only_hit_the_active_table_without_reserving_content_space() {
        let source = "Before\n\n| A | B |\n|---|---|\n| one | two |\n\nBetween\n\n| C | D |\n|---|---|\n| three | four |\n\nAfter";
        let projection = Projection::new(source);
        let theme = crate::theme::Preset::Paper.resolve();
        let images = ImageStore::default();
        for zoom in [0.8, 1.0, 1.5] {
            let input = LayoutInput {
                projection: &projection,
                theme: &theme,
                zoom,
                width: 640.0,
                base: None,
                images: &images,
            };
            let mut rich = VirtualLayout::default();
            rich.rebuild(input);
            rich.show_viewport(input, 0.0, 1600.0);
            for table in 0..2 {
                let first = rich.resident().cell_bounds(table, 0, 0).unwrap();
                assert_eq!(first.x, 0.0);
                let row = rich.resident().cell_bounds(table, 1, 0).unwrap();
                let row_point = row_handle(row).center();
                assert_eq!(table_hit(&rich, row_point, zoom, None), None);
                assert_eq!(table_hit(&rich, row_point, zoom, Some(1 - table)), None);
                assert_eq!(
                    table_hit(&rich, row_point, zoom, Some(table)),
                    Some((table, 1, 0, TableExtent::Row))
                );
                let column = column_handle(first, zoom).center();
                assert_eq!(
                    table_hit(&rich, column, zoom, None),
                    Some((table, 0, 0, TableExtent::Cell))
                );
                assert_eq!(
                    table_hit(&rich, column, zoom, Some(table)),
                    Some((table, 0, 0, TableExtent::Column))
                );
            }
        }
    }
    use crate::theme::Preset;

    #[test]
    fn strikethrough_remains_visible_over_opaque_forward_and_reverse_selections() {
        use iced::advanced::Renderer as _;

        let source = "~~left      right~~";
        let projection = Projection::new(source);
        let content = native_buffer::with_text(&projection.text);
        let images = ImageStore::default();
        let mut theme = Preset::Paper.resolve();
        theme.tokens.text = iced::Color::from_rgb(1.0, 0.0, 1.0);
        theme.tokens.selection = iced::Color::from_rgb(0.0, 1.0, 0.0);
        for selection in [
            Selection {
                anchor: 0,
                head: source.len(),
            },
            Selection {
                anchor: source.len(),
                head: 0,
            },
        ] {
            let mut editor = RichEditor::new(
                &content,
                &projection,
                source,
                &theme,
                &images,
                Binding::from_key_press,
            )
            .selection(selection);
            let mut renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
                iced::Font::DEFAULT,
                18.0.into(),
            ));
            let mut tree = widget::Tree::new(&editor as &dyn Widget<Message, Theme, Renderer>);
            let size = Size::new(320.0, 100.0);
            let clip = Rectangle::with_size(size);
            let node = editor.layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
            let state = tree.state.downcast_mut::<State>();
            state.focused = true;
            let x = state.caret(9).x;
            let strike = state
                .rich
                .resident()
                .decorations
                .iter()
                .find_map(|(bounds, kind)| {
                    (matches!(kind, Decoration::Strikethrough(_))
                        && bounds.x <= x
                        && x < bounds.x + bounds.width)
                        .then_some(*bounds)
                })
                .expect("measured strike covers the whitespace between words");
            let y = strike.y - state.scroll;
            renderer.reset(clip);
            editor.draw(
                &tree,
                &mut renderer,
                &theme.iced,
                &renderer::Style {
                    text_color: theme.tokens.text,
                },
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &clip,
            );
            let mut pixels = tiny_skia::Pixmap::new(320, 100).unwrap();
            let mut mask = tiny_skia::Mask::new(320, 100).unwrap();
            let viewport =
                iced_tiny_skia::graphics::Viewport::with_physical_size(Size::new(320, 100), 1.0);
            if let Renderer::Secondary(renderer) = &mut renderer {
                renderer.draw(
                    &mut pixels.as_mut(),
                    &mut mask,
                    &viewport,
                    &[clip],
                    theme.tokens.canvas,
                );
            }
            assert!(
                (y.floor() as u32..(y + strike.height).ceil() as u32).any(|row| {
                    pixels
                        .pixel(x as u32, row)
                        .is_some_and(|pixel| pixel.red() > 100)
                }),
                "the strike must remain visible through the selected whitespace"
            );
            assert!(
                pixels.pixel(x as u32, (y + 5.0) as u32).unwrap().green() > 200,
                "the regression actually draws an opaque selection behind the strike"
            );
        }
    }

    #[test]
    fn task_checkbox_hit_and_release_use_measured_geometry_without_changing_selection() {
        for scenario in [
            "click", "drag", "double", "wheel", "resize", "revision", "right", "leave",
        ] {
            let source = include_str!("../../tests/fixtures/live-tasks.md");
            let projection = Projection::new(source);
            let changed_source = source.replacen("nested", "altered", 1);
            let changed_projection = Projection::new(&changed_source);
            let content = native_buffer::with_text(&projection.text);
            let theme = Preset::Paper.resolve();
            let images = ImageStore::default();
            let selection = Selection {
                anchor: source.len(),
                head: source.find("Write").unwrap(),
            };
            let mut editor = RichEditor::new(
                &content,
                &projection,
                source,
                &theme,
                &images,
                Binding::from_key_press,
            )
            .selection(selection)
            .focused(true);
            let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
                iced::Font::DEFAULT,
                18.0.into(),
            ));
            let mut tree = widget::Tree::new(&editor as &dyn Widget<Message, Theme, Renderer>);
            let size = Size::new(280.0, 400.0);
            let mut node =
                editor.layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
            let marker = source.find("[X]").unwrap();
            let state = tree.state.downcast_mut::<State>();
            state.visual_cursor = Some((selection.head, true));
            let previous_scroll = state.scroll;
            let checkbox = state.rich.resident().source_selection_rects(Selection {
                anchor: marker,
                head: marker + 3,
            })[0];
            let point = checkbox.center() - Vector::new(0.0, state.scroll);
            assert_eq!(
                task_at(state, &projection, checkbox.center()),
                Some(marker..marker + 3)
            );
            assert!(
                task_at(
                    state,
                    &projection,
                    Point::new(checkbox.x + checkbox.width + 12.0, checkbox.center_y())
                )
                .is_none()
            );
            let mut messages = Vec::new();
            let mut clipboard = iced::advanced::clipboard::Null;
            macro_rules! event {
                ($event:expr, $point:expr) => {
                    editor.update(
                        &mut tree,
                        &$event,
                        Layout::new(&node),
                        mouse::Cursor::Available($point),
                        &renderer,
                        &mut clipboard,
                        &mut Shell::new(&mut messages),
                        &Rectangle::with_size(size),
                    )
                };
            }
            let button = if scenario == "right" {
                mouse::Button::Right
            } else {
                mouse::Button::Left
            };
            event!(Event::Mouse(mouse::Event::ButtonPressed(button)), point);
            assert!(messages.is_empty(), "{scenario}: press does not toggle");
            match scenario {
                "drag" => {
                    event!(
                        Event::Mouse(mouse::Event::CursorMoved {
                            position: point + Vector::new(20.0, 0.0)
                        }),
                        point + Vector::new(20.0, 0.0)
                    );
                    event!(
                        Event::Mouse(mouse::Event::CursorMoved { position: point }),
                        point
                    );
                }
                "leave" => event!(Event::Mouse(mouse::Event::CursorLeft), point),
                "wheel" => event!(
                    Event::Mouse(mouse::Event::WheelScrolled {
                        delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 }
                    }),
                    point
                ),
                "resize" => {
                    node = editor.layout(
                        &mut tree,
                        &renderer,
                        &layout::Limits::new(Size::ZERO, Size::new(240.0, 400.0)),
                    );
                }
                "revision" => {
                    editor.projection = &changed_projection;
                    editor.source = &changed_source;
                }
                _ => {}
            }
            event!(Event::Mouse(mouse::Event::ButtonReleased(button)), point);
            if scenario == "double" {
                event!(Event::Mouse(mouse::Event::ButtonPressed(button)), point);
                event!(Event::Mouse(mouse::Event::ButtonReleased(button)), point);
            }
            if matches!(scenario, "click" | "double") {
                assert_eq!(messages.len(), 1, "{scenario}");
                assert!(
                    matches!(&messages[0], Message::ToggleTaskSource { marker: range, revision }
                    if *range == (marker..marker + 3) && Arc::ptr_eq(revision, &projection.source_id))
                );
            } else {
                assert!(messages.is_empty(), "{scenario}");
            }
            let state = tree.state.downcast_ref::<State>();
            assert_eq!(state.last_selection, Some(selection), "{scenario}");
            assert_eq!(
                state.visual_cursor,
                Some((selection.head, true)),
                "{scenario}"
            );
            if !matches!(scenario, "wheel" | "resize") {
                assert_eq!(state.scroll, previous_scroll, "{scenario}");
            }
        }
    }

    #[test]
    fn virtualized_task_checkbox_is_clickable_after_a_distant_source_jump() {
        let mut source = "Paragraph with **bold** words.\n\n".repeat(600);
        let marker = source.len() + 2;
        source.push_str("- [ ] Distant café task\n");
        let projection = Projection::new(&source);
        let content = native_buffer::with_text(&projection.text);
        let theme = Preset::Paper.resolve();
        let images = ImageStore::default();
        let selection = Selection::caret(marker + 8);
        let mut editor = RichEditor::new(
            &content,
            &projection,
            &source,
            &theme,
            &images,
            Binding::from_key_press,
        )
        .selection(selection)
        .focused(true);
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::DEFAULT,
            18.0.into(),
        ));
        let mut tree = widget::Tree::new(&editor as &dyn Widget<Message, Theme, Renderer>);
        let size = Size::new(280.0, 160.0);
        let node = editor.layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
        let state = tree.state.downcast_ref::<State>();
        assert!(state.rich.resident().rows.len() < 100);
        let checkbox = state.rich.resident().source_selection_rects(Selection {
            anchor: marker,
            head: marker + 3,
        })[0];
        let point = checkbox.center() - Vector::new(0.0, state.scroll);
        assert!(Rectangle::with_size(size).contains(point));
        let mut messages = Vec::new();
        for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            editor.update(
                &mut tree,
                &Event::Mouse(event),
                Layout::new(&node),
                mouse::Cursor::Available(point),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(size),
            );
        }
        assert!(
            matches!(messages.as_slice(), [Message::ToggleTaskSource { marker: range, .. }] if *range == (marker..marker + 3))
        );
    }

    #[test]
    fn copying_a_rich_document_retains_heading_table_and_image_markdown() {
        struct ClipboardText(String);
        impl Clipboard for ClipboardText {
            fn read(&self, _: iced::advanced::clipboard::Kind) -> Option<String> {
                Some(self.0.clone())
            }
            fn write(&mut self, _: iced::advanced::clipboard::Kind, text: String) {
                self.0 = text;
            }
        }
        for source in [
            "# Heading\n\n| **A** | B |\n|---|---|\n| x | y |\n\n![alt](room.png)",
            "# Heading\n\n```rust\nlet café = 1;\n```\n\nAfter",
        ] {
            let projection = Projection::new(source);
            let mut content = native_buffer::with_text(&projection.text);
            content.perform(Action::SelectAll);
            let theme = Preset::Paper.resolve();
            let images = ImageStore::default();
            let editor = RichEditor::new(
                &content,
                &projection,
                source,
                &theme,
                &images,
                Binding::from_key_press,
            )
            .selection(Selection {
                anchor: 0,
                head: source.len(),
            });
            let mut clipboard = ClipboardText(String::new());
            editor.apply_binding(
                Binding::Copy,
                &mut State::default(),
                &mut clipboard,
                &mut Shell::new(&mut Vec::new()),
                Size::new(640.0, 600.0),
            );
            assert_eq!(clipboard.0, source);
        }
    }

    #[test]
    fn code_copy_button_uses_normalized_content_without_moving_selection() {
        #[derive(Default)]
        struct CodeClipboard(String);
        impl Clipboard for CodeClipboard {
            fn read(&self, _: iced::advanced::clipboard::Kind) -> Option<String> {
                None
            }
            fn write(&mut self, _: iced::advanced::clipboard::Kind, text: String) {
                self.0 = text;
            }
        }
        let source = "Before\n\n```rust\r\nlet café = \"🦀\";\r\n```\r\n\nAfter";
        let projection = Projection::new(source);
        let content = native_buffer::with_text(&projection.text);
        let theme = Preset::Paper.resolve();
        let images = ImageStore::default();
        let selection = Selection {
            anchor: source.len(),
            head: 0,
        };
        let mut editor = RichEditor::new(
            &content,
            &projection,
            source,
            &theme,
            &images,
            Binding::from_key_press,
        )
        .selection(selection);
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::DEFAULT,
            18.0.into(),
        ));
        let mut tree = widget::Tree::new(&editor as &dyn Widget<Message, Theme, Renderer>);
        let size = Size::new(640.0, 600.0);
        let node = editor.layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
        let header = tree
            .state
            .downcast_ref::<State>()
            .rich
            .resident()
            .fragments
            .iter()
            .find(|fragment| matches!(fragment.kind, FragmentKind::CodeHeader { .. }))
            .unwrap()
            .bounds;
        let mut clipboard = CodeClipboard::default();
        let mut messages = Vec::new();
        editor.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Layout::new(&node),
            mouse::Cursor::Available(code_copy_bounds(header, 1.0).center()),
            &renderer,
            &mut clipboard,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
        assert_eq!(clipboard.0, "let café = \"🦀\";\n");
        assert!(messages.is_empty());
        assert_eq!(
            tree.state.downcast_ref::<State>().last_selection,
            Some(selection)
        );
        assert!(tree.state.downcast_ref::<State>().copied_code.is_some());
    }

    #[test]
    fn widget_drag_selection_crosses_heading_table_image_and_paragraph_in_both_directions() {
        let source = include_str!("../../tests/fixtures/rich-blocks.md").trim_end_matches('\n');
        let projection = Projection::new(source);
        let content = native_buffer::with_text(&projection.text);
        let theme = Preset::Paper.resolve();
        let images = ImageStore::default();
        let mut editor = RichEditor::new(
            &content,
            &projection,
            source,
            &theme,
            &images,
            Binding::from_key_press,
        );
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::DEFAULT,
            18.0.into(),
        ));
        let mut tree = widget::Tree::new(&editor as &dyn Widget<Message, Theme, Renderer>);
        let node = editor.layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(640.0, 1100.0)),
        );
        let layout = Layout::new(&node);
        let state = tree.state.downcast_ref::<State>();
        let start_offset = 2;
        let end_offset = projection.text.find("After").unwrap() + 3;
        let point = |offset| {
            let caret = state.rich.resident().caret(offset);
            Point::new(caret.x + 0.1, caret.center_y())
        };
        let (start, end) = (point(start_offset), point(end_offset));
        let mut clipboard = iced::advanced::clipboard::Null;
        for (start, end, expected) in [
            (
                start,
                end,
                Selection {
                    anchor: start_offset,
                    head: end_offset,
                },
            ),
            (
                end,
                start,
                Selection {
                    anchor: end_offset,
                    head: start_offset,
                },
            ),
        ] {
            let mut messages = Vec::new();
            for (event, cursor) in [
                (
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    start,
                ),
                (
                    Event::Mouse(mouse::Event::CursorMoved { position: end }),
                    end,
                ),
                (
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    end,
                ),
            ] {
                editor.update(
                    &mut tree,
                    &event,
                    layout,
                    mouse::Cursor::Available(cursor),
                    &renderer,
                    &mut clipboard,
                    &mut Shell::new(&mut messages),
                    &layout.bounds(),
                );
            }
            let selected = messages
                .iter()
                .rev()
                .find_map(|message| match message {
                    Message::SelectSource(selection) => Some(*selection),
                    _ => None,
                })
                .unwrap();
            let raw = selected;
            let selected = projection.display_selection(raw);
            assert_eq!(selected, expected);
            assert_eq!(projection.source_selection(selected, raw), raw);
            assert!(
                tree.state
                    .downcast_ref::<State>()
                    .rich
                    .resident()
                    .selection_rects(selected)
                    .len()
                    > 5
            );
        }
    }

    #[test]
    fn drag_anchor_survives_contextual_disclosure_between_widget_updates() {
        let source = include_str!("../../tests/fixtures/rich-blocks.md").trim_end_matches('\n');
        let theme = Preset::Paper.resolve();
        let images = ImageStore::default();
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::DEFAULT,
            18.0.into(),
        ));
        let mut clipboard = iced::advanced::clipboard::Null;
        let heading = 4;
        let paragraph = source.find("After").unwrap() + 3;
        for (anchor, head) in [(heading, paragraph), (paragraph, heading)] {
            let mut projection = Projection::new(source);
            let mut content = native_buffer::with_text(&projection.text);
            let mut selection = Selection::caret(2);
            let mut tree = widget::Tree::new(&RichEditor::new(
                &content,
                &projection,
                source,
                &theme,
                &images,
                Binding::from_key_press,
            )
                as &dyn Widget<Message, Theme, Renderer>);
            for (event, target) in [
                (
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    anchor,
                ),
                (
                    Event::Mouse(mouse::Event::CursorMoved {
                        position: Point::ORIGIN,
                    }),
                    head,
                ),
                (
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    head,
                ),
            ] {
                let mut editor = RichEditor::new(
                    &content,
                    &projection,
                    source,
                    &theme,
                    &images,
                    Binding::from_key_press,
                )
                .selection(selection)
                .focused(true);
                let node = editor.layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, Size::new(640.0, 1100.0)),
                );
                let state = tree.state.downcast_ref::<State>();
                let caret = state.rich.resident().caret(projection.to_display(target));
                let point = Point::new(caret.x + 0.1, caret.center_y() - state.scroll);
                let event = if matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. })) {
                    Event::Mouse(mouse::Event::CursorMoved { position: point })
                } else {
                    event
                };
                let mut messages = Vec::new();
                editor.update(
                    &mut tree,
                    &event,
                    Layout::new(&node),
                    mouse::Cursor::Available(point),
                    &renderer,
                    &mut clipboard,
                    &mut Shell::new(&mut messages),
                    &Rectangle::with_size(node.size()),
                );
                if let Some(next) = messages.iter().rev().find_map(|message| match message {
                    Message::SelectSource(selection) => Some(*selection),
                    _ => None,
                }) {
                    selection = next;
                }
                drop(editor);
                projection.set_disclosure(projection.disclosure_at(Some(selection.head)));
                content = native_buffer::with_text(&projection.text);
                content.move_to(
                    projection
                        .display_selection(selection)
                        .cursor(&projection.text),
                );
            }
            assert_eq!(selection, Selection { anchor, head });
        }
    }

    #[test]
    fn disclosure_reflow_anchors_caret_viewport_and_wrap_affinity() {
        let source = (0..80)
            .map(|i| format!("## Line{i}withmanywordswithmanywordswithmanywords\n\n"))
            .collect::<String>();
        let first = source.find("Line25").unwrap();
        let second = source.find("Line26").unwrap();
        let mut projection = Projection::new(&source);
        let content = native_buffer::with_text(&projection.text);
        let theme = Preset::Paper.resolve();
        let images = ImageStore::default();
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::DEFAULT,
            18.0.into(),
        ));
        let editor = RichEditor::new(
            &content,
            &projection,
            &source,
            &theme,
            &images,
            Binding::from_key_press,
        );
        let mut tree = widget::Tree::new(&editor as &dyn Widget<Message, Theme, Renderer>);
        let limits = layout::Limits::new(Size::ZERO, Size::new(180.0, 500.0));
        drop(editor);
        let relayout =
            |tree: &mut widget::Tree, projection: &Projection, selection: Selection, focused| {
                let mut content = native_buffer::with_text(&projection.text);
                projection
                    .display_selection(selection)
                    .restore(&mut content, &projection.text);
                RichEditor::new(
                    &content,
                    projection,
                    &source,
                    &theme,
                    &images,
                    Binding::from_key_press,
                )
                .selection(selection)
                .focused(focused)
                .layout(tree, &renderer, &limits);
            };
        relayout(&mut tree, &projection, Selection::caret(first), true);
        let state = tree.state.downcast_mut::<State>();
        state.focused = true;
        state.scroll = state.rich.source_caret(first, None).y - 80.0;
        state.preferred_x = Some(73.0);
        state.drag_anchor = Some(first);
        let height = state.rich.height();
        let mut reflowed = false;
        for (head, focused) in [
            (first, true),
            (first, false),
            (first, true),
            (second, true),
            (first, true),
        ] {
            let state = tree.state.downcast_ref::<State>();
            let expected_y = state.caret(head).y - state.scroll;
            projection.set_disclosure(projection.disclosure_at(focused.then_some(head)));
            let selection = Selection {
                anchor: first,
                head,
            };
            relayout(&mut tree, &projection, selection, focused);
            let state = tree.state.downcast_ref::<State>();
            assert!((state.caret(head).y - state.scroll - expected_y).abs() < 0.01);
            assert_eq!(state.last_selection, Some(selection));
            assert_eq!(state.preferred_x, Some(73.0));
            assert_eq!(state.drag_anchor, Some(first));
            reflowed |= state.rich.height() != height;
        }
        assert!(
            reflowed,
            "heading disclosure should change wrapping in this narrow fixture"
        );

        let state = tree.state.downcast_mut::<State>();
        let caret = state.rich.source_caret(first, None);
        let edge = state.rich.resident().source_line_edge(first, caret, true);
        assert!(state.rich.source_caret_on_side(edge, false).y > caret.y);
        state.visual_cursor = Some((edge, true));
        let expected_y = state.caret(edge).y - state.scroll;
        projection.set_disclosure(projection.disclosure_at(None));
        relayout(&mut tree, &projection, Selection::caret(edge), false);
        let state = tree.state.downcast_ref::<State>();
        assert_eq!(state.visual_cursor.unwrap().0, edge);
        assert_eq!(
            state.caret(edge),
            state.rich.source_caret_on_side(edge, true)
        );
        assert!((state.caret(edge).y - state.scroll - expected_y).abs() < 0.01);

        // Focusing an offscreen caret must retain the visible source row.
        let state = tree.state.downcast_mut::<State>();
        state.scroll = state
            .rich
            .source_caret(source.find("Line60").unwrap(), None)
            .y
            + 7.0;
        state.last_selection = Some(Selection::caret(first));
        let (anchor, y) = state.rich.resident().viewport_anchor(state.scroll).unwrap();
        let screen_y = y - state.scroll;
        projection.set_disclosure(projection.disclosure_at(Some(first)));
        relayout(&mut tree, &projection, Selection::caret(first), true);
        let state = tree.state.downcast_ref::<State>();
        assert!((state.rich.source_caret(anchor, None).y - state.scroll - screen_y).abs() < 0.01);
        assert!(state.caret(first).y < state.scroll);
        assert!(state.scroll <= state.rich.height() - state.viewport_height);

        // An explicit offscreen navigation jump must also retain wrap intent
        // when it changes the active disclosure context.
        projection.set_disclosure(projection.disclosure_at(None));
        relayout(&mut tree, &projection, Selection::caret(first), false);
        let state = tree.state.downcast_mut::<State>();
        let edge = state
            .rich
            .resident()
            .source_line_edge(first, state.caret(first), true);
        state.visual_cursor = Some((edge, true));
        state.last_selection = Some(Selection::caret(first));
        projection.set_disclosure(projection.disclosure_at(Some(edge)));
        relayout(&mut tree, &projection, Selection::caret(edge), true);
        let state = tree.state.downcast_ref::<State>();
        assert_eq!(state.visual_cursor, Some((edge, true)));
        assert_eq!(
            state.caret(edge),
            state.rich.source_caret_on_side(edge, true)
        );
    }

    #[test]
    fn virtualized_navigation_disclosure_resize_and_drag_keep_source_anchors() {
        let source = (0..600).map(|i| format!(
            "## Heading {i}\n\nParagraph {i} with [label](https://example.com/a/very/long/destination/which/wraps) **bold words** and café 🦀.\n\n"
        )).collect::<String>();
        let theme = Preset::Paper.resolve();
        let images = ImageStore::default();
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::DEFAULT,
            18.0.into(),
        ));
        let head = source.find("Paragraph 300").unwrap() + "Paragraph 300 with [la".len();
        let mut projection = Projection::with_context(&source, Some(head));
        let mut content = native_buffer::with_text(&projection.text);
        let mut selection = Selection { anchor: 0, head };
        let initial = RichEditor::new(
            &content,
            &projection,
            &source,
            &theme,
            &images,
            Binding::from_key_press,
        );
        let mut tree = widget::Tree::new(&initial as &dyn Widget<Message, Theme, Renderer>);
        drop(initial);
        let relayout = |tree: &mut widget::Tree,
                        content: &Content,
                        projection: &Projection,
                        selection,
                        width| {
            RichEditor::new(
                content,
                projection,
                &source,
                &theme,
                &images,
                Binding::from_key_press,
            )
            .selection(selection)
            .focused(true)
            .layout(
                tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(width, 500.0)),
            );
        };
        relayout(&mut tree, &content, &projection, selection, 240.0);
        tree.state.downcast_mut::<State>().focused = true;
        for focus in [false, true, false, true] {
            let state = tree.state.downcast_ref::<State>();
            let screen_y = state.caret(head).y - state.scroll;
            projection.set_disclosure(projection.disclosure_at(focus.then_some(head)));
            content = native_buffer::with_text(&projection.text);
            relayout(&mut tree, &content, &projection, selection, 240.0);
            let state = tree.state.downcast_ref::<State>();
            assert!((state.caret(head).y - state.scroll - screen_y).abs() < 0.1);
            assert_eq!(state.last_selection, Some(selection));
            assert!(state.rich.resident().fragments.len() < 100);
        }
        // Scroll away, then disclose and resize while the selection is offscreen.
        let state = tree.state.downcast_mut::<State>();
        state.scroll += 12_000.0;
        let editor = RichEditor::new(
            &content,
            &projection,
            &source,
            &theme,
            &images,
            Binding::from_key_press,
        )
        .selection(selection);
        editor.refresh_viewport(state, Size::new(240.0, 500.0));
        drop(editor);
        let (anchor, y) = state.rich.resident().viewport_anchor(state.scroll).unwrap();
        let screen_y = y - state.scroll;
        projection.set_disclosure(projection.disclosure_at(None));
        content = native_buffer::with_text(&projection.text);
        relayout(&mut tree, &content, &projection, selection, 180.0);
        let state = tree.state.downcast_ref::<State>();
        assert!((state.rich.source_caret(anchor, None).y - state.scroll - screen_y).abs() < 0.15);
        assert!(state.caret(head).y < state.scroll);

        // Shift+PageUp measures a distant, evicted caret before navigating.
        let mut messages = Vec::new();
        RichEditor::new(
            &content,
            &projection,
            &source,
            &theme,
            &images,
            Binding::from_key_press,
        )
        .selection(selection)
        .apply_binding(
            Binding::Select(Motion::PageUp),
            tree.state.downcast_mut::<State>(),
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            Size::new(180.0, 500.0),
        );
        selection = messages
            .into_iter()
            .find_map(|message| match message {
                Message::SelectSource(next) => Some(next),
                _ => None,
            })
            .unwrap();
        assert_eq!(selection.anchor, 0);
        assert!(selection.head < head);
        relayout(&mut tree, &content, &projection, selection, 180.0);
        let state = tree.state.downcast_ref::<State>();
        let caret = state.caret(selection.head);
        assert!(caret.y >= state.scroll && caret.y + caret.height <= state.scroll + 500.0);

        // Drag anchors remain source offsets across several evicted windows,
        // in both directions, without retaining geometry between endpoints.
        for (anchor, positions) in [(0, [0.8, 0.2, 0.7]), (source.len(), [0.1, 0.7, 0.0])] {
            let state = tree.state.downcast_mut::<State>();
            state.drag_anchor = Some(anchor);
            state.drag_position = Some(Point::new(80.0, 250.0));
            for fraction in positions {
                state.scroll = state.rich.height() * fraction;
                let editor = RichEditor::new(
                    &content,
                    &projection,
                    &source,
                    &theme,
                    &images,
                    Binding::from_key_press,
                );
                editor.refresh_viewport(state, Size::new(180.0, 500.0));
                let expected = state
                    .rich
                    .resident()
                    .hit_source_with_row(Point::new(80.0, state.scroll + 250.0))
                    .0;
                let mut messages = Vec::new();
                state.drag(&projection, &mut Shell::new(&mut messages));
                assert!(
                    matches!(messages.as_slice(), [Message::SelectSource(next)] if next.anchor == anchor && next.head == expected)
                );
                assert!(state.rich.resident().fragments.len() < 100);
            }
        }

        // Replacing a document must reveal its initial caret even when the old
        // document was scrolled away with that same source selection.
        let state = tree.state.downcast_mut::<State>();
        state.scroll = state.rich.height() * 0.7;
        let editor = RichEditor::new(
            &content,
            &projection,
            &source,
            &theme,
            &images,
            Binding::from_key_press,
        );
        editor.refresh_viewport(state, Size::new(180.0, 500.0));
        state.last_selection = Some(Selection::caret(0));
        state.focused = false;
        let replacement = "A new document with its own text.\n\n".repeat(600);
        let projection = Projection::new(&replacement);
        let content = native_buffer::with_text(&projection.text);
        RichEditor::new(
            &content,
            &projection,
            &replacement,
            &theme,
            &images,
            Binding::from_key_press,
        )
        .layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(180.0, 500.0)),
        );
        let state = tree.state.downcast_ref::<State>();
        assert!(state.scroll <= 5.0);
        assert!(state.caret(0).y >= state.scroll);
        assert!(state.rich.resident().fragments.len() < 100);
    }

    #[test]
    fn wrap_affinity_survives_eviction_before_keyboard_navigation() {
        let source = format!("{}\n\n", "Continuouswordswithoutspaces".repeat(6)).repeat(300);
        let projection = Projection::new(&source);
        let content = native_buffer::with_text(&projection.text);
        let theme = Preset::Paper.resolve();
        let images = ImageStore::default();
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::DEFAULT,
            18.0.into(),
        ));
        let head = source.len() / 2 + 2;
        let mut editor = RichEditor::new(
            &content,
            &projection,
            &source,
            &theme,
            &images,
            Binding::from_key_press,
        )
        .selection(Selection::caret(head));
        let mut tree = widget::Tree::new(&editor as &dyn Widget<Message, Theme, Renderer>);
        editor.layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(180.0, 500.0)),
        );
        let state = tree.state.downcast_mut::<State>();
        let edge = state
            .rich
            .resident()
            .source_line_edge(head, state.caret(head), true);
        assert!(
            state.rich.source_caret_on_side(edge, false).y
                > state.rich.source_caret_on_side(edge, true).y
        );
        state.visual_cursor = Some((edge, true));
        let expected = state
            .rich
            .resident()
            .source_line_edge(edge, state.caret(edge), false);
        assert_ne!(expected, edge);
        state.scroll = 0.0;
        editor.refresh_viewport(state, Size::new(180.0, 500.0));
        assert!(!state.rich.contains_source(edge));
        let mut messages = Vec::new();
        editor.source_selection = Selection::caret(edge);
        editor.apply_binding(
            Binding::Move(Motion::Home),
            state,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            Size::new(180.0, 500.0),
        );
        assert!(
            matches!(messages.as_slice(), [Message::SelectSource(next)] if *next == Selection::caret(expected))
        );
    }

    #[test]
    fn ime_preedit_is_transient_and_commit_produces_one_edit() {
        let projection = Projection::new("# Title");
        let content = native_buffer::with_text(&projection.text);
        let theme = Preset::Paper.resolve();
        let images = ImageStore::default();
        let mut editor = RichEditor::new(
            &content,
            &projection,
            "# Title",
            &theme,
            &images,
            Binding::from_key_press,
        );
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::DEFAULT,
            18.0.into(),
        ));
        let mut tree = widget::Tree::new(&editor as &dyn Widget<Message, Theme, Renderer>);
        let node = editor.layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(640.0, 600.0)),
        );
        operation::Focusable::focus(tree.state.downcast_mut::<State>());
        let mut messages = Vec::new();
        let mut clipboard = iced::advanced::clipboard::Null;
        for event in [
            input_method::Event::Opened,
            input_method::Event::Preedit("にほん".into(), Some(0..9)),
            input_method::Event::Commit("日本語".into()),
        ] {
            editor.update(
                &mut tree,
                &Event::InputMethod(event),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut clipboard,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(node.size()),
            );
        }
        assert_eq!(content.text(), "Title");
        assert_eq!(messages.len(), 1);
        assert!(
            matches!(&messages[0], Message::Edit(Action::Edit(Edit::Paste(text))) if text.as_str() == "日本語")
        );
        assert!(
            tree.state
                .downcast_ref::<State>()
                .preedit
                .as_ref()
                .unwrap()
                .content
                .is_empty()
        );
    }
}
