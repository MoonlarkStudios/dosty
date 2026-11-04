//! Contextual, keyboard-operated table commands with checked dialog targets.

use iced::advanced::widget;
use iced::keyboard::{self, Key, key::Named};
use iced::widget::{
    button, column, container, mouse_area, operation, row, rule, scrollable, space, text,
};
use iced::{Element, Event, Fill, Padding, Point, Size, Task, event, window};
use pulldown_cmark::Alignment;

use super::{App, EDITOR, EditorMode, Message as AppMessage, Overlay};
use crate::document::collaboration::{EditOrigin, StablePosition, StableSelection};
use crate::document::selection::Selection;
use crate::document::table_edit::{self, Command};
use crate::document::writing::Target;
use crate::theme::styles::{self, Button};

const COMMANDS: [(Command, &str); 10] = [
    (Command::RowBefore, "Insert row above"),
    (Command::RowAfter, "Insert row below"),
    (Command::DeleteRow, "Delete row"),
    (Command::ColumnBefore, "Insert column left"),
    (Command::ColumnAfter, "Insert column right"),
    (Command::DeleteColumn, "Delete column"),
    (Command::Align(Alignment::None), "Default"),
    (Command::Align(Alignment::Left), "Left"),
    (Command::Align(Alignment::Center), "Center"),
    (Command::Align(Alignment::Right), "Right"),
];

const MENU_SCROLL: &str = "table-context-menu-scroll";

fn menu_item_id(index: usize) -> widget::Id {
    widget::Id::from(format!("table-context-menu-item-{index}"))
}

/// Keep keyboard choices visible using the same measured bounds as the menu.
/// Moving the selection changes styling, so native focus scrolling cannot do it.
fn reveal_menu_item(index: usize) -> Task<AppMessage> {
    struct Locate {
        target: widget::Id,
        viewport: Option<(f32, f32, f32)>,
        item: Option<iced::Rectangle>,
    }

    impl widget::Operation<f32> for Locate {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation<f32>)) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&widget::Id>,
            bounds: iced::Rectangle,
            content: iced::Rectangle,
            translation: iced::Vector,
            _state: &mut dyn widget::operation::Scrollable,
        ) {
            if id == Some(&widget::Id::from(MENU_SCROLL)) {
                self.viewport = Some((content.y, bounds.height, translation.y));
            }
        }

        fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
            if id == Some(&self.target) {
                self.item = Some(bounds);
            }
        }

        fn finish(&self) -> widget::operation::Outcome<f32> {
            let offset =
                self.viewport
                    .zip(self.item)
                    .and_then(|((origin, height, offset), item)| {
                        let top = item.y - origin;
                        let bottom = top + item.height;
                        if top < offset {
                            Some(top.max(0.0))
                        } else if bottom > offset + height {
                            Some((bottom - height).max(0.0))
                        } else {
                            None
                        }
                    });
            offset.map_or(
                widget::operation::Outcome::None,
                widget::operation::Outcome::Some,
            )
        }
    }

    widget::operate(Locate {
        target: menu_item_id(index),
        viewport: None,
        item: None,
    })
    .then(|offset| {
        operation::scroll_to(
            MENU_SCROLL,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(offset),
            },
        )
    })
}

/// Source bounds of the active value, excluding its original Markdown padding.
/// Opposite associations retain text typed at either edge through edits/undo.
pub(super) struct EditingCell {
    start: StablePosition,
    end: StablePosition,
}

fn horizontal_space(character: char) -> bool {
    character.is_whitespace() && !matches!(character, '\r' | '\n')
}

impl App {
    pub(super) fn table_menu_grid(&self) -> Option<super::table_grid::View> {
        let form = self.table.form.as_ref()?;
        let menu = form.menu.as_ref()?;
        let range = form
            .target
            .resolve(self.document.collaboration(), self.document.source())
            .ok()?;
        let index = self
            .projection
            .rich
            .tables
            .iter()
            .position(|table| table.source_range == range)?;
        let table = &self.projection.rich.tables[index];
        let cell = (form.row, form.column);
        let (anchor, head) = match menu.scope {
            MenuScope::Row => ((form.row, 0), (form.row, table.alignment.len() - 1)),
            MenuScope::Column | MenuScope::Alignment => {
                ((0, form.column), (table.rows.len() - 1, form.column))
            }
            MenuScope::Table => ((0, 0), (table.rows.len() - 1, table.alignment.len() - 1)),
            MenuScope::Cell => menu
                .grid
                .map_or((cell, cell), |grid| (grid.anchor, grid.head)),
        };
        Some(super::table_grid::View {
            table: index,
            anchor,
            head,
            active: menu
                .grid
                .filter(|_| menu.scope == MenuScope::Cell)
                .map_or(cell, |grid| grid.active),
            editing: false,
        })
    }

    fn menu_error(&mut self, error: String) -> Task<AppMessage> {
        if let Some(form) = &mut self.table.form {
            form.error = Some(error);
        }
        Task::none()
    }

    fn apply_menu_action(&mut self, action: ContextAction) -> Task<AppMessage> {
        if !matches!(self.overlay, Some(Overlay::Table)) || !self.table.is_menu() {
            return Task::none();
        }
        let form = self.table.form.as_ref().unwrap();
        let menu = form.menu.as_ref().unwrap();
        if !menu_items(menu.scope)
            .iter()
            .any(|(item, _)| matches!(item, MenuItem::Action(candidate) if *candidate == action))
        {
            return Task::none();
        }
        // Every action, including selection and clear, checks the captured table
        // before dismissing the menu. A stale menu never retargets a nearby cell.
        let range = match form
            .target
            .resolve(self.document.collaboration(), self.document.source())
        {
            Ok(range) => range,
            Err(error) => return self.menu_error(error),
        };
        let Some(index) = self
            .projection
            .rich
            .tables
            .iter()
            .position(|table| table.source_range == range)
        else {
            return self.menu_error("The selected table is no longer available.".into());
        };
        let (row, column, scope, previous) = (form.row, form.column, menu.scope, menu.grid);
        let selection = self.selection;
        if let ContextAction::Command(command) = action {
            if let Err(error) = self.apply_table_command(command) {
                return self.menu_error(error);
            }
            self.clear_grid();
            self.table = State::default();
            self.overlay = None;
            self.editor_focused = true;
            self.sync_editor();
            if let Some((table, row, column)) =
                table_edit::cell_at(&self.projection, self.selection)
                && let Some(index) = self
                    .projection
                    .rich
                    .tables
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, table))
            {
                return self.update_grid(super::table_grid::Message::Select {
                    revision: std::sync::Arc::clone(&self.projection.source_id),
                    table: index,
                    row,
                    column,
                    extend: false,
                    extent: super::table_grid::Extent::Cell,
                });
            }
            return operation::focus(EDITOR);
        }
        self.table = State::default();
        self.overlay = None;
        if matches!(action, ContextAction::DeleteTable | ContextAction::Exit) {
            self.clear_grid();
            self.selection =
                Selection::caret(self.projection.rich.tables[index].source_rows[row][column].start);
            return self.table_context_action(Some(action));
        }
        use super::table_grid::{Extent, Message as GridMessage};
        let extent = match action {
            ContextAction::SelectRow => Extent::Row,
            ContextAction::SelectColumn => Extent::Column,
            ContextAction::SelectTable => Extent::Table,
            _ => match scope {
                MenuScope::Row => Extent::Row,
                MenuScope::Column | MenuScope::Alignment => Extent::Column,
                MenuScope::Table => Extent::Table,
                MenuScope::Cell => Extent::Cell,
            },
        };
        let task = self.update_grid(GridMessage::Select {
            revision: std::sync::Arc::clone(&self.projection.source_id),
            table: index,
            row,
            column,
            extend: false,
            extent,
        });
        if action == ContextAction::Clear {
            if extent == Extent::Cell
                && let Some(mut grid) = previous.filter(|grid| grid.contains(row, column))
            {
                grid.table = index;
                self.selection = selection;
                self.restore_grid(grid);
            }
            return self.update_grid(GridMessage::Clear);
        }
        task
    }

    pub(super) fn table_menu_view(&self, size: Size) -> Element<'_, AppMessage> {
        let Some(form) = &self.table.form else {
            return text("").into();
        };
        let Some(menu) = &form.menu else {
            return text("").into();
        };
        let tokens = self.resolved_theme.tokens;
        let title = match menu.scope {
            MenuScope::Cell => format!(
                "Cell {}{}",
                crate::editor::rich_editor::column_name(form.column),
                form.row + 1
            ),
            MenuScope::Row => format!("Row {}", form.row + 1),
            MenuScope::Column => format!(
                "Column {}",
                crate::editor::rich_editor::column_name(form.column)
            ),
            MenuScope::Alignment => format!(
                "Align column {}",
                crate::editor::rich_editor::column_name(form.column)
            ),
            MenuScope::Table => "Table".into(),
        };
        let mut content = column![text(title).size(12).color(tokens.muted)].spacing(2);
        if menu.scope != menu.root {
            let parent = if menu.scope == MenuScope::Alignment {
                MenuScope::Column
            } else {
                menu.root
            };
            content = content.push(
                button(text("‹  Back").size(12))
                    .on_press(AppMessage::Table(Message::MenuScope(parent)))
                    .padding([6, 10])
                    .width(Fill)
                    .style(styles::button_style(tokens, Button::Quiet)),
            );
        }
        for (index, &(item, label)) in menu_items(menu.scope).iter().enumerate() {
            if matches!(
                item,
                MenuItem::Action(
                    ContextAction::Command(Command::DeleteRow | Command::DeleteColumn)
                        | ContextAction::DeleteTable
                )
            ) {
                content = content.push(rule::horizontal(1).style(move |_| rule::Style {
                    color: tokens.border,
                    radius: 0.0.into(),
                    fill_mode: rule::FillMode::Full,
                    snap: true,
                }));
            }
            let message = match item {
                MenuItem::Action(action) => Message::MenuAction(action),
                MenuItem::Scope(scope) => Message::MenuScope(scope),
            };
            let label = if matches!(item, MenuItem::Action(ContextAction::Clear))
                && menu.scope == MenuScope::Cell
            {
                menu.grid
                    .map(|grid| {
                        (grid.anchor.0.abs_diff(grid.head.0) + 1)
                            * (grid.anchor.1.abs_diff(grid.head.1) + 1)
                    })
                    .filter(|count| *count > 1)
                    .map_or_else(|| label.to_owned(), |count| format!("Clear {count} cells"))
            } else {
                label.to_owned()
            };
            let checked = matches!(item, MenuItem::Action(ContextAction::Command(Command::Align(alignment))) if alignment == form.alignment);
            content = content.push(
                container(
                    mouse_area(
                        button(row![
                            text(label).size(13),
                            space::horizontal(),
                            text(if checked { "✓" } else { "" }).size(13)
                        ])
                        .on_press(AppMessage::Table(message))
                        .width(Fill)
                        .padding([8, 10])
                        .style(styles::button_style(
                            tokens,
                            if form.selected == index {
                                Button::Selected
                            } else {
                                Button::Quiet
                            },
                        )),
                    )
                    .on_enter(AppMessage::Table(Message::MenuHover(index))),
                )
                .id(menu_item_id(index))
                .width(Fill),
            );
        }
        if (menu.scope == MenuScope::Row && form.last_row)
            || (menu.scope == MenuScope::Column && form.last_column)
        {
            content = content.push(
                text("Deleting the last row or column removes the table.")
                    .size(11)
                    .color(tokens.muted),
            );
        }
        if let Some(error) = &form.error {
            content = content.push(text(error).size(12).color(tokens.text));
        }
        let width = 236.0_f32.min((size.width - 16.0).max(1.0));
        let height = (76.0
            + menu_items(menu.scope).len() as f32 * 34.0
            + if menu.scope != menu.root { 34.0 } else { 0.0 }
            + if form.error.is_some() { 80.0 } else { 0.0 })
            + if (menu.scope == MenuScope::Row && form.last_row)
                || (menu.scope == MenuScope::Column && form.last_column)
            {
                40.0
            } else {
                0.0
            };
        let height = height.min((size.height - 16.0).max(1.0));
        let position = menu_position(menu.position, size, Size::new(width, height));
        container(iced::widget::opaque(
            container(
                scrollable(content)
                    .id(MENU_SCROLL)
                    .height(iced::Shrink)
                    .style(styles::scrollbar(tokens)),
            )
            .padding(8)
            .width(width)
            .max_height(height)
            .style(styles::context_menu(tokens)),
        ))
        .padding(Padding {
            top: position.y,
            left: position.x,
            right: 0.0,
            bottom: 0.0,
        })
        .into()
    }

    pub(super) fn apply_table_input(&mut self, edit: table_edit::TableEdit, separate: bool) {
        if !separate && !self.selection.is_caret() {
            self.document.break_history_group();
        }
        let previous = self.table_editing.take();
        if let Some(first) = edit.edits.first()
            && let Some((table, row, column)) =
                table_edit::cell_at(&self.projection, Selection::caret(first.range.start))
        {
            let cell = &table.source_rows[row][column];
            self.table_editing = Some(EditingCell {
                start: self
                    .document
                    .collaboration()
                    .position(cell.start, yrs::Assoc::Before),
                end: self
                    .document
                    .collaboration()
                    .position(cell.end, yrs::Assoc::After),
            });
        }
        if !self.apply_table_plan(edit, separate) {
            self.table_editing = previous;
        }
    }
    pub(super) fn editing_cell_range(&self) -> Option<std::ops::Range<usize>> {
        if self.mode != EditorMode::Live || !self.editor_focused || self.grid.has_selection() {
            return None;
        }
        let editing = self.table_editing.as_ref()?;
        let collaboration = self.document.collaboration();
        let range = collaboration.resolve(&editing.start)?..collaboration.resolve(&editing.end)?;
        let text = self.document.source().get(range.clone())?;
        (text.is_empty()
            || text.starts_with(horizontal_space)
            || text.ends_with(horizontal_space)
            || text.contains('|'))
        .then_some(range)
    }

    pub(super) fn sync_table_editing(&mut self) {
        if self.mode != EditorMode::Live || !self.editor_focused {
            return;
        }
        let Some((table, row, column)) =
            table_edit::cell_at(&self.projection, Selection::caret(self.selection.head))
        else {
            self.table_editing = None;
            return;
        };
        let cell = &table.source_rows[row][column];
        let collaboration = self.document.collaboration();
        let source = self.document.source();
        if let Some(editing) = &self.table_editing
            && let Some(start) = collaboration.resolve(&editing.start)
            && let Some(end) = collaboration.resolve(&editing.end)
            && start <= cell.start
            && cell.end <= end
            && start <= self.selection.head
            && self.selection.head <= end
            && source[start..cell.start].chars().all(horizontal_space)
            && source[cell.end..end].chars().all(horizontal_space)
        {
            return;
        }
        let mut range = cell.clone();
        // A restored source caret can sit in whitespace trimmed by the parser.
        // Preserve that explicit position when focus or undo restores the cell.
        let start = range.start.min(self.selection.head);
        let end = range.end.max(self.selection.head);
        if source[start..range.start].chars().all(horizontal_space)
            && source[range.end..end].chars().all(horizontal_space)
        {
            range = start..end;
        }
        self.table_editing = Some(EditingCell {
            start: collaboration.position(range.start, yrs::Assoc::Before),
            end: collaboration.position(range.end, yrs::Assoc::After),
        });
    }
}

fn menu_position(point: Point, viewport: Size, menu: Size) -> Point {
    Point::new(
        point
            .x
            .clamp(8.0, (viewport.width - menu.width - 8.0).max(8.0)),
        point
            .y
            .clamp(8.0, (viewport.height - menu.height - 8.0).max(8.0)),
    )
}

#[derive(Debug, Clone)]
pub enum Message {
    Open,
    OpenMenu,
    DismissMenu,
    OpenContext {
        revision: std::sync::Arc<()>,
        table: usize,
        row: usize,
        column: usize,
        position: Point,
        extent: super::table_grid::Extent,
    },
    MenuScope(MenuScope),
    MenuHover(usize),
    MenuAction(ContextAction),
    Apply(Command),
    Key {
        key: Named,
        shift: bool,
    },
    Context(ContextAction),
    InsertTab,
    Exit,
    FocusCell {
        revision: std::sync::Arc<()>,
        table: usize,
        row: usize,
        column: usize,
        selection: Selection,
        open: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContextAction {
    Command(Command),
    SelectRow,
    SelectColumn,
    SelectTable,
    Clear,
    DeleteTable,
    Exit,
}

impl std::fmt::Display for ContextAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Command(command) => COMMANDS
                .iter()
                .find(|(action, _)| action == command)
                .map_or("Alignment", |(_, label)| *label),
            Self::SelectRow => "Select row",
            Self::SelectColumn => "Select column",
            Self::SelectTable => "Select table",
            Self::Clear => "Clear contents",
            Self::DeleteTable => "Delete table",
            Self::Exit => "Leave table",
        };
        formatter.write_str(label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuScope {
    Cell,
    Row,
    Column,
    Alignment,
    Table,
}

#[derive(Clone, Copy)]
enum MenuItem {
    Action(ContextAction),
    Scope(MenuScope),
}

fn menu_items(scope: MenuScope) -> &'static [(MenuItem, &'static str)] {
    use ContextAction as A;
    use MenuItem::{Action, Scope};
    match scope {
        MenuScope::Cell => &[
            (Scope(MenuScope::Row), "Row actions  ›"),
            (Scope(MenuScope::Column), "Column actions  ›"),
            (Action(A::Clear), "Clear contents"),
            (Scope(MenuScope::Table), "Table actions  ›"),
        ],
        MenuScope::Row => &[
            (Action(A::Command(Command::RowBefore)), "Insert row above"),
            (Action(A::Command(Command::RowAfter)), "Insert row below"),
            (Action(A::SelectRow), "Select row"),
            (Action(A::Clear), "Clear row contents"),
            (Action(A::Command(Command::DeleteRow)), "Delete row"),
        ],
        MenuScope::Column => &[
            (
                Action(A::Command(Command::ColumnBefore)),
                "Insert column left",
            ),
            (
                Action(A::Command(Command::ColumnAfter)),
                "Insert column right",
            ),
            (Action(A::SelectColumn), "Select column"),
            (Scope(MenuScope::Alignment), "Column alignment  ›"),
            (Action(A::Clear), "Clear column contents"),
            (Action(A::Command(Command::DeleteColumn)), "Delete column"),
        ],
        MenuScope::Alignment => &[
            (
                Action(A::Command(Command::Align(Alignment::None))),
                "Default",
            ),
            (Action(A::Command(Command::Align(Alignment::Left))), "Left"),
            (
                Action(A::Command(Command::Align(Alignment::Center))),
                "Center",
            ),
            (
                Action(A::Command(Command::Align(Alignment::Right))),
                "Right",
            ),
        ],
        MenuScope::Table => &[
            (Action(A::SelectTable), "Select table"),
            (Action(A::Clear), "Clear table contents"),
            (Action(A::Exit), "Leave table"),
            (Action(A::DeleteTable), "Delete table"),
        ],
    }
}

struct ContextMenu {
    position: Point,
    root: MenuScope,
    scope: MenuScope,
    grid: Option<super::table_grid::View>,
}

struct Form {
    menu: Option<ContextMenu>,
    target: Target,
    selection: StableSelection,
    selected: usize,
    row: usize,
    column: usize,
    alignment: Alignment,
    last_row: bool,
    last_column: bool,
    error: Option<String>,
}

#[derive(Default)]
pub(super) struct State {
    form: Option<Form>,
}

impl State {
    pub(super) fn is_menu(&self) -> bool {
        self.form.as_ref().is_some_and(|form| form.menu.is_some())
    }
}

// The editor underneath a modal may capture navigation keys. Listen to these
// only while the table dialog is open, without changing ordinary key routing.
pub(super) fn event_message(
    event: Event,
    status: event::Status,
    id: window::Id,
) -> Option<AppMessage> {
    if matches!(event, Event::Window(window::Event::Unfocused)) {
        return Some(AppMessage::Table(Message::DismissMenu));
    }
    if let Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Named(key),
        modifiers,
        ..
    }) = &event
        && (modifiers.is_empty() || *modifiers == keyboard::Modifiers::SHIFT && *key == Named::Tab)
        && matches!(
            key,
            Named::ArrowUp
                | Named::ArrowDown
                | Named::ArrowLeft
                | Named::ArrowRight
                | Named::Home
                | Named::End
                | Named::Tab
                | Named::Enter
                | Named::Escape
        )
    {
        return Some(AppMessage::Table(Message::Key {
            key: *key,
            shift: modifiers.shift(),
        }));
    }
    super::event_message(event, status, id)
}

impl App {
    pub(super) fn update_table(&mut self, message: Message) -> Task<AppMessage> {
        if matches!(message, Message::DismissMenu) {
            return if matches!(self.overlay, Some(Overlay::Table)) && self.table.is_menu() {
                self.update_message(AppMessage::CloseOverlay)
            } else {
                self.update_outline(super::outline::Message::CancelDrag)
            };
        }
        if self.loading || self.mode == EditorMode::Read {
            return Task::none();
        }
        // Check pointer authority before committing a draft, which itself
        // advances the source revision without changing the table's grid.
        if let Message::FocusCell { revision, .. } | Message::OpenContext { revision, .. } =
            &message
            && !std::sync::Arc::ptr_eq(revision, &self.projection.source_id)
        {
            return Task::none();
        }
        if let Message::MenuScope(scope) = message {
            if let Some(form) = &mut self.table.form
                && let Some(menu) = &mut form.menu
            {
                let previous = menu.scope;
                menu.scope = scope;
                form.selected = menu_items(scope)
                    .iter()
                    .position(
                        |(item, _)| matches!(item, MenuItem::Scope(child) if *child == previous),
                    )
                    .unwrap_or(0);
                return operation::scroll_to(
                    MENU_SCROLL,
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(0.0),
                    },
                )
                .chain(reveal_menu_item(form.selected));
            }
            return Task::none();
        }
        if let Message::MenuHover(index) = message {
            if let Some(form) = &mut self.table.form
                && let Some(menu) = &form.menu
                && index < menu_items(menu.scope).len()
            {
                form.selected = index;
            }
            return Task::none();
        }
        if let Message::MenuAction(action) = message {
            return self.apply_menu_action(action);
        }
        if self.overlay.is_none() {
            if !self.commit_cell() {
                return Task::none();
            }
            if self.grid_view().is_some()
                && let Message::Context(action) = message
            {
                let grid_message = match action {
                    ContextAction::SelectRow => Some(super::table_grid::Message::SelectExtent(
                        super::table_grid::Extent::Row,
                    )),
                    ContextAction::SelectColumn => Some(super::table_grid::Message::SelectExtent(
                        super::table_grid::Extent::Column,
                    )),
                    ContextAction::SelectTable => Some(super::table_grid::Message::SelectExtent(
                        super::table_grid::Extent::Table,
                    )),
                    ContextAction::Clear => Some(super::table_grid::Message::Clear),
                    _ => None,
                };
                if let Some(message) = grid_message {
                    return self.update_grid(message);
                }
            }
        }
        if let Message::OpenContext {
            table,
            row,
            column,
            position,
            extent,
            ..
        } = message
        {
            if self.overlay.is_some() || self.mode != EditorMode::Live {
                return Task::none();
            }
            let Some(target) = self.projection.rich.tables.get(table) else {
                return Task::none();
            };
            if target
                .source_rows
                .get(row)
                .and_then(|cells| cells.get(column))
                .is_none()
            {
                return Task::none();
            }
            let grid = self
                .grid_view()
                .filter(|grid| grid.table == table && grid.contains(row, column));
            let scope = match extent {
                super::table_grid::Extent::Cell => MenuScope::Cell,
                super::table_grid::Extent::Row => MenuScope::Row,
                super::table_grid::Extent::Column => MenuScope::Column,
                super::table_grid::Extent::Table => MenuScope::Table,
            };
            let task = self.open_table_at(table, row, column);
            let form = self.table.form.as_mut().unwrap();
            form.selected = 0;
            form.menu = Some(ContextMenu {
                position,
                root: scope,
                scope,
                grid,
            });
            return task;
        }
        if matches!(
            message,
            Message::Apply(_)
                | Message::Context(
                    ContextAction::Command(_) | ContextAction::DeleteTable | ContextAction::Exit
                )
                | Message::Exit
                | Message::InsertTab
        ) {
            self.clear_grid();
        }
        if let Message::FocusCell {
            revision: _,
            table,
            row,
            column,
            selection,
            open,
        } = message
        {
            if self.overlay.is_some() || self.mode != EditorMode::Live {
                return Task::none();
            }
            let Some(table) = self.projection.rich.tables.get(table) else {
                return Task::none();
            };
            if table
                .source_rows
                .get(row)
                .and_then(|cells| cells.get(column))
                .is_none()
            {
                return Task::none();
            }
            if open {
                let index = self
                    .projection
                    .rich
                    .tables
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, table))
                    .unwrap();
                return self.open_table_at(index, row, column);
            }
            self.editor_focused = true;
            if let Some(mut edit) =
                table_edit::materialize_cell(self.document.source(), row, column, table)
            {
                if !selection.is_caret() {
                    edit.selection.anchor = table_edit::remap_selection(
                        Selection::caret(selection.anchor),
                        &edit.edits,
                    )
                    .anchor;
                }
                self.table_editing = None;
                self.selection = selection;
                self.apply_table_plan(edit, true);
            } else {
                self.selection = selection;
                self.sync_editor();
                self.document.break_history_group();
            }
            return operation::focus(EDITOR);
        }
        if matches!(
            message,
            Message::Context(_) | Message::InsertTab | Message::Exit
        ) && self.overlay.is_none()
        {
            let action = match message {
                Message::Context(action) => Some(action),
                Message::Exit => Some(ContextAction::Exit),
                _ => None,
            };
            return self.table_context_action(action);
        }
        if let Message::Key { key, shift } = message {
            if !matches!(self.overlay, Some(Overlay::Table)) {
                return Task::none();
            }
            let Some(form) = &mut self.table.form else {
                return Task::none();
            };
            if let Some(menu) = &mut form.menu {
                let items = menu_items(menu.scope);
                match key {
                    Named::ArrowUp | Named::Tab if key == Named::ArrowUp || shift => {
                        form.selected = (form.selected + items.len() - 1) % items.len();
                    }
                    Named::ArrowDown | Named::Tab => {
                        form.selected = (form.selected + 1) % items.len()
                    }
                    Named::Home => form.selected = 0,
                    Named::End => form.selected = items.len() - 1,
                    Named::ArrowLeft | Named::Escape if menu.scope != menu.root => {
                        let parent = if menu.scope == MenuScope::Alignment {
                            MenuScope::Column
                        } else {
                            menu.root
                        };
                        return self.update_table(Message::MenuScope(parent));
                    }
                    Named::Escape => return self.update_message(AppMessage::CloseOverlay),
                    Named::Enter | Named::ArrowRight => match items[form.selected].0 {
                        MenuItem::Scope(scope) => {
                            return self.update_table(Message::MenuScope(scope));
                        }
                        MenuItem::Action(action) if key == Named::Enter => {
                            return self.apply_menu_action(action);
                        }
                        _ => {}
                    },
                    _ => {}
                }
                return reveal_menu_item(form.selected);
            }
            match key {
                Named::Escape => return self.update_message(AppMessage::CloseOverlay),
                Named::ArrowUp | Named::Tab if key == Named::ArrowUp || shift => {
                    form.selected = (form.selected + COMMANDS.len() - 1) % COMMANDS.len();
                }
                Named::ArrowDown | Named::Tab => {
                    form.selected = (form.selected + 1) % COMMANDS.len();
                }
                Named::Enter => {
                    let command = COMMANDS[form.selected].0;
                    return self.update_table(Message::Apply(command));
                }
                _ => {}
            }
            return Task::none();
        }
        if self.loading || self.mode == EditorMode::Read {
            return Task::none();
        }
        match message {
            Message::OpenMenu if self.overlay.is_none() && self.mode == EditorMode::Live => {
                let target = self
                    .grid_view()
                    .or_else(|| self.grid_resume())
                    .map(|g| (g.table, g.active.0, g.active.1))
                    .or_else(|| {
                        table_edit::cell_at(&self.projection, Selection::caret(self.selection.head))
                            .and_then(|(table, row, column)| {
                                self.projection
                                    .rich
                                    .tables
                                    .iter()
                                    .position(|candidate| std::ptr::eq(candidate, table))
                                    .map(|index| (index, row, column))
                            })
                    });
                if let Some((table, row, column)) = target {
                    return crate::editor::rich_editor::open_table_menu(
                        std::sync::Arc::clone(&self.projection.source_id),
                        table,
                        row,
                        column,
                    );
                }
                self.notification = Some("Select a table cell to open its controls.".into());
            }
            Message::OpenMenu => return self.update_table(Message::Open),
            Message::Open if self.overlay.is_none() => {
                let Some((table, row, column)) =
                    table_edit::cell_at(&self.projection, Selection::caret(self.selection.head))
                else {
                    self.notification =
                        Some("Place the caret or select text inside one table cell.".into());
                    return Task::none();
                };
                let index = self
                    .projection
                    .rich
                    .tables
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, table))
                    .unwrap();
                return self.open_table_at(index, row, column);
            }
            Message::Apply(command) if matches!(self.overlay, Some(Overlay::Table)) => {
                match self.apply_table_command(command) {
                    Ok(()) => {
                        self.table = State::default();
                        self.overlay = None;
                        self.editor_focused = true;
                        self.sync_editor();
                        return operation::focus(EDITOR);
                    }
                    Err(error) => {
                        if let Some(form) = &mut self.table.form {
                            form.error = Some(error);
                        }
                    }
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn apply_table_command(&mut self, command: Command) -> Result<(), String> {
        let form = self.table.form.as_ref().ok_or("No table is selected.")?;
        let source = self.document.source();
        let range = form.target.resolve(self.document.collaboration(), source)?;
        let selection = self
            .document
            .collaboration()
            .resolve_selection(&form.selection)
            .ok_or("The selected cell is no longer available.")?;
        let table = self
            .projection
            .rich
            .tables
            .iter()
            .find(|table| table.source_range == range)
            .ok_or("The selected table is no longer available.")?;
        let edit =
            table_edit::command_at(source, table, form.row, form.column, selection, command)?;
        if !edit.edits.is_empty() {
            self.document.break_history_group();
            let result = self.document.transact(
                &edit.edits,
                EditOrigin::Human,
                Some((selection, edit.selection)),
                true,
            );
            self.document.break_history_group();
            result.map_err(|error| error.to_string())?;
            self.selection = edit.selection;
            self.preserve_viewport_generation = Some(self.document.collaboration().generation());
            self.changed();
        }
        Ok(())
    }

    fn open_table_at(&mut self, index: usize, row: usize, column: usize) -> Task<AppMessage> {
        let table = &self.projection.rich.tables[index];
        self.table.form = Some(Form {
            menu: None,
            target: Target::new(
                self.document.collaboration(),
                self.document.source(),
                table.source_range.clone(),
            ),
            selection: self.document.collaboration().selection(self.selection),
            selected: 1,
            row,
            column,
            alignment: table.alignment[column],
            last_row: table.source_rows.len() == 1,
            last_column: table.alignment.len() == 1,
            error: None,
        });
        self.overlay = Some(Overlay::Table);
        widget::operate(widget::operation::focusable::unfocus())
    }

    fn table_context_action(&mut self, action: Option<ContextAction>) -> Task<AppMessage> {
        let Some((table, row, _)) =
            table_edit::cell_at(&self.projection, Selection::caret(self.selection.head))
        else {
            return Task::none();
        };
        let focus = Selection::caret(self.selection.head);
        let edit = match action {
            Some(ContextAction::Command(command)) => {
                match table_edit::command(&self.projection, self.document.source(), focus, command)
                {
                    Ok(mut edit) => {
                        edit.selection = table_edit::remap_selection(self.selection, &edit.edits);
                        Some(edit)
                    }
                    Err(error) => {
                        self.notification = Some(error.to_string());
                        None
                    }
                }
            }
            Some(
                ContextAction::SelectRow | ContextAction::SelectColumn | ContextAction::SelectTable,
            ) => {
                let rows = if action == Some(ContextAction::SelectRow) {
                    &table.source_rows[row..row + 1]
                } else {
                    &table.source_rows[..]
                };
                self.selection = Selection {
                    anchor: rows.first().unwrap().first().unwrap().start,
                    head: rows.last().unwrap().last().unwrap().end,
                };
                self.document.break_history_group();
                None
            }
            Some(ContextAction::Clear) => {
                let selection = if self.selection.is_caret() {
                    let (_, row, column) = table_edit::cell_at(&self.projection, focus).unwrap();
                    let cell = &table.source_rows[row][column];
                    Selection {
                        anchor: cell.start,
                        head: cell.end,
                    }
                } else {
                    self.selection
                };
                table_edit::replace_across_cells(
                    &self.projection,
                    self.document.source(),
                    selection,
                    "",
                )
                .or_else(|| {
                    table_edit::insert_text(&self.projection, self.document.source(), selection, "")
                })
            }
            Some(ContextAction::DeleteTable) => Some(table_edit::TableEdit {
                edits: vec![crate::document::collaboration::SourceEdit {
                    range: table.source_range.clone(),
                    text: String::new(),
                }],
                selection: Selection::caret(table.source_range.start),
            }),
            Some(ContextAction::Exit) => {
                let end = table.source_range.end;
                let source = self.document.source();
                let ending = if source[table.source_range.clone()].contains("\r\n") {
                    "\r\n"
                } else if source[table.source_range.clone()].contains('\r') {
                    "\r"
                } else {
                    "\n"
                };
                let text = ending.repeat(if source[..end].ends_with(['\r', '\n']) {
                    1
                } else {
                    2
                });
                Some(table_edit::TableEdit {
                    selection: Selection::caret(end + text.len()),
                    edits: vec![crate::document::collaboration::SourceEdit {
                        range: end..end,
                        text,
                    }],
                })
            }
            None => table_edit::replace_document_selection(
                &self.projection,
                self.document.source(),
                self.selection,
                "\t",
            )
            .or_else(|| {
                table_edit::insert_text(
                    &self.projection,
                    self.document.source(),
                    self.selection,
                    "\t",
                )
            }),
        };
        if let Some(edit) = edit {
            if action == Some(ContextAction::Clear) || action.is_none() {
                self.apply_table_input(edit, true);
            } else {
                self.apply_table_plan(edit, true);
            }
        }
        self.editor_focused = true;
        self.sync_editor();
        operation::focus(EDITOR)
    }

    pub(super) fn table_view(&self) -> Element<'_, AppMessage> {
        let Some(form) = &self.table.form else {
            return text("").into();
        };
        let tokens = self.resolved_theme.tokens;
        let control = |index: usize| {
            let (command, label) = COMMANDS[index];
            button(text(label).size(13))
                .on_press(AppMessage::Table(Message::Apply(command)))
                .width(Fill)
                .padding([9, 10])
                .style(styles::button_style(
                    tokens,
                    if form.selected == index {
                        Button::Selected
                    } else {
                        Button::Quiet
                    },
                ))
        };
        let mut content = column![
            text("Table controls").size(22),
            text(format!(
                "{} · column {}",
                if form.row == 0 {
                    "Header".to_owned()
                } else {
                    format!("Row {}", form.row)
                },
                form.column + 1
            ))
            .size(13)
            .color(tokens.muted),
            row![
                column![
                    text("Rows").size(12).color(tokens.muted),
                    control(0),
                    control(1),
                    control(2)
                ]
                .spacing(4),
                column![
                    text("Columns").size(12).color(tokens.muted),
                    control(3),
                    control(4),
                    control(5)
                ]
                .spacing(4),
            ]
            .spacing(12),
            text("Column alignment").size(12).color(tokens.muted),
            row![control(6), control(7), control(8), control(9)].spacing(4),
            text("↑ / ↓ or Tab to choose · Enter to apply · Esc to cancel")
                .size(12)
                .color(tokens.muted),
        ]
        .spacing(12);
        if form.row == 0 {
            content = content.push(text("Inserting above creates a new header. Deleting the header promotes the next row.").size(12).color(tokens.muted));
        }
        if form.last_row || form.last_column {
            content = content.push(
                text("Deleting the only row or column removes the table.")
                    .size(12)
                    .color(tokens.muted),
            );
        }
        if let Some(error) = &form.error {
            content = content.push(text(error).size(13).color(tokens.text));
        }
        content = content.push(self.small_button("Cancel", AppMessage::CloseOverlay, false));
        container(content)
            .padding(24)
            .width(480)
            .style(styles::card(tokens))
            .into()
    }
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;
