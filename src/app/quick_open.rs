//! Keyboard navigation over workspace files and the current document’s headings.

use std::path::{Path, PathBuf};

use iced::advanced::widget;
use iced::keyboard::key::Named;
use iced::widget::{
    button, column, container, operation, row, scrollable, text, text_input, tooltip,
};
use iced::{Alignment, Element, Fill, Padding, Size, Task};

use super::{App, EDITOR, Message as AppMessage, Overlay, chrome, outline};
use crate::app::icon::Icon;
use crate::document::outline::Section;
use crate::editor::outline_filter::FilterInput;
use crate::theme::styles::{self, Button};

pub(super) const INPUT: &str = "quick-open-query";
const RESULTS: &str = "quick-open-results";
const MAX_RESULTS: usize = 100;
const RESULT_HEIGHT: f32 = 48.0;

#[derive(Debug, Clone)]
pub enum Message {
    Open,
    Focus {
        request: u64,
        previous: Option<widget::Id>,
    },
    Query(String),
    Step(i32),
    Hover(usize),
    Accept,
    Choose(PathBuf),
    ChooseHeading {
        index: usize,
        revision: u64,
        request: u64,
    },
    Cancel,
}

struct Candidate {
    recency: usize,
    pin: Option<usize>,
    path: PathBuf,
    name: String,
    relative: String,
    name_lower: String,
    path_lower: String,
}

#[derive(Default)]
pub(super) struct State {
    root: Option<PathBuf>,
    candidates: Vec<Candidate>,
    query: String,
    results: Vec<usize>,
    matches: usize,
    selected: usize,
    previous: Option<widget::Id>,
    request: u64,
}

impl State {
    pub(super) fn clear(&mut self) {
        let request = self.request.wrapping_add(1);
        *self = Self {
            request,
            ..Self::default()
        };
    }

    /// Index workspace files. Documents in `recent` (most recent first) lead
    /// the list and break ties within each match rank. Pins lead only the empty query.
    fn index(
        &mut self,
        root: &Path,
        files: impl Iterator<Item = impl AsRef<Path>>,
        recent: &[PathBuf],
        pins: &[PathBuf],
    ) {
        let recency: std::collections::HashMap<&Path, usize> = recent
            .iter()
            .enumerate()
            .map(|(rank, path)| (path.as_path(), rank))
            .collect();
        let recency = |path: &Path| recency.get(path).copied().unwrap_or(usize::MAX);
        let pins: std::collections::HashMap<&Path, usize> = pins
            .iter()
            .enumerate()
            .map(|(rank, path)| (path.as_path(), rank))
            .collect();
        self.candidates = files
            .filter_map(|path| {
                let path = path.as_ref();
                let relative = path.strip_prefix(root).ok()?;
                let pin = pins.get(relative).copied();
                let name = path.file_name()?.to_string_lossy().into_owned();
                let relative = relative
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");
                Some(Candidate {
                    recency: recency(path),
                    pin,
                    path: path.to_owned(),
                    name_lower: name.to_lowercase(),
                    path_lower: relative.to_lowercase(),
                    name,
                    relative,
                })
            })
            .collect();
        self.candidates.sort_by(|a, b| {
            a.recency
                .cmp(&b.recency)
                .then_with(|| a.name_lower.cmp(&b.name_lower))
                .then_with(|| a.path_lower.cmp(&b.path_lower))
        });
    }

    fn initial_results(&self) -> Vec<usize> {
        let mut pinned: Vec<_> = self
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| candidate.pin.map(|rank| (rank, index)))
            .collect();
        pinned.sort_unstable();
        pinned
            .into_iter()
            .map(|(_, index)| index)
            .chain(
                self.candidates
                    .iter()
                    .enumerate()
                    .filter(|(_, candidate)| candidate.pin.is_none())
                    .map(|(index, _)| index),
            )
            .take(MAX_RESULTS)
            .collect()
    }

    fn select_previous_file(&mut self, current: &Path) {
        let mut available = self
            .results
            .iter()
            .enumerate()
            .filter(|&(_, index)| self.candidates[*index].path != current);
        self.selected = available
            .clone()
            .filter(|&(_, index)| self.candidates[*index].recency != usize::MAX)
            .min_by_key(|&(_, index)| self.candidates[*index].recency)
            .or_else(|| available.next())
            .map_or(0, |(position, _)| position);
    }

    fn search_files(&mut self) {
        let query = self.query.trim().to_lowercase();
        let query = if cfg!(target_os = "windows") {
            query.replace('\\', "/")
        } else {
            query
        };
        if query.is_empty() {
            self.matches = self.candidates.len();
            self.results = self.initial_results();
            self.selected = self.selected.min(self.results.len().saturating_sub(1));
            return;
        }
        let terms: Vec<_> = query.split_whitespace().collect();
        let mut ranked: [Vec<usize>; 3] = Default::default();
        self.matches = 0;
        for (index, candidate) in self.candidates.iter().enumerate() {
            if !terms.iter().all(|term| candidate.path_lower.contains(term)) {
                continue;
            }
            self.matches += 1;
            let rank = if candidate.name_lower == query {
                0
            } else if candidate.name_lower.starts_with(&query) {
                1
            } else {
                2
            };
            if ranked[rank].len() < MAX_RESULTS {
                ranked[rank].push(index);
            }
        }
        self.results = ranked.into_iter().flatten().take(MAX_RESULTS).collect();
        self.selected = self.selected.min(self.results.len().saturating_sub(1));
    }

    fn headings(&self) -> bool {
        self.root.is_none() || self.heading_query().is_some()
    }

    fn heading_query(&self) -> Option<&str> {
        let query = self.query.trim_start();
        if query == "#" {
            Some("")
        } else {
            query.strip_prefix("# ")
        }
    }

    fn search_headings(&mut self, sections: &[Section]) {
        let query = self.heading_query().unwrap_or(&self.query).to_lowercase();
        let terms: Vec<_> = query.split_whitespace().collect();
        self.results.clear();
        self.matches = 0;
        for index in outline::matching_headings(sections, &terms) {
            self.matches += 1;
            if self.results.len() < MAX_RESULTS {
                self.results.push(index);
            }
        }
        self.selected = self.selected.min(self.results.len().saturating_sub(1));
    }

    fn selected_path(&self) -> Option<&Path> {
        if self.headings() {
            return None;
        }
        self.results
            .get(self.selected)
            .map(|&index| self.candidates[index].path.as_path())
    }
}

impl App {
    pub(super) fn quick_open_available(&self) -> bool {
        self.folder.root().is_some() || !self.outline.sections.is_empty()
    }

    pub(super) fn refresh_quick_open_headings(&mut self) {
        if self.overlay == Some(Overlay::QuickOpen) && self.quick_open.headings() {
            self.quick_open.search_headings(&self.outline.sections);
        }
    }

    pub(super) fn refresh_quick_open(&mut self) -> Task<AppMessage> {
        if self.overlay != Some(Overlay::QuickOpen) {
            return Task::none();
        }
        if self.folder.root() != self.quick_open.root.as_deref() {
            return self.cancel_quick_open();
        }
        let selected = self.quick_open.selected_path().map(Path::to_owned);
        if let Some(root) = self.folder.root() {
            let recent = self.workspace.recent_documents();
            self.quick_open.index(
                root,
                self.folder.files(),
                &recent,
                self.workspace.pins(root),
            );
        }
        if self.quick_open.headings() {
            return Task::none();
        }
        self.quick_open.search_files();
        self.quick_open.selected = self
            .quick_open
            .results
            .iter()
            .position(|&index| Some(&self.quick_open.candidates[index].path) == selected.as_ref())
            .unwrap_or(0);
        self.reveal_quick_open_choice()
    }

    pub(super) fn cancel_quick_open(&mut self) -> Task<AppMessage> {
        if self.overlay != Some(Overlay::QuickOpen) {
            return Task::none();
        }
        let previous = self
            .quick_open
            .previous
            .clone()
            .unwrap_or_else(|| widget::Id::from(EDITOR));
        self.quick_open.clear();
        self.overlay = None;
        self.preserve_viewport_generation = Some(self.document.collaboration().generation());
        operation::focus(previous)
    }

    pub(super) fn update_quick_open(&mut self, message: Message) -> Task<AppMessage> {
        if matches!(message, Message::Open) {
            if self.overlay == Some(Overlay::QuickOpen) {
                return self.cancel_quick_open();
            }
            if self.busy || self.loading || !matches!(self.overlay, None | Some(Overlay::Chrome)) {
                return Task::none();
            }
            if !self.quick_open_available() {
                return Task::none();
            }
            self.quick_open.clear();
            if let Some(root) = self.folder.root() {
                self.quick_open.root = Some(root.to_owned());
                let recent = self.workspace.recent_documents();
                self.quick_open.index(
                    root,
                    self.folder.files(),
                    &recent,
                    self.workspace.pins(root),
                );
                self.quick_open.search_files();
                // Pins change visual order. Keep Enter pointed at the previous file.
                if let Some(current) = self.path.as_deref().filter(|path| path.starts_with(root)) {
                    self.quick_open.select_previous_file(current);
                }
            } else {
                self.quick_open.search_headings(&self.outline.sections);
            }
            self.chrome.menu = None;
            self.overlay = Some(Overlay::QuickOpen);
            self.preserve_viewport_generation = Some(self.document.collaboration().generation());
            let request = self.quick_open.request;
            struct Focused(Option<widget::Id>);
            impl widget::Operation<Option<widget::Id>> for Focused {
                fn traverse(
                    &mut self,
                    operate: &mut dyn FnMut(&mut dyn widget::Operation<Option<widget::Id>>),
                ) {
                    operate(self);
                }
                fn focusable(
                    &mut self,
                    id: Option<&widget::Id>,
                    _: iced::Rectangle,
                    state: &mut dyn widget::operation::Focusable,
                ) {
                    if state.is_focused() {
                        self.0 = id.cloned();
                    }
                }
                fn finish(&self) -> widget::operation::Outcome<Option<widget::Id>> {
                    widget::operation::Outcome::Some(self.0.clone())
                }
            }
            return widget::operate(Focused(None))
                .map(move |previous| AppMessage::QuickOpen(Message::Focus { request, previous }));
        }
        if self.overlay != Some(Overlay::QuickOpen) {
            return Task::none();
        }
        if self.quick_open.root.as_deref() != self.folder.root() {
            return self.cancel_quick_open();
        }
        match message {
            Message::Open => {}
            Message::Focus { request, previous } => {
                if request == self.quick_open.request {
                    self.quick_open.previous = previous;
                    return Task::batch([operation::focus(INPUT), self.reveal_quick_open_choice()]);
                }
            }
            Message::Query(query) => {
                self.quick_open.query = query.chars().take(1024).collect();
                self.quick_open.selected = 0;
                if self.quick_open.headings() {
                    self.quick_open.search_headings(&self.outline.sections);
                } else {
                    self.quick_open.search_files();
                }
                return operation::snap_to(RESULTS, scrollable::RelativeOffset::START);
            }
            Message::Step(direction) => {
                self.quick_open.selected = self
                    .quick_open
                    .selected
                    .saturating_add_signed(direction as isize)
                    .min(self.quick_open.results.len().saturating_sub(1));
                return self.reveal_quick_open_choice();
            }
            Message::Hover(index) => {
                self.quick_open.selected =
                    index.min(self.quick_open.results.len().saturating_sub(1))
            }
            Message::Accept => {
                if self.quick_open.headings()
                    && let Some(&index) = self.quick_open.results.get(self.quick_open.selected)
                {
                    return self.update_quick_open(Message::ChooseHeading {
                        index,
                        revision: self.document.revision(),
                        request: self.quick_open.request,
                    });
                }
                if let Some(path) = self.quick_open.selected_path().map(Path::to_owned) {
                    return self.update_quick_open(Message::Choose(path));
                }
            }
            Message::ChooseHeading {
                index,
                revision,
                request,
            } if !self.busy
                && !self.loading
                && self.quick_open.headings()
                && request == self.quick_open.request
                && revision == self.document.revision()
                && self.quick_open.results.contains(&index) =>
            {
                self.quick_open.clear();
                self.overlay = None;
                self.preserve_viewport_generation = None;
                return self.update_message(AppMessage::Outline(outline::Message::Jump(index)));
            }
            Message::Choose(path) if !self.busy && !self.loading && !self.quick_open.headings() => {
                if !self.folder.files().any(|file| file == path)
                    || !self
                        .quick_open
                        .candidates
                        .iter()
                        .any(|file| file.path == path)
                {
                    return Task::none();
                }
                let focus = self.cancel_quick_open();
                if self.path.as_ref() == Some(&path) {
                    return focus;
                }
                // This is a known file choice. Reuse Files' guarded loader;
                // reclassifying a stale path could turn a renamed directory
                // into an unintended workspace switch.
                let open = self.update_folder(super::folder::Message::Tree(
                    crate::editor::file_tree::Action::Open(path),
                ));
                return Task::batch([focus, open]);
            }
            Message::Choose(_) | Message::ChooseHeading { .. } => {}
            Message::Cancel => return self.cancel_quick_open(),
        }
        Task::none()
    }

    fn reveal_quick_open_choice(&self) -> Task<AppMessage> {
        // Results have one fixed row height; keep keyboard navigation visible
        // without moving focus out of the query or changing the document caret.
        struct Reveal {
            top: f32,
        }
        impl widget::Operation<f32> for Reveal {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation<f32>)) {
                operate(self);
            }
            fn scrollable(
                &mut self,
                id: Option<&widget::Id>,
                bounds: iced::Rectangle,
                _: iced::Rectangle,
                translation: iced::Vector,
                _: &mut dyn widget::operation::Scrollable,
            ) {
                if id == Some(&widget::Id::from(RESULTS)) {
                    self.top = if self.top < translation.y {
                        self.top
                    } else if self.top + RESULT_HEIGHT > translation.y + bounds.height {
                        self.top + RESULT_HEIGHT - bounds.height
                    } else {
                        translation.y
                    };
                }
            }
            fn finish(&self) -> widget::operation::Outcome<f32> {
                widget::operation::Outcome::Some(self.top.max(0.0))
            }
        }
        widget::operate(Reveal {
            top: self.quick_open.selected as f32 * RESULT_HEIGHT,
        })
        .then(|offset| {
            operation::scroll_to(
                RESULTS,
                scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(offset),
                },
            )
        })
    }

    pub(super) fn quick_open_view(&self, size: Size) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let headings = self.quick_open.headings();
        let placeholder = if headings {
            "Find a heading…"
        } else if self.outline.sections.is_empty() {
            "Find a file by name or path…"
        } else {
            "Find a file, or type # then Space for headings…"
        };
        let input = text_input(placeholder, &self.quick_open.query)
            .id(INPUT)
            .on_input(|query| AppMessage::QuickOpen(Message::Query(query)))
            .on_submit(AppMessage::QuickOpen(Message::Accept))
            .padding(10)
            .size(14)
            .style(styles::input(tokens));
        let input = FilterInput::new(input, |key| {
            Some(AppMessage::QuickOpen(match key {
                Named::ArrowUp => Message::Step(-1),
                Named::ArrowDown => Message::Step(1),
                Named::Escape => Message::Cancel,
                _ => return None,
            }))
        });
        let mut results = column![];
        for (position, &index) in self.quick_open.results.iter().enumerate() {
            let (title, context, hint, message, pinned) = if headings {
                let title = outline::heading_label(&self.outline.sections[index]);
                let mut parents: Vec<_> = outline::heading_parents(&self.outline.sections, index)
                    .map(outline::heading_label)
                    .collect();
                parents.reverse();
                let context = parents.join(" › ");
                let hint = if context.is_empty() {
                    title.to_owned()
                } else {
                    format!("{context} › {title}")
                };
                (
                    title,
                    context,
                    hint,
                    Message::ChooseHeading {
                        index,
                        revision: self.document.revision(),
                        request: self.quick_open.request,
                    },
                    false,
                )
            } else {
                let file = &self.quick_open.candidates[index];
                (
                    file.name.as_str(),
                    file.relative.rsplit_once('/').map_or_else(
                        || {
                            self.folder
                                .root()
                                .map(super::folder::name)
                                .unwrap_or_default()
                        },
                        |(parent, _)| parent.to_owned(),
                    ),
                    file.path.display().to_string(),
                    Message::Choose(file.path.clone()),
                    file.pin.is_some(),
                )
            };
            let title = row![
                container(text(title).size(13).wrapping(text::Wrapping::None))
                    .clip(true)
                    .width(Fill)
            ]
            .align_y(Alignment::Center)
            .spacing(6);
            let title = if pinned {
                title.push(text("Pinned").size(11).color(tokens.muted))
            } else {
                title
            };
            let choice = button(
                container(
                    column![
                        title,
                        text(context)
                            .size(11)
                            .color(tokens.muted)
                            .wrapping(text::Wrapping::None),
                    ]
                    .spacing(2),
                )
                .clip(true)
                .width(Fill),
            )
            .padding([6, 10])
            .height(RESULT_HEIGHT)
            .width(Fill)
            .on_press(AppMessage::QuickOpen(message))
            .style(styles::button_style(
                tokens,
                if position == self.quick_open.selected {
                    Button::Selected
                } else {
                    Button::Quiet
                },
            ));
            results = results.push(
                iced::widget::mouse_area(tooltip(
                    choice,
                    text(hint).size(11),
                    tooltip::Position::Bottom,
                ))
                .on_move(move |_| AppMessage::QuickOpen(Message::Hover(position))),
            );
        }
        if self.quick_open.results.is_empty() {
            results = results.push(
                container(
                    text(if headings {
                        "No matching headings"
                    } else if self.quick_open.candidates.is_empty() {
                        "No Markdown files in this workspace"
                    } else {
                        "No matching files"
                    })
                    .size(13)
                    .color(tokens.muted),
                )
                .padding(16),
            );
        }
        let available = (size.height - chrome::TITLEBAR_HEIGHT - 80.0).max(160.0);
        let body = scrollable(results)
            .id(RESULTS)
            .height(iced::Shrink)
            .style(styles::scrollbar(tokens));
        let mut content = column![
            row![
                text(if headings {
                    "Jump to Heading"
                } else {
                    "Quick Open"
                })
                .size(13),
                container(
                    text(if headings {
                        self.display_name.clone()
                    } else {
                        self.folder
                            .root()
                            .map(super::folder::name)
                            .unwrap_or_default()
                    })
                    .size(12)
                    .color(tokens.muted)
                    .wrapping(text::Wrapping::None)
                )
                .clip(true)
                .width(Fill)
                .align_right(Fill),
                self.icon_button(
                    Icon::Close,
                    "Close Quick Open",
                    AppMessage::QuickOpen(Message::Cancel),
                    false
                )
            ]
            .align_y(Alignment::Center)
            .spacing(8),
            input,
            container(body).max_height((available - 100.0).clamp(80.0, 320.0)),
        ]
        .spacing(8);
        if self.quick_open.matches > MAX_RESULTS {
            content = content.push(
                text(format!(
                    "Showing {MAX_RESULTS} of {} matches",
                    self.quick_open.matches
                ))
                .size(11)
                .color(tokens.muted),
            );
        }
        container(
            container(content)
                .padding(12)
                .width(560.0_f32.min((size.width - 32.0).max(160.0)))
                .style(styles::context_menu(tokens)),
        )
        .center_x(Fill)
        .align_top(Fill)
        .padding(Padding::ZERO.top(chrome::TITLEBAR_HEIGHT + 32.0))
        .into()
    }
}

#[cfg(test)]
#[path = "quick_open_tests.rs"]
mod tests;
