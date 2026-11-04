//! Spreadsheet selection and a single transient, native in-cell text editor.
//!
//! A draft never mutates Markdown. Commit checks its stable source target and
//! enters the normal document transaction path once; cancel has no history.

use std::sync::Arc;

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{operation, text_editor};
use iced::{Element, Task};
use yrs::Assoc;

use super::{App, EDITOR, EditorMode, Message as AppMessage};
use crate::document::collaboration::StablePosition;
use crate::document::selection::{Selection, TableSelection};
use crate::document::table_edit;
use crate::document::writing::Target;
use crate::editor::native_buffer;
use crate::io::table_clipboard::{Producer, Snapshot};
use crate::theme::styles;

pub(super) const CELL_EDITOR: &str = "table-cell-editor";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extent {
    Cell,
    Row,
    Column,
    Table,
}

#[derive(Debug, Clone)]
pub enum Message {
    Select {
        revision: Arc<()>,
        table: usize,
        row: usize,
        column: usize,
        extend: bool,
        extent: Extent,
    },
    Begin(Option<String>),
    Draft(text_editor::Action),
    Commit(Option<(i32, i32)>),
    Cancel,
    ExitBoundary {
        revision: Arc<()>,
        table: usize,
        row: usize,
        column: usize,
        up: bool,
    },
    Move {
        rows: i32,
        columns: i32,
        extend: bool,
    },
    Edge {
        rows: i32,
        columns: i32,
        extend: bool,
    },
    Boundary {
        end: bool,
        whole: bool,
        extend: bool,
    },
    Tab(bool),
    Clear,
    SelectExtent(Extent),
    DraftUndo(bool),
    PasteSelection,
    PasteTextReady {
        revision: Arc<()>,
        view: View,
        text: Option<String>,
    },
    EmptyPasteReady {
        revision: Arc<()>,
        view: View,
        result: Result<Option<Snapshot>, String>,
    },
    Paste(String),
    PasteReady {
        revision: Arc<()>,
        view: View,
        text: String,
        producer: Producer,
        values: Option<Vec<Vec<String>>>,
    },
    CopySelection {
        cut: bool,
    },
    CopyTextReady {
        revision: Arc<()>,
        view: View,
        text: String,
        values: Vec<Vec<String>>,
        cut: bool,
        copied: Option<String>,
    },
    CopyReady {
        revision: Arc<()>,
        view: View,
        cut: bool,
        result: Result<(), String>,
    },
    Cut {
        revision: Arc<()>,
        view: View,
    },
    Copied,
    ClipboardError(String),
}

/// Indices are derived from stable source positions after source revisions.
/// The explicit column disambiguates implicit empty cells in short source rows.
struct Cell {
    position: StablePosition,
    row: usize,
    column: usize,
}

struct Grid {
    table: StablePosition,
    anchor: Cell,
    head: Cell,
    active: Cell,
    revision: Arc<()>,
    tab_column: Option<usize>,
}

pub(super) struct Draft {
    target: Target,
    original: String,
    pub buffer: native_buffer::Buffer,
    undo: Vec<DraftEdit>,
    redo: Vec<DraftEdit>,
}

struct DraftEdit {
    start: usize,
    old: String,
    new: String,
    before: Selection,
    after: Selection,
}

#[derive(Default)]
pub(super) struct State {
    grid: Option<Grid>,
    /// Keep the source target for re-entry while hiding editing presentation.
    quiet: bool,
    pub draft: Option<Draft>,
}

impl State {
    pub(super) fn has_selection(&self) -> bool {
        self.grid.is_some() && !self.quiet
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    pub table: usize,
    pub anchor: (usize, usize),
    pub head: (usize, usize),
    pub active: (usize, usize),
    pub editing: bool,
}

impl View {
    pub fn contains(self, row: usize, column: usize) -> bool {
        (self.anchor.0.min(self.head.0)..=self.anchor.0.max(self.head.0)).contains(&row)
            && (self.anchor.1.min(self.head.1)..=self.anchor.1.max(self.head.1)).contains(&column)
    }
}

/// Count at most the two physical line endings needed for a blank separator.
/// Native source editing treats CRLF and LFCR as paired endings.
fn boundary_endings(text: &str) -> usize {
    let mut bytes = text.bytes().peekable();
    let mut count = 0;
    while let Some(byte) = bytes.next() {
        if matches!(byte, b'\r' | b'\n') {
            count += 1;
            if bytes
                .peek()
                .is_some_and(|next| matches!((byte, *next), (b'\r', b'\n') | (b'\n', b'\r')))
            {
                bytes.next();
            }
            if count == 2 {
                break;
            }
        }
    }
    count
}

impl App {
    pub(super) fn grid_view(&self) -> Option<View> {
        if matches!(self.overlay, Some(super::Overlay::Table)) && self.table.is_menu() {
            return self.table_menu_grid();
        }
        self.grid_position().filter(|_| !self.grid.quiet)
    }

    fn grid_position(&self) -> Option<View> {
        if self.mode != EditorMode::Live || self.overlay.is_some() {
            return None;
        }
        let grid = self.grid.grid.as_ref()?;
        let position = self.document.collaboration.resolve(&grid.table)?;
        let table_index = self.projection.rich.tables.iter().position(|table| {
            table.source_range.start <= position && position <= table.source_range.end
        })?;
        let table = &self.projection.rich.tables[table_index];
        let resolve = |cell: &Cell| {
            let point = self.document.collaboration.resolve(&cell.position)?;
            if Arc::ptr_eq(&grid.revision, &self.projection.source_id) {
                return table
                    .source_rows
                    .get(cell.row)?
                    .get(cell.column)
                    .map(|_| (cell.row, cell.column));
            }
            // Prefer the old column when several implicit empty cells share a
            // source position; otherwise follow the surviving cell's source.
            table
                .source_rows
                .iter()
                .enumerate()
                .find_map(|(row, cells)| {
                    cells
                        .get(cell.column)
                        .filter(|range| range.start <= point && point <= range.end)
                        .map(|_| (row, cell.column))
                })
                .or_else(|| {
                    table
                        .source_rows
                        .iter()
                        .enumerate()
                        .find_map(|(row, cells)| {
                            cells
                                .iter()
                                .position(|range| range.start <= point && point <= range.end)
                                .map(|column| (row, column))
                        })
                })
        };
        Some(View {
            table: table_index,
            anchor: resolve(&grid.anchor)?,
            head: resolve(&grid.head)?,
            active: resolve(&grid.active)?,
            editing: self.grid.draft.is_some(),
        })
    }

    pub(super) fn grid_resume(&self) -> Option<View> {
        if !self.grid.quiet || !self.selection.is_caret() {
            return None;
        }
        let view = self.grid_position()?;
        let cell =
            &self.projection.rich.tables[view.table].source_rows[view.active.0][view.active.1];
        (cell.start <= self.selection.head && self.selection.head <= cell.end).then_some(view)
    }

    fn set_grid(
        &mut self,
        table_index: usize,
        anchor: (usize, usize),
        head: (usize, usize),
        tab_column: Option<usize>,
    ) {
        self.set_grid_active(table_index, anchor, head, anchor, tab_column);
    }

    fn set_grid_active(
        &mut self,
        table_index: usize,
        anchor: (usize, usize),
        head: (usize, usize),
        active: (usize, usize),
        tab_column: Option<usize>,
    ) {
        let table = &self.projection.rich.tables[table_index];
        let collaboration = &self.document.collaboration;
        let cell = |(row, column): (usize, usize)| Cell {
            position: collaboration.position(table.source_rows[row][column].start, Assoc::After),
            row,
            column,
        };
        let a = &table.source_rows[anchor.0][anchor.1];
        let h = &table.source_rows[head.0][head.1];
        self.selection = if anchor == head {
            Selection::caret(h.start)
        } else if anchor <= head {
            Selection {
                anchor: a.start,
                head: h.end,
            }
        } else {
            Selection {
                anchor: a.end,
                head: h.start,
            }
        };
        self.grid.grid = Some(Grid {
            table: collaboration.position(table.source_range.start, Assoc::Before),
            anchor: cell(anchor),
            head: cell(head),
            active: cell(active),
            revision: Arc::clone(&self.projection.source_id),
            tab_column,
        });
        self.grid.quiet = false;
        // Cell selection has no text caret affinity in concealed padding.
        self.table_editing = None;
        if active != anchor {
            self.selection = Selection::caret(table.source_rows[active.0][active.1].start);
        }
        self.editor_focused = true;
        self.document.break_history_group();
        self.sync_editor();
    }

    pub(super) fn clear_grid(&mut self) {
        self.grid = State::default();
    }

    pub(super) fn restore_grid(&mut self, view: View) {
        let selection = self.selection;
        if let Some(table) = self.projection.rich.tables.get(view.table) {
            let clamp = |(r, c): (usize, usize)| {
                (
                    r.min(table.rows.len() - 1),
                    c.min(table.alignment.len() - 1),
                )
            };
            self.set_grid_active(
                view.table,
                clamp(view.anchor),
                clamp(view.head),
                clamp(view.active),
                None,
            );
            self.selection = selection;
            self.sync_editor();
        } else {
            self.clear_grid();
        }
    }

    /// Keep the clipboard rectangle with the source transaction so Undo/Redo
    /// can restore its shape even when paste grows the table.
    fn apply_clipboard_plan(
        &mut self,
        edit: table_edit::TableEdit,
        before: View,
        after: View,
    ) -> bool {
        if !edit.edits.is_empty() {
            let source = self.projection.rich.tables[before.table].source_range.start;
            let target = |view: View| TableSelection {
                source,
                anchor: view.anchor,
                head: view.head,
                active: view.active,
            };
            match self.document.transact_table(
                &edit.edits,
                crate::document::collaboration::EditOrigin::Human,
                Some((self.selection, edit.selection)),
                (target(before), target(after)),
                true,
            ) {
                Ok(()) => {
                    self.selection = edit.selection;
                    self.sync_editor();
                    self.changed();
                }
                Err(error) => {
                    self.notification = Some(error.to_string());
                    return false;
                }
            }
        }
        self.set_grid_active(after.table, after.anchor, after.head, after.active, None);
        true
    }

    pub(super) fn restore_clipboard_selection(&mut self, selection: TableSelection) {
        if let Some(table) = self
            .projection
            .rich
            .tables
            .iter()
            .position(|table| table.source_range.start == selection.source)
        {
            let target = &self.projection.rich.tables[table];
            let clamp = |(row, column): (usize, usize)| {
                (
                    row.min(target.rows.len() - 1),
                    column.min(target.alignment.len() - 1),
                )
            };
            self.set_grid_active(
                table,
                clamp(selection.anchor),
                clamp(selection.head),
                clamp(selection.active),
                None,
            );
        } else {
            self.clear_grid();
        }
    }

    pub(super) fn update_grid(&mut self, message: Message) -> Task<AppMessage> {
        if self.loading || self.mode != EditorMode::Live || self.overlay.is_some() {
            return Task::none();
        }
        match message {
            Message::CopySelection { cut } => {
                let Some(view) = self.grid_view().filter(|view| !view.editing) else {
                    return Task::none();
                };
                let count = (view.anchor.0.abs_diff(view.head.0) + 1)
                    .checked_mul(view.anchor.1.abs_diff(view.head.1) + 1);
                if count.is_none_or(|count| {
                    count > crate::document::collaboration::MAX_TRANSACTION_EDITS
                }) {
                    return self.update_grid(Message::ClipboardError(
                        "Copy up to 65,536 cells at a time.".into(),
                    ));
                }
                let table = &self.projection.rich.tables[view.table];
                let values = (view.anchor.0.min(view.head.0)..=view.anchor.0.max(view.head.0))
                    .map(|row| {
                        (view.anchor.1.min(view.head.1)..=view.anchor.1.max(view.head.1))
                            .map(|column| {
                                self.projection.text[table.rows[row][column].clone()].to_owned()
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let text = crate::document::table_clipboard::encode(&values);
                if text.len() > crate::document::collaboration::MAX_SOURCE_BYTES {
                    return self.update_grid(Message::ClipboardError(
                        "The clipboard supports up to 16 MiB of text.".into(),
                    ));
                }
                let revision = Arc::clone(&self.projection.source_id);
                return iced::clipboard::write(text.clone()).chain(iced::clipboard::read().map(
                    move |copied| {
                        AppMessage::TableGrid(Message::CopyTextReady {
                            revision: Arc::clone(&revision),
                            view,
                            text: text.clone(),
                            values: values.clone(),
                            cut,
                            copied,
                        })
                    },
                ));
            }
            Message::CopyTextReady {
                revision,
                view,
                text,
                values,
                cut,
                copied,
            } => {
                if copied.as_deref() != Some(text.as_str()) {
                    return self.update_grid(Message::ClipboardError(
                        "Couldn't copy to the clipboard. Your cells were kept.".into(),
                    ));
                }
                let Some(clipboard) = self.native_table_clipboard.clone() else {
                    return self.update_grid(if cut {
                        Message::Cut { revision, view }
                    } else {
                        Message::Copied
                    });
                };
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let html = crate::document::table_clipboard::encode_html(&values)?;
                            clipboard.write(&text, &html)
                        })
                        .await
                        .map_err(|error| error.to_string())?
                    },
                    move |result| {
                        AppMessage::TableGrid(Message::CopyReady {
                            revision: Arc::clone(&revision),
                            view,
                            cut,
                            result,
                        })
                    },
                );
            }
            Message::CopyReady {
                revision,
                view,
                cut,
                result,
            } => {
                return self.update_grid(match result {
                    Ok(()) if cut => Message::Cut { revision, view },
                    Ok(()) => Message::Copied,
                    Err(error) => Message::ClipboardError(error),
                });
            }
            Message::Copied => {
                self.notification = None;
                return Task::none();
            }
            Message::ClipboardError(error) => {
                self.notification = Some(error);
                return Task::none();
            }
            Message::Cut { revision, view } => {
                if !Arc::ptr_eq(&revision, &self.projection.source_id)
                    || self.grid_view() != Some(view)
                {
                    self.notification =
                        Some("The table changed. Copy the selection again before cutting.".into());
                    return Task::none();
                }
                return self.update_grid(Message::Clear);
            }
            Message::ExitBoundary {
                revision,
                table,
                row,
                column,
                up,
            } => {
                if !Arc::ptr_eq(&revision, &self.projection.source_id) || self.grid.draft.is_some()
                {
                    return Task::none();
                }
                let Some(target) = self.projection.rich.tables.get(table) else {
                    return Task::none();
                };
                if row != if up { 0 } else { target.rows.len() - 1 }
                    || target
                        .source_rows
                        .get(row)
                        .and_then(|cells| cells.get(column))
                        .is_none()
                {
                    return Task::none();
                }
                let source = self.document.source();
                let edge = if up {
                    source[..target.source_range.start]
                        .rfind(['\r', '\n'])
                        .map_or(0, |index| index + 1)
                } else {
                    target.source_range.end
                };
                let outside = if up { 0..edge } else { edge..source.len() };
                // Only whitespace and empty quote rails may remain between this
                // table/container and the document edge. Other blocks keep their
                // ordinary measured navigation path.
                if source[outside.clone()]
                    .char_indices()
                    .any(|(index, character)| {
                        !character.is_whitespace()
                            && !self.projection.is_quote_marker(outside.start + index)
                    })
                {
                    return Task::none();
                }
                let table_source = &source[target.source_range.clone()];
                let ending = table_source.find(['\r', '\n']).map_or("\n", |index| {
                    if table_source[index..].starts_with("\r\n") {
                        "\r\n"
                    } else if table_source.as_bytes()[index] == b'\r' {
                        "\r"
                    } else {
                        "\n"
                    }
                });
                let whitespace = if up {
                    &source[..source.len() - source.trim_start_matches(char::is_whitespace).len()]
                } else {
                    &source[source.trim_end_matches(char::is_whitespace).len()..]
                };
                let text = ending.repeat(2 - boundary_endings(whitespace));
                let at = if up { 0 } else { source.len() };
                let selection = Selection::caret(if up { 0 } else { at + text.len() });
                let applied = if text.is_empty() {
                    self.selection = selection;
                    true
                } else {
                    self.apply_table_plan(
                        table_edit::TableEdit {
                            edits: vec![crate::document::collaboration::SourceEdit {
                                range: at..at,
                                text,
                            }],
                            selection,
                        },
                        true,
                    )
                };
                if applied {
                    self.clear_grid();
                    self.table_editing = None;
                    self.editor_focused = true;
                    self.sync_editor();
                    return operation::focus(EDITOR);
                }
                return Task::none();
            }
            Message::Select {
                revision,
                table,
                row,
                column,
                extend,
                extent,
            } => {
                if !Arc::ptr_eq(&revision, &self.projection.source_id) {
                    return Task::none();
                }
                if self
                    .projection
                    .rich
                    .tables
                    .get(table)
                    .and_then(|t| t.rows.get(row))
                    .and_then(|r| r.get(column))
                    .is_none()
                {
                    return Task::none();
                }
                let old = self.grid_view().or_else(|| self.grid_resume());
                if !self.commit_cell() {
                    return Task::none();
                }
                let t = &self.projection.rich.tables[table];
                let mut anchor = if extend {
                    old.filter(|v| v.table == table)
                        .map_or((row, column), |v| v.anchor)
                } else {
                    (row, column)
                };
                let mut head = (row, column);
                match extent {
                    Extent::Row => {
                        anchor.1 = 0;
                        head.1 = t.alignment.len() - 1;
                    }
                    Extent::Column => {
                        anchor.0 = 0;
                        head.0 = t.rows.len() - 1;
                    }
                    Extent::Table => {
                        anchor = (0, 0);
                        head = (t.rows.len() - 1, t.alignment.len() - 1);
                    }
                    Extent::Cell => {}
                }
                let active = if extend {
                    old.filter(|v| v.table == table)
                        .map_or(anchor, |v| v.active)
                } else {
                    (row, column)
                };
                self.set_grid_active(table, anchor, head, active, None);
                if extent != Extent::Cell {
                    self.focus_grid_active();
                }
            }
            Message::Begin(initial) => {
                let Some(view) = self.grid_view() else {
                    return Task::none();
                };
                if self.grid.draft.is_some() {
                    return Task::none();
                }
                self.focus_grid_active();
                let t = &self.projection.rich.tables[view.table];
                let (row, column) = view.active;
                let source_range = t.source_rows[row][column].clone();
                let original = self.projection.text[t.rows[row][column].clone()].to_owned();
                let text = initial
                    .map(|text| text.replace("\r\n", "\n").replace('\r', "\n"))
                    .unwrap_or_else(|| original.clone());
                if text.len() > crate::io::storage::MAX_FILE_BYTES as usize {
                    self.notification =
                        Some("The editor supports cell drafts up to 16 MiB.".into());
                    return Task::none();
                }
                let mut buffer = native_buffer::Buffer::new(&text);
                buffer.select(Selection::caret(text.len()));
                let undo = if original != text && original.len() + text.len() <= 2 * 1024 * 1024 {
                    vec![DraftEdit {
                        start: 0,
                        old: original.clone(),
                        new: text.clone(),
                        before: Selection::caret(original.len()),
                        after: Selection::caret(text.len()),
                    }]
                } else {
                    Vec::new()
                };
                self.grid.draft = Some(Draft {
                    target: Target::new(
                        &self.document.collaboration,
                        self.document.source(),
                        source_range,
                    ),
                    original,
                    buffer,
                    undo,
                    redo: Vec::new(),
                });
                return operation::focus(CELL_EDITOR);
            }
            Message::Draft(action) => {
                // A cell newline is always a logical LF. Physical Markdown
                // row endings remain untouched by the commit serializer.
                let action = match action {
                    text_editor::Action::Edit(text_editor::Edit::Paste(text))
                        if text.contains('\r') =>
                    {
                        text_editor::Action::Edit(text_editor::Edit::Paste(Arc::new(
                            text.replace("\r\n", "\n").replace('\r', "\n"),
                        )))
                    }
                    action => action,
                };
                if let Some(draft) = &mut self.grid.draft {
                    let before = draft.buffer.selection();
                    if let text_editor::Action::Edit(text_editor::Edit::Paste(text)) = &action
                        && draft.buffer.snapshot().len() - before.range().len() + text.len()
                            > crate::io::storage::MAX_FILE_BYTES as usize
                    {
                        self.notification =
                            Some("The editor supports cell drafts up to 16 MiB.".into());
                        return Task::none();
                    }
                    let old = action.is_edit().then(|| draft.buffer.snapshot().to_owned());
                    draft.buffer.perform(action);
                    if let Some(old) = old {
                        if draft.buffer.snapshot().len()
                            > crate::io::storage::MAX_FILE_BYTES as usize
                        {
                            draft.buffer = native_buffer::Buffer::new(&old);
                            draft.buffer.select(before);
                            self.notification =
                                Some("The editor supports cell drafts up to 16 MiB.".into());
                            return Task::none();
                        }
                        let text = draft.buffer.snapshot();
                        if old != text {
                            let diff = crate::document::projection::difference(&old, text);
                            draft.undo.push(DraftEdit {
                                start: diff.old.start,
                                old: old[diff.old].to_owned(),
                                new: text[diff.new].to_owned(),
                                before,
                                after: draft.buffer.selection(),
                            });
                            // Bound local history independently of source history.
                            if draft.undo.len() > 256 {
                                draft.undo.remove(0);
                            }
                            while draft
                                .undo
                                .iter()
                                .map(|edit| edit.old.len() + edit.new.len())
                                .sum::<usize>()
                                > 2 * 1024 * 1024
                            {
                                draft.undo.remove(0);
                            }
                            draft.redo.clear();
                        }
                    }
                }
                return Task::none();
            }
            Message::DraftUndo(redo) => {
                if let Some(draft) = &mut self.grid.draft {
                    let edit = if redo {
                        draft.redo.pop()
                    } else {
                        draft.undo.pop()
                    };
                    if let Some(edit) = edit {
                        if redo {
                            draft
                                .buffer
                                .replace(edit.start..edit.start + edit.old.len(), &edit.new);
                            draft.buffer.select(edit.after);
                            draft.undo.push(edit);
                        } else {
                            draft
                                .buffer
                                .replace(edit.start..edit.start + edit.new.len(), &edit.old);
                            draft.buffer.select(edit.before);
                            draft.redo.push(edit);
                        }
                    }
                }
                return Task::none();
            }
            Message::Cancel => {
                let editing = self.grid.draft.is_some();
                self.grid.draft = None;
                if let Some(view) = self.grid_view() {
                    if editing {
                        self.set_grid_active(view.table, view.anchor, view.head, view.active, None);
                        self.focus_grid_active();
                    } else if view.anchor != view.head {
                        self.set_grid(view.table, view.active, view.active, None);
                    } else {
                        self.focus_grid_active();
                        self.grid.quiet = true;
                        self.sync_editor();
                    }
                } else {
                    self.clear_grid();
                    self.sync_editor();
                }
            }
            Message::Commit(movement) => {
                if !self.commit_cell() {
                    return Task::none();
                }
                if let Some((rows, columns)) = movement {
                    if self.cycle_grid(rows, columns) {
                        return operation::focus(EDITOR);
                    }
                    if rows > 0
                        && let Some(view) = self.grid_view()
                    {
                        let table = &self.projection.rich.tables[view.table];
                        if view.active.0 + 1 == table.rows.len() {
                            let plan = table_edit::command_at(
                                self.document.source(),
                                table,
                                view.active.0,
                                view.active.1,
                                self.selection,
                                table_edit::Command::RowAfter,
                            );
                            if let Ok(plan) = plan {
                                self.apply_table_plan(plan, true);
                            }
                        }
                    }
                    let columns = if rows != 0 {
                        self.grid
                            .grid
                            .as_ref()
                            .and_then(|g| g.tab_column)
                            .zip(self.grid_view())
                            .map_or(columns, |(column, view)| {
                                column as i32 - view.active.1 as i32
                            })
                    } else {
                        columns
                    };
                    return self.update_grid(Message::Move {
                        rows,
                        columns,
                        extend: false,
                    });
                }
            }
            Message::Move {
                rows,
                columns,
                extend,
            } => {
                if !self.commit_cell() {
                    return Task::none();
                }
                let Some(view) = self.grid_view() else {
                    return Task::none();
                };
                let table = &self.projection.rich.tables[view.table];
                let from = if extend { view.head } else { view.active };
                let head = (
                    from.0
                        .saturating_add_signed(rows as isize)
                        .min(table.rows.len() - 1),
                    from.1
                        .saturating_add_signed(columns as isize)
                        .min(table.alignment.len() - 1),
                );
                self.set_grid(
                    view.table,
                    if extend { view.anchor } else { head },
                    head,
                    None,
                );
            }
            Message::Boundary { end, whole, extend } => {
                let Some(view) = self.grid_view() else {
                    return Task::none();
                };
                let table = &self.projection.rich.tables[view.table];
                let row = if whole {
                    if end { table.rows.len() - 1 } else { 0 }
                } else {
                    view.active.0
                };
                let head = (row, if end { table.alignment.len() - 1 } else { 0 });
                self.set_grid(
                    view.table,
                    if extend { view.anchor } else { head },
                    head,
                    None,
                );
            }
            Message::Edge {
                rows,
                columns,
                extend,
            } => {
                let Some(view) = self.grid_view() else {
                    return Task::none();
                };
                let table = &self.projection.rich.tables[view.table];
                let from = if extend { view.head } else { view.active };
                let limit = if rows != 0 {
                    table.rows.len()
                } else {
                    table.alignment.len()
                };
                let direction = if rows < 0 || columns < 0 { -1 } else { 1 };
                let at = |index: usize| {
                    if rows != 0 {
                        (index, from.1)
                    } else {
                        (from.0, index)
                    }
                };
                let populated = |index: usize| {
                    let (r, c) = at(index);
                    !table.rows[r][c].is_empty()
                };
                let mut index = if rows != 0 { from.0 } else { from.1 };
                let next = index.saturating_add_signed(direction).min(limit - 1);
                let contiguous = populated(index) && populated(next);
                while index.saturating_add_signed(direction).min(limit - 1) != index {
                    let next = index.saturating_add_signed(direction).min(limit - 1);
                    if contiguous && !populated(next) {
                        break;
                    }
                    index = next;
                    if !contiguous && populated(index) {
                        break;
                    }
                }
                let head = at(index);
                self.set_grid(
                    view.table,
                    if extend { view.anchor } else { head },
                    head,
                    None,
                );
            }
            Message::Tab(backward) => {
                if !self.commit_cell() {
                    return Task::none();
                }
                let Some(view) = self.grid_view() else {
                    return Task::none();
                };
                if self.cycle_grid(0, if backward { -1 } else { 1 }) {
                    return operation::focus(EDITOR);
                }
                let table = &self.projection.rich.tables[view.table];
                let columns = table.alignment.len();
                let start = self
                    .grid
                    .grid
                    .as_ref()
                    .and_then(|g| g.tab_column)
                    .unwrap_or(view.anchor.1);
                let index = view.active.0 * columns + view.active.1;
                let index = if backward {
                    index.saturating_sub(1)
                } else {
                    index + 1
                };
                if index == table.rows.len() * columns {
                    let plan = table_edit::command_at(
                        self.document.source(),
                        table,
                        view.active.0,
                        view.active.1,
                        self.selection,
                        table_edit::Command::RowAfter,
                    );
                    if let Ok(plan) = plan {
                        if !self.apply_table_plan(plan, true) {
                            return Task::none();
                        }
                    } else {
                        return Task::none();
                    }
                }
                let head = (index / columns, index % columns);
                self.set_grid(view.table, head, head, Some(start));
            }
            Message::Clear => {
                let Some(view) = self.grid_view() else {
                    return Task::none();
                };
                if !self.commit_cell() {
                    return Task::none();
                }
                if let Some(edit) = table_edit::clear_rectangle(
                    &self.projection,
                    self.document.source(),
                    view.table,
                    view.anchor,
                    view.head,
                ) {
                    self.table_editing = None;
                    if self.apply_clipboard_plan(edit, view, view) {
                        self.notification = None;
                    }
                }
            }
            Message::SelectExtent(extent) => {
                let Some(view) = self.grid_view() else {
                    return Task::none();
                };
                return self.update_grid(Message::Select {
                    revision: Arc::clone(&self.projection.source_id),
                    table: view.table,
                    row: view.active.0,
                    column: view.active.1,
                    extend: false,
                    extent,
                });
            }
            Message::PasteSelection => {
                let Some(view) = self.grid_view().filter(|view| !view.editing) else {
                    return Task::none();
                };
                let revision = Arc::clone(&self.projection.source_id);
                return iced::clipboard::read().map(move |text| {
                    AppMessage::TableGrid(Message::PasteTextReady {
                        revision: Arc::clone(&revision),
                        view,
                        text,
                    })
                });
            }
            Message::PasteTextReady {
                revision,
                view,
                text,
            } => {
                if !Arc::ptr_eq(&revision, &self.projection.source_id)
                    || self.grid_view() != Some(view)
                {
                    return self.update_grid(Message::ClipboardError(
                        "The table selection changed. Paste again at the intended cell.".into(),
                    ));
                }
                if let Some(text) = text {
                    return self.update_grid(Message::Paste(text));
                }
                if let Some(clipboard) = self.native_table_clipboard.clone() {
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || clipboard.read_empty())
                                .await
                                .map_err(|error| error.to_string())?
                        },
                        move |result| {
                            AppMessage::TableGrid(Message::EmptyPasteReady {
                                revision: Arc::clone(&revision),
                                view,
                                result,
                            })
                        },
                    );
                }
            }
            Message::EmptyPasteReady {
                revision,
                view,
                result,
            } => {
                if !Arc::ptr_eq(&revision, &self.projection.source_id)
                    || self.grid_view() != Some(view)
                {
                    return self.update_grid(Message::ClipboardError(
                        "The table selection changed. Paste again at the intended cell.".into(),
                    ));
                }
                match result {
                    Ok(Some(Snapshot {
                        producer,
                        values: Some(values),
                    })) if values.iter().flatten().all(String::is_empty) => {
                        return self.update_grid(Message::PasteReady {
                            revision,
                            view,
                            text: String::new(),
                            producer,
                            values: Some(values),
                        });
                    }
                    Err(error) => return self.update_grid(Message::ClipboardError(error)),
                    _ => {}
                }
            }
            Message::Paste(text) => {
                let Some(view) = self.grid_view() else {
                    return Task::none();
                };
                let revision = Arc::clone(&self.projection.source_id);
                let Some(clipboard) = self.native_table_clipboard.clone() else {
                    return self.update_grid(Message::PasteReady {
                        revision,
                        view,
                        text,
                        producer: Producer::Standard,
                        values: None,
                    });
                };
                if text.len() > crate::document::collaboration::MAX_SOURCE_BYTES {
                    return self.update_grid(Message::ClipboardError(
                        "The clipboard supports up to 16 MiB of text.".into(),
                    ));
                }
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let snapshot = clipboard.read(&text)?;
                            Ok::<_, String>((text, snapshot))
                        })
                        .await
                        .map_err(|error| error.to_string())?
                    },
                    move |result| match result {
                        Ok((text, snapshot)) => AppMessage::TableGrid(Message::PasteReady {
                            revision: Arc::clone(&revision),
                            view,
                            text,
                            producer: snapshot.producer,
                            values: snapshot.values,
                        }),
                        Err(error) => {
                            AppMessage::TableGrid(Message::ClipboardError(error.to_string()))
                        }
                    },
                );
            }
            Message::PasteReady {
                revision,
                view: expected,
                text,
                producer,
                values: native_values,
            } => {
                if !Arc::ptr_eq(&revision, &self.projection.source_id)
                    || self.grid_view() != Some(expected)
                {
                    return self.update_grid(Message::ClipboardError(
                        "The table selection changed. Paste again at the intended cell.".into(),
                    ));
                }
                let decoded = match native_values {
                    Some(values) => Ok(values),
                    None => match producer {
                        Producer::Standard | Producer::Excel => {
                            crate::document::table_clipboard::decode(&text)
                        }
                        Producer::Sheets => crate::document::table_clipboard::decode_sheets(&text),
                    },
                };
                let values = match decoded {
                    Ok(values) => values,
                    Err(error) => {
                        self.notification = Some(error.into());
                        return Task::none();
                    }
                };
                if !self.commit_cell() {
                    return Task::none();
                }
                let Some(view) = self.grid_view() else {
                    return Task::none();
                };
                let range = view.anchor != view.head;
                let start = if range {
                    (
                        view.anchor.0.min(view.head.0),
                        view.anchor.1.min(view.head.1),
                    )
                } else {
                    view.active
                };
                let (input_rows, input_columns) = (values.len(), values[0].len());
                let (rows, columns) = if range {
                    (
                        view.anchor.0.abs_diff(view.head.0) + 1,
                        view.anchor.1.abs_diff(view.head.1) + 1,
                    )
                } else {
                    (input_rows, input_columns)
                };
                if rows % input_rows != 0 || columns % input_columns != 0 {
                    self.notification = Some("Select one starting cell, or a range that is a multiple of the copied block.".into());
                    return Task::none();
                }
                let copies = (rows / input_rows).checked_mul(columns / input_columns);
                let bytes = values
                    .iter()
                    .flatten()
                    .try_fold(0usize, |sum, value| sum.checked_add(value.len()));
                if rows.checked_mul(columns).is_none_or(|count| {
                    count > crate::document::collaboration::MAX_TRANSACTION_EDITS
                }) || copies
                    .zip(bytes)
                    .and_then(|(copies, bytes)| copies.checked_mul(bytes))
                    .is_none_or(|bytes| bytes > crate::document::collaboration::MAX_SOURCE_BYTES)
                {
                    self.notification =
                        Some("This paste exceeds the document or cell limit.".into());
                    return Task::none();
                }
                let values = if rows == input_rows && columns == input_columns {
                    values
                } else {
                    (0..rows)
                        .map(|row| {
                            (0..columns)
                                .map(|column| {
                                    values[row % input_rows][column % input_columns].clone()
                                })
                                .collect()
                        })
                        .collect()
                };
                let edit = match table_edit::paste_rectangle(
                    &self.projection,
                    self.document.source(),
                    view.table,
                    start,
                    &values,
                ) {
                    Ok(edit) => edit,
                    Err(error) => {
                        self.notification = Some(error.into());
                        return Task::none();
                    }
                };
                let end = (start.0 + rows - 1, start.1 + columns - 1);
                let after = View {
                    table: view.table,
                    anchor: if range { view.anchor } else { start },
                    head: if range { view.head } else { end },
                    active: if range { view.active } else { start },
                    editing: false,
                };
                if self.apply_clipboard_plan(edit, view, after) {
                    self.notification = None;
                }
            }
        }
        operation::focus(EDITOR)
    }

    /// Tab walks across a selected rectangle; Enter walks down it. The active
    /// cell moves without throwing away the range or its anchor/head direction.
    fn cycle_grid(&mut self, rows: i32, columns: i32) -> bool {
        let Some(view) = self.grid_view().filter(|v| v.anchor != v.head) else {
            return false;
        };
        let r0 = view.anchor.0.min(view.head.0);
        let c0 = view.anchor.1.min(view.head.1);
        let height = view.anchor.0.abs_diff(view.head.0) + 1;
        let width = view.anchor.1.abs_diff(view.head.1) + 1;
        let index = if columns != 0 {
            (view.active.0 - r0) * width + view.active.1 - c0
        } else {
            (view.active.1 - c0) * height + view.active.0 - r0
        };
        let delta = if rows < 0 || columns < 0 { -1 } else { 1 };
        let index = (index as isize + delta).rem_euclid((height * width) as isize) as usize;
        let active = if columns != 0 {
            (r0 + index / width, c0 + index % width)
        } else {
            (r0 + index % height, c0 + index / height)
        };
        self.set_grid_active(view.table, view.anchor, view.head, active, None);
        true
    }

    fn focus_grid_active(&mut self) {
        if let Some(view) = self.grid_view() {
            self.selection = Selection::caret(
                self.projection.rich.tables[view.table].source_rows[view.active.0][view.active.1]
                    .start,
            );
            self.sync_editor();
        }
    }

    /// Failure keeps the draft available, so a concurrent edit never loses
    /// either replica's content and the user can still copy or cancel the draft.
    pub(super) fn commit_cell(&mut self) -> bool {
        let Some(draft) = self.grid.draft.as_ref() else {
            return true;
        };
        let range = match draft
            .target
            .resolve(&self.document.collaboration, self.document.source())
        {
            Ok(range) => range,
            Err(_) => {
                self.notification=Some("This cell changed while you were editing. Copy your draft or press Escape to keep the newer value.".into());
                return false;
            }
        };
        let Some(view) = self.grid_view() else {
            return false;
        };
        let text = draft.buffer.snapshot();
        if text == draft.original {
            self.grid.draft = None;
            return true;
        }
        if self.projection.rich.tables[view.table].source_rows[view.active.0][view.active.1]
            != range
        {
            return false;
        }
        let plan = table_edit::commit_cell(
            &self.projection,
            self.document.source(),
            view.table,
            view.active,
            text,
        );
        let Some(plan) = plan else {
            return false;
        };
        if !self.apply_table_plan(plan, true) {
            return false;
        }
        self.grid.draft = None;
        self.set_grid_active(
            view.table,
            view.anchor,
            view.head,
            view.active,
            self.grid.grid.as_ref().and_then(|g| g.tab_column),
        );
        self.focus_grid_active();
        true
    }

    pub(super) fn cell_editor(&self) -> Option<Element<'_, AppMessage>> {
        let draft = self.grid.draft.as_ref()?;
        Some(
            text_editor(draft.buffer.content())
                .id(CELL_EDITOR)
                .on_action(|action| AppMessage::TableGrid(Message::Draft(action)))
                .key_binding(draft_binding)
                .font(styles::BODY_FONT)
                .size(self.resolved_theme.typography.body_size * self.zoom)
                .line_height(iced::widget::text::LineHeight::Relative(
                    self.resolved_theme.typography.line_height,
                ))
                .padding((self.resolved_theme.typography.body_size * self.zoom * 0.65).min(12.0))
                .height(iced::Shrink)
                .style(styles::table_editor(self.resolved_theme.tokens))
                .into(),
        )
    }
}

fn draft_binding(press: text_editor::KeyPress) -> Option<text_editor::Binding<AppMessage>> {
    if !matches!(press.status, text_editor::Status::Focused { .. }) {
        return None;
    }
    let message = match press.key.as_ref() {
        Key::Named(Named::Escape) => Some(Message::Cancel),
        Key::Named(Named::Enter) if press.modifiers.command() => {
            return Some(text_editor::Binding::Enter);
        }
        Key::Named(Named::Enter) => Some(Message::Commit(Some((
            if press.modifiers.shift() { -1 } else { 1 },
            0,
        )))),
        Key::Named(Named::Tab)
            if press.modifiers.is_empty() || press.modifiers == Modifiers::SHIFT =>
        {
            Some(Message::Tab(press.modifiers.shift()))
        }
        Key::Character(character)
            if press.modifiers.command() && character.eq_ignore_ascii_case("z") =>
        {
            Some(Message::DraftUndo(press.modifiers.shift()))
        }
        Key::Character(character)
            if press.modifiers.command() && character.eq_ignore_ascii_case("y") =>
        {
            Some(Message::DraftUndo(true))
        }
        _ => None,
    };
    message
        .map(|m| text_editor::Binding::Custom(AppMessage::TableGrid(m)))
        .or_else(|| text_editor::Binding::from_key_press(press))
}

#[cfg(test)]
#[path = "table_grid_tests.rs"]
mod tests;
