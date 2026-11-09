//! Outline disclosure, heading search, and section commands.

use iced::advanced::widget;
use iced::widget::{
    button, column, container, operation, row, scrollable, space, text, text_input, tooltip,
};
use iced::{Alignment, Element, Fill, Task};

use super::{App, EDITOR, EditorMode, Message as AppMessage, Sidebar};
use crate::app::icon::{self, Icon};
use crate::document::collaboration::{EditOrigin, StableSelection};
use crate::document::outline::{self, DropTarget, Section};
use crate::document::selection::Selection;
use crate::editor::outline_filter::FilterInput;
use crate::editor::outline_row::OutlineRow;
use crate::theme::styles::{self, Button};

const FILTER: &str = "outline-filter";
const OUTLINE_SCROLL: &str = "outline-scroll";

fn reveal_outline_row(index: usize) -> Task<AppMessage> {
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
            if id == Some(&widget::Id::from(OUTLINE_SCROLL)) {
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
        target: widget::Id::from(format!("outline-section-{index}")),
        viewport: None,
        item: None,
    })
    .then(|offset| {
        operation::scroll_to(
            OUTLINE_SCROLL,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(offset),
            },
        )
    })
}

#[cfg(test)]
#[path = "outline_tests.rs"]
mod tests;

#[derive(Debug, Clone)]
pub enum Message {
    Toggle(usize),
    FoldAll(bool),
    FoldCurrent(bool),
    Jump(usize),
    Move(i32),
    BeginDrag(usize),
    Hover(DropTarget),
    Leave(usize),
    Drop(DropTarget),
    CancelDrag,
    Filter,
    Query(String),
    Step(i32),
    Accept,
    CancelFilter,
}

#[derive(Default)]
pub(super) struct State {
    revision: Option<u64>,
    pub sections: Vec<Section>,
    // Moving text creates new CRDT identities. Retain both anchors so folds
    // follow the section through move, undo, and redo. Deleted anchors collapse
    // and cannot accidentally fold the next heading.
    collapsed: Vec<Vec<StableSelection>>,
    folded: Vec<bool>,
    pub drag: Option<(usize, u64)>,
    pub hover: Option<DropTarget>,
    pub filter_open: bool,
    query: String,
    results: Vec<usize>,
    selected: usize,
}

impl State {
    fn filtering(&self) -> bool {
        self.filter_open && !self.query.trim().is_empty()
    }

    fn visible(&self) -> Vec<usize> {
        if !self.filtering() {
            return self.unfiltered();
        }
        let mut shown = vec![false; self.sections.len()];
        for &index in &self.results {
            shown[index] = true;
            let mut parent = self.sections[index].parent;
            while let Some(index) = parent {
                shown[index] = true;
                parent = self.sections[index].parent;
            }
        }
        shown
            .into_iter()
            .enumerate()
            .filter_map(|(index, show)| show.then_some(index))
            .collect()
    }

    fn unfiltered(&self) -> Vec<usize> {
        let mut result = Vec::new();
        let mut index = 0;
        while index < self.sections.len() {
            result.push(index);
            index += if self.folded[index] {
                self.sections[index].descendants + 1
            } else {
                1
            };
        }
        result
    }

    fn search(&mut self) {
        let query = self.query.to_lowercase();
        let terms: Vec<_> = query.split_whitespace().collect();
        if terms.is_empty() {
            self.results = self.unfiltered();
            self.selected = self.selected.min(self.results.len().saturating_sub(1));
            return;
        }
        self.results = self
            .sections
            .iter()
            .enumerate()
            .filter_map(|(index, section)| {
                let title = section.title.to_lowercase();
                let mut label = title.clone();
                let mut parent = section.parent;
                while let Some(index) = parent {
                    label.push(' ');
                    label.push_str(&self.sections[index].title.to_lowercase());
                    parent = self.sections[index].parent;
                }
                terms
                    .iter()
                    .all(|term| label.contains(term))
                    .then_some(index)
            })
            .collect();
        self.selected = self.selected.min(self.results.len().saturating_sub(1));
    }

    fn cancel_drag(&mut self) {
        self.drag = None;
        self.hover = None;
    }
}

impl App {
    pub(super) fn sync_outline(&mut self) {
        if self.outline.revision == Some(self.document.revision()) {
            return;
        }
        self.outline.cancel_drag();
        self.outline.sections = outline::sections(&self.document);
        self.outline.revision = Some(self.document.revision());
        self.refresh_folds();
        if self.outline.sections.is_empty() {
            self.outline.filter_open = false;
            self.outline.query.clear();
        }
        if self.outline.filter_open {
            self.outline.search();
        } else {
            self.outline.results.clear();
        }
    }

    fn folded_section(&self, anchors: &[StableSelection]) -> Option<usize> {
        anchors.iter().find_map(|anchor| {
            let selection = self.document.collaboration().resolve_selection(anchor)?;
            if selection.is_caret() {
                return None;
            }
            self.outline
                .sections
                .binary_search_by_key(&selection.range().start, |s| s.range.start)
                .ok()
        })
    }

    fn refresh_folds(&mut self) {
        self.outline.folded = vec![false; self.outline.sections.len()];
        for anchors in &self.outline.collapsed {
            if let Some(index) = self.folded_section(anchors) {
                self.outline.folded[index] = true;
            }
        }
    }

    fn fold_section(&mut self, index: usize, folded: bool) {
        let Some(section) = self.outline.sections.get(index) else {
            return;
        };
        if section.descendants == 0 {
            return;
        }
        if folded && !self.outline.folded[index] {
            let start = section.range.start;
            let end = start
                + self.document.source()[start..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8);
            self.outline
                .collapsed
                .push(vec![self.document.collaboration().selection(Selection {
                    anchor: start,
                    head: end,
                })]);
        } else if !folded {
            let remove: Vec<_> = self
                .outline
                .collapsed
                .iter()
                .enumerate()
                .filter_map(|(slot, anchors)| {
                    (self.folded_section(anchors) == Some(index)).then_some(slot)
                })
                .collect();
            for slot in remove.into_iter().rev() {
                self.outline.collapsed.remove(slot);
            }
        }
        self.outline.folded[index] = folded;
    }

    pub(super) fn update_outline(&mut self, message: Message) -> Task<AppMessage> {
        if matches!(message, Message::CancelFilter) {
            self.outline.cancel_drag();
        }
        if matches!(message, Message::CancelDrag) {
            self.outline.cancel_drag();
            return Task::none();
        }
        if self.loading {
            return Task::none();
        }
        if self.overlay.is_some() {
            return Task::none();
        }
        match message {
            Message::Toggle(index) => {
                if self.outline.filtering() {
                    return Task::none();
                }
                if let Some(folded) = self.outline.folded.get(index).copied() {
                    self.fold_section(index, !folded);
                }
            }
            Message::FoldAll(folded) => {
                if self.outline.filtering() {
                    return Task::none();
                }
                if folded {
                    for index in 0..self.outline.sections.len() {
                        self.fold_section(index, true);
                    }
                } else {
                    self.outline.collapsed.clear();
                    self.refresh_folds();
                }
            }
            Message::FoldCurrent(folded) => {
                if let Some(mut index) =
                    outline::current(&self.outline.sections, self.selection.head)
                {
                    if !folded {
                        let mut parent = self.outline.sections[index].parent;
                        while let Some(ancestor) = parent {
                            if self.outline.folded[ancestor] {
                                index = ancestor;
                            }
                            parent = self.outline.sections[ancestor].parent;
                        }
                    }
                    if folded
                        && (self.outline.sections[index].descendants == 0
                            || self.outline.folded[index])
                    {
                        index = self.outline.sections[index].parent.unwrap_or(index);
                    }
                    self.fold_section(index, folded);
                }
            }
            Message::Jump(index) => {
                let Some(section) = self.outline.sections.get(index) else {
                    return Task::none();
                };
                let block = section.block;
                let mut parent = section.parent;
                while let Some(index) = parent {
                    parent = self.outline.sections[index].parent;
                    self.fold_section(index, false);
                }
                self.overlay = None;
                self.outline.cancel_drag();
                if let Some(slot) = self
                    .outline
                    .results
                    .iter()
                    .position(|&result| result == index)
                {
                    self.outline.selected = slot;
                }
                if self.mode == EditorMode::Read {
                    self.selection = Selection::caret(self.document.blocks()[block].range.start);
                    self.document.break_history_group();
                    self.sync_editor();
                }
                return self.update_message(AppMessage::JumpTo(block));
            }
            Message::Move(direction) => {
                if let Some(from) = outline::current(&self.outline.sections, self.selection.head)
                    && let Some(section) = outline::sibling(&self.outline.sections, from, direction)
                {
                    return self.move_section(
                        from,
                        DropTarget {
                            section,
                            after: direction > 0,
                        },
                    );
                }
            }
            Message::BeginDrag(index) => {
                if self.outline.filtering() {
                    return Task::none();
                }
                if index < self.outline.sections.len() {
                    self.outline.drag = Some((index, self.document.revision()));
                    self.outline.hover = None;
                }
            }
            Message::Hover(target) => {
                self.outline.hover = self.outline.drag.and_then(|(from, revision)| {
                    (revision == self.document.revision()
                        && outline::destination(&self.outline.sections, from, target).is_some())
                    .then_some(target)
                });
            }
            Message::Leave(index) => {
                if self
                    .outline
                    .hover
                    .is_some_and(|target| target.section == index)
                {
                    self.outline.hover = None;
                }
            }
            Message::Drop(target) => {
                let drag = self.outline.drag;
                self.outline.cancel_drag();
                if let Some((from, revision)) = drag
                    && revision == self.document.revision()
                {
                    return self.move_section(from, target);
                }
            }
            Message::CancelDrag => {}
            Message::Filter => {
                if self.outline.sections.is_empty() {
                    return Task::none();
                }
                self.outline.cancel_drag();
                self.document.break_history_group();
                self.outline.filter_open = true;
                self.outline.search();
                self.outline.selected = self
                    .outline
                    .results
                    .iter()
                    .position(|&index| {
                        Some(index) == outline::current(&self.outline.sections, self.selection.head)
                    })
                    .unwrap_or(0);
                let show = self.show_sidebar(Sidebar::Outline);
                return Task::batch([show, operation::focus(FILTER)]);
            }
            Message::Query(query) if self.outline.filter_open => {
                self.outline.query = query;
                self.outline.selected = 0;
                self.outline.cancel_drag();
                self.outline.search();
                return operation::snap_to(
                    OUTLINE_SCROLL,
                    iced::widget::scrollable::RelativeOffset::START,
                );
            }
            Message::Step(direction) if self.outline.filter_open => {
                self.outline.selected = self
                    .outline
                    .selected
                    .saturating_add_signed(direction as isize)
                    .min(self.outline.results.len().saturating_sub(1));
                if let Some(&index) = self.outline.results.get(self.outline.selected) {
                    return reveal_outline_row(index);
                }
            }
            Message::Accept if self.outline.filter_open => {
                if let Some(&index) = self.outline.results.get(self.outline.selected) {
                    return self.update_outline(Message::Jump(index));
                }
            }
            Message::CancelFilter if self.outline.filter_open => {
                self.outline.filter_open = false;
                self.outline.query.clear();
                self.outline.search();
                return operation::focus(EDITOR);
            }
            Message::CancelFilter => {}
            Message::Query(_) | Message::Step(_) | Message::Accept => {}
        }
        if self.outline.filter_open {
            self.outline.search();
        }
        Task::none()
    }

    fn move_section(&mut self, from: usize, target: DropTarget) -> Task<AppMessage> {
        let movement =
            match outline::prepare(self.document.source(), &self.outline.sections, from, target) {
                Ok(Some(movement)) => movement,
                Ok(None) => return Task::none(),
                Err(error) => {
                    self.notification = Some(error.into());
                    return Task::none();
                }
            };
        let source_range = self.outline.sections[from].range.clone();
        let folds: Vec<_> = self
            .outline
            .collapsed
            .iter()
            .enumerate()
            .filter_map(|(slot, anchors)| {
                let index = self.folded_section(anchors)?;
                let start = self.outline.sections[index].range.start;
                source_range
                    .contains(&start)
                    .then_some((slot, movement.offset(start, false)))
            })
            .collect();
        let after = movement.selection(self.selection);
        match self.document.transact(
            &movement.edits,
            EditOrigin::Human,
            Some((self.selection, after)),
            true,
        ) {
            Ok(()) => {
                self.document.break_history_group();
                for (slot, start) in folds {
                    let end = start
                        + self.document.source()[start..]
                            .chars()
                            .next()
                            .map_or(0, char::len_utf8);
                    self.outline.collapsed[slot].push(self.document.collaboration().selection(
                        Selection {
                            anchor: start,
                            head: end,
                        },
                    ));
                }
                self.selection = after;
                if self.mode == EditorMode::Read {
                    self.mode = EditorMode::Live;
                }
                self.editor_focused = true;
                self.sync_editor();
                self.changed();
                operation::focus(EDITOR)
            }
            Err(error) => {
                self.notification = Some(error.to_string());
                Task::none()
            }
        }
    }

    pub(super) fn outline_list(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let mut list = column![];
        let current = outline::current(&self.outline.sections, self.selection.head);
        let visible = self.outline.visible();
        let indicator = self.outline.hover.map(|target| {
            if target.after {
                let end = target.section + self.outline.sections[target.section].descendants;
                DropTarget {
                    section: visible[visible.partition_point(|&index| index <= end) - 1],
                    after: true,
                }
            } else {
                target
            }
        });
        for index in visible {
            let section = &self.outline.sections[index];
            let active = if self.outline.filter_open {
                self.outline.results.get(self.outline.selected) == Some(&index)
            } else {
                current.is_some_and(|current| {
                    current == index
                        || self.outline.folded[index]
                            && current > index
                            && current <= index + section.descendants
                })
            };
            let disclosure: Element<'_, AppMessage> = if section.descendants > 0 {
                button(
                    text(if self.outline.folded[index] && !self.outline.filtering() {
                        "▸"
                    } else {
                        "▾"
                    })
                    .size(12),
                )
                .padding([5, 2])
                .width(16)
                .height(styles::SIDEBAR_ROW_HEIGHT)
                .on_press_maybe(
                    (!self.outline.filtering())
                        .then_some(AppMessage::Outline(Message::Toggle(index))),
                )
                .style(styles::button_style(tokens, Button::Quiet))
                .into()
            } else {
                space().width(16).into()
            };
            let title = if section.title.is_empty() {
                "Untitled heading"
            } else {
                &section.title
            };
            let label = button(
                container(
                    text(title)
                        .size(styles::SIDEBAR_TEXT_SIZE)
                        .wrapping(text::Wrapping::None),
                )
                .clip(true),
            )
            .padding([5, 4])
            .height(styles::SIDEBAR_ROW_HEIGHT)
            .width(Fill)
            .on_press(AppMessage::Outline(Message::Jump(index)))
            .style(styles::sidebar_label(tokens, active));
            let handle = tooltip(
                space().width(22).height(styles::SIDEBAR_ROW_HEIGHT),
                text("Drag before/after a heading of the same level").size(12),
                tooltip::Position::Right,
            );
            let mut depth = 0;
            let mut parent = section.parent;
            while let Some(index) = parent {
                depth += 1;
                parent = self.outline.sections[index].parent;
            }
            let contents = container(row![disclosure, label, handle].align_y(Alignment::Center))
                .id(format!("outline-section-{index}"))
                .padding(
                    iced::Padding::ZERO
                        .left(styles::SIDEBAR_INSET + depth as f32 * styles::SIDEBAR_INDENT),
                );
            list = list.push(
                OutlineRow::new(
                    contents.into(),
                    index,
                    self.outline.drag.is_some(),
                    self.outline.hover,
                    tokens,
                )
                .active(active)
                .draggable(!self.outline.filtering())
                .indicator(
                    indicator
                        .filter(|target| target.section == index)
                        .map(|target| target.after),
                ),
            );
        }
        if self.outline.sections.is_empty() {
            list = list.push(text("No headings").size(12).color(tokens.muted));
        }
        if self.outline.filtering() && self.outline.results.is_empty() {
            list = list.push(text("No matching headings").size(12).color(tokens.muted));
        }
        list.into()
    }

    pub(super) fn outline_tools(&self) -> Element<'_, AppMessage> {
        if self.outline.sections.is_empty() {
            return space().width(0).into();
        }
        let tool = |kind, label, message, enabled: bool| {
            let tokens = self.resolved_theme.tokens;
            let selected = matches!(kind, Icon::Search) && self.outline.filter_open;
            container(tooltip(
                button(icon::sized(
                    kind,
                    if !enabled {
                        tokens.disabled
                    } else if selected {
                        tokens.on_accent_soft
                    } else {
                        tokens.muted
                    },
                    16.0,
                ))
                .padding(6)
                .on_press_maybe(enabled.then_some(AppMessage::Outline(message)))
                .style(styles::button_style(
                    tokens,
                    if selected {
                        Button::Selected
                    } else {
                        Button::Quiet
                    },
                )),
                text(label).size(11),
                tooltip::Position::Bottom,
            ))
        };
        row![
            tool(
                Icon::Search,
                "Filter headings",
                if self.outline.filter_open {
                    Message::CancelFilter
                } else {
                    Message::Filter
                },
                true
            )
            .id("outline-filter-control"),
            tool(
                Icon::CollapseAll,
                "Collapse all",
                Message::FoldAll(true),
                !self.outline.filtering()
            )
            .id("outline-collapse-control"),
            tool(
                Icon::ExpandAll,
                "Expand all",
                Message::FoldAll(false),
                !self.outline.filtering()
            )
            .id("outline-expand-control"),
        ]
        .spacing(0)
        .into()
    }

    pub(super) fn outline_view(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let mut content = column![].spacing(8);
        if self.outline.filter_open && !self.outline.sections.is_empty() {
            let input = text_input("Filter headings…", &self.outline.query)
                .id(FILTER)
                .on_input(|query| AppMessage::Outline(Message::Query(query)))
                .on_submit(AppMessage::Outline(Message::Accept))
                .size(12)
                .padding(8)
                .style(styles::input(tokens));
            content = content.push(FilterInput::new(input, |key| {
                use iced::keyboard::key::Named;
                Some(AppMessage::Outline(match key {
                    Named::ArrowUp => Message::Step(-1),
                    Named::ArrowDown => Message::Step(1),
                    Named::Escape => Message::CancelFilter,
                    _ => return None,
                }))
            }));
        }
        content
            .push(
                iced::widget::scrollable(self.outline_list())
                    .id(OUTLINE_SCROLL)
                    .height(Fill)
                    .style(styles::scrollbar(tokens)),
            )
            .height(Fill)
            .into()
    }
}
