//! Workspace link suggestions and the on-demand incoming links panel.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{button, column, container, operation, row, scrollable, text, text_input};
use iced::{Alignment, Element, Event, Fill, Task, event, window};

use super::{App, EDITOR, EditorMode, Message as AppMessage, Overlay};
use crate::document::selection::Selection;
use crate::document::writing::HeadingIds;
use crate::editor::{outline_filter::FilterInput, viewport};
use crate::io::document_links::{self, Backlink, Backlinks, Heading};
use crate::io::storage::FileError;
use crate::theme::styles::{self, Button};

const FILTER: &str = "link-suggestion-filter";
pub(super) const DESTINATION: &str = "writing-link-destination";
const RESULTS: usize = 8;
const BACKLINKS_LIST: &str = "backlinks-list";

#[derive(Debug, Clone)]
pub enum Jump {
    Heading(String),
    Reference { offset: usize, digest: blake3::Hash },
}

#[derive(Debug, Clone)]
pub enum Message {
    Pick,
    ClosePicker,
    PickAnotherFile,
    Filter(String),
    Step(i32),
    Accept,
    Choice(Choice),
    HeadingsLoaded(Arc<()>, Result<Vec<Heading>, FileError>),
    ShowBacklinks,
    BacklinksLoaded(Arc<AtomicBool>, Result<Backlinks, FileError>),
    StepBacklinks(i32),
    OpenBacklink(Option<usize>),
}

#[derive(Debug, Clone)]
pub enum Choice {
    File(PathBuf),
    Heading(Option<String>),
}

#[derive(Default)]
pub(super) struct State {
    picker: Option<Picker>,
    backlinks: Option<BacklinkPanel>,
}

impl State {
    pub(super) fn close_picker(&mut self) {
        self.picker = None;
    }

    pub(super) fn close(&mut self) {
        self.close_picker();
        if let Some(panel) = self.backlinks.take() {
            panel.request.store(true, Ordering::Relaxed);
        }
    }
}

struct Picker {
    root: PathBuf,
    base: PathBuf,
    filter: String,
    selected: usize,
    target: Option<Target>,
}

struct Target {
    file: PathBuf,
    request: Arc<()>,
    headings: Option<Result<Vec<Heading>, FileError>>,
}

struct BacklinkPanel {
    root: PathBuf,
    origin: PathBuf,
    request: Arc<AtomicBool>,
    result: Option<Result<Backlinks, FileError>>,
    selected: usize,
}

pub(super) fn event_message(
    event: Event,
    status: event::Status,
    id: window::Id,
) -> Option<AppMessage> {
    if let Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Named(key),
        modifiers,
        ..
    }) = &event
        && modifiers.is_empty()
    {
        let action = match key {
            Named::ArrowUp => Some(Message::StepBacklinks(-1)),
            Named::ArrowDown => Some(Message::StepBacklinks(1)),
            Named::Enter => Some(Message::OpenBacklink(None)),
            _ => None,
        };
        if let Some(action) = action {
            return Some(AppMessage::Links(action));
        }
    }
    super::event_message(event, status, id)
}

struct Suggestion {
    title: String,
    detail: Option<String>,
    choice: Choice,
}

impl App {
    pub(super) fn link_base(&self) -> Option<&Path> {
        self.session
            .path
            .as_deref()
            .and_then(Path::parent)
            .or(self.session.folder.as_deref())
    }

    fn link_picker_scope(&self) -> Option<(&Path, &Path)> {
        let root = self.folder.root()?;
        let base = self.link_base()?;
        base.starts_with(root).then_some((root, base))
    }

    fn backlink_scope(&self) -> Option<(&Path, &Path)> {
        let root = self.folder.root()?;
        let origin = self.session.path.as_deref()?;
        (origin.starts_with(root) && origin != root).then_some((root, origin))
    }

    pub(super) fn link_picker_available(&self) -> bool {
        self.link_picker_scope().is_some()
    }

    pub(super) fn backlinks_available(&self) -> bool {
        self.backlink_scope().is_some()
    }

    pub(super) fn update_document_links(&mut self, message: Message) -> Task<AppMessage> {
        let picker_active = self.overlay == Some(Overlay::Writing)
            && self
                .link_picker_scope()
                .zip(self.links.picker.as_ref())
                .is_some_and(|((root, base), picker)| root == picker.root && base == picker.base);
        match message {
            Message::Pick if self.link_form_open() => {
                let Some((root, base)) = self.link_picker_scope() else {
                    return Task::none();
                };
                self.links.picker = Some(Picker {
                    root: root.to_owned(),
                    base: base.to_owned(),
                    filter: String::new(),
                    selected: 0,
                    target: None,
                });
                return operation::focus(FILTER);
            }
            Message::ClosePicker if picker_active => {
                self.links.close_picker();
                return operation::focus(DESTINATION);
            }
            Message::PickAnotherFile if picker_active => {
                if let Some(picker) = &mut self.links.picker {
                    picker.target = None;
                    picker.filter.clear();
                    picker.selected = 0;
                    return operation::focus(FILTER);
                }
            }
            Message::Filter(filter) if picker_active => {
                if let Some(picker) = &mut self.links.picker {
                    picker.filter = filter.chars().take(256).collect();
                    picker.selected = 0;
                }
            }
            Message::Step(step) if picker_active => {
                let count = self.link_suggestions().len();
                if let Some(picker) = &mut self.links.picker {
                    picker.selected = picker
                        .selected
                        .saturating_add_signed(step as isize)
                        .min(count.saturating_sub(1));
                }
            }
            Message::Accept if picker_active => {
                if let Some(suggestion) = self.link_suggestions().into_iter().nth(
                    self.links
                        .picker
                        .as_ref()
                        .map_or(0, |picker| picker.selected),
                ) {
                    return self.select_link_suggestion(suggestion.choice);
                }
            }
            Message::Choice(choice) if picker_active => return self.select_link_suggestion(choice),
            Message::HeadingsLoaded(request, headings) if picker_active => {
                if let Some(target) = self
                    .links
                    .picker
                    .as_mut()
                    .and_then(|picker| picker.target.as_mut())
                    .filter(|target| Arc::ptr_eq(&request, &target.request))
                {
                    target.headings = Some(headings);
                }
            }
            Message::ShowBacklinks if self.overlay.is_none() && !self.busy && !self.loading => {
                let Some((root, origin)) = self.backlink_scope() else {
                    return Task::none();
                };
                let root = root.to_owned();
                let origin = origin.to_owned();
                let request = Arc::new(AtomicBool::new(false));
                self.links.backlinks = Some(BacklinkPanel {
                    root: root.clone(),
                    origin: origin.clone(),
                    request: Arc::clone(&request),
                    result: None,
                    selected: 0,
                });
                self.overlay = Some(Overlay::Backlinks);
                self.preserve_viewport_generation =
                    Some(self.session.document.collaboration().generation());
                let paths = self.folder.files().map(Path::to_owned).collect();
                let complete = !self.folder.incomplete();
                return Task::perform(
                    document_links::backlinks(root, paths, origin, Arc::clone(&request)),
                    move |result| {
                        let result = result.map(|mut result| {
                            result.partial |= !complete;
                            result
                        });
                        AppMessage::Links(Message::BacklinksLoaded(Arc::clone(&request), result))
                    },
                );
            }
            Message::BacklinksLoaded(request, result)
                if self.overlay == Some(Overlay::Backlinks) =>
            {
                if let Some(panel) = &mut self.links.backlinks
                    && Arc::ptr_eq(&request, &panel.request)
                {
                    panel.result = Some(result);
                }
            }
            Message::StepBacklinks(step) if self.overlay == Some(Overlay::Backlinks) => {
                if let Some(panel) = &mut self.links.backlinks
                    && let Some(Ok(result)) = &panel.result
                {
                    let last = result.entries.len().saturating_sub(1);
                    panel.selected = panel
                        .selected
                        .saturating_add_signed(step as isize)
                        .min(last);
                    return operation::snap_to(
                        BACKLINKS_LIST,
                        scrollable::RelativeOffset {
                            x: 0.0,
                            y: panel.selected as f32 / last.max(1) as f32,
                        },
                    );
                }
            }
            Message::OpenBacklink(index) if self.overlay == Some(Overlay::Backlinks) => {
                let Some(result) = self
                    .links
                    .backlinks
                    .as_ref()
                    .filter(|panel| self.backlink_scope() == Some((&panel.root, &panel.origin)))
                    .and_then(|panel| {
                        panel
                            .result
                            .as_ref()?
                            .as_ref()
                            .ok()?
                            .entries
                            .get(index.unwrap_or(panel.selected))
                    })
                    .cloned()
                else {
                    return Task::none();
                };
                if self.folder.has_file(&result.path) {
                    return self.open_document_link(
                        result.path,
                        Some(Jump::Reference {
                            offset: result.offset,
                            digest: result.digest,
                        }),
                    );
                }
                self.notification = Some("This file is no longer in the open place.".into());
            }
            _ => {}
        }
        Task::none()
    }

    fn select_link_suggestion(&mut self, choice: Choice) -> Task<AppMessage> {
        let Some(picker) = self.links.picker.as_mut() else {
            return Task::none();
        };
        match choice {
            Choice::File(file) if picker.target.is_none() && self.folder.has_file(&file) => {
                if document_links::relative(&picker.root, &picker.base, &file).is_none() {
                    return Task::none();
                }
                let request = Arc::new(());
                let root = picker.root.clone();
                let open = if self.session.path.as_ref() == Some(&file) {
                    Some(&self.session)
                } else {
                    self.background
                        .dirty()
                        .find(|session| session.path.as_ref() == Some(&file))
                };
                let headings = open.map(|session| {
                    let mut ids = HeadingIds::default();
                    Ok(session
                        .document
                        .blocks()
                        .iter()
                        .enumerate()
                        .filter_map(|(index, block)| {
                            let (_, title) = block.heading.as_ref()?;
                            let fragment =
                                ids.assign_for_block(title, session.document.block_source(index));
                            (!fragment.is_empty()).then(|| Heading {
                                title: title.clone(),
                                fragment,
                            })
                        })
                        .take(document_links::MAX_HEADINGS)
                        .collect())
                });
                picker.filter.clear();
                picker.selected = 0;
                picker.target = Some(Target {
                    file: file.clone(),
                    request: Arc::clone(&request),
                    headings,
                });
                if open.is_none() {
                    return Task::perform(document_links::headings(root, file), move |headings| {
                        AppMessage::Links(Message::HeadingsLoaded(Arc::clone(&request), headings))
                    });
                }
            }
            Choice::Heading(fragment) => {
                let Some(target) = &picker.target else {
                    return Task::none();
                };
                if !self.folder.has_file(&target.file) {
                    self.notification = Some("This file is no longer in the open place.".into());
                    return Task::none();
                }
                let Some(relative) =
                    document_links::relative(&picker.root, &picker.base, &target.file)
                else {
                    return Task::none();
                };
                let (destination, title) = if let Some(fragment) = fragment {
                    let Some(heading) = target
                        .headings
                        .as_ref()
                        .and_then(|headings| headings.as_ref().ok())
                        .and_then(|headings| {
                            headings.iter().find(|heading| heading.fragment == fragment)
                        })
                    else {
                        return Task::none();
                    };
                    let path = if self.session.path.as_ref() == Some(&target.file) {
                        String::new()
                    } else {
                        relative
                    };
                    (
                        document_links::with_heading(path, &fragment),
                        heading.title.clone(),
                    )
                } else {
                    let title = target
                        .file
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .unwrap_or(&relative);
                    (relative.clone(), title.to_owned())
                };
                self.links.close_picker();
                self.set_suggested_link(destination, title);
                return operation::focus(DESTINATION);
            }
            _ => {}
        }
        Task::none()
    }

    fn link_suggestions(&self) -> Vec<Suggestion> {
        let Some(picker) = &self.links.picker else {
            return Vec::new();
        };
        let query = picker.filter.trim().to_lowercase();
        if let Some(target) = &picker.target {
            let mut suggestions = vec![Suggestion {
                title: "Link to whole file".into(),
                detail: None,
                choice: Choice::Heading(None),
            }];
            if let Some(Ok(headings)) = &target.headings {
                suggestions.extend(
                    headings
                        .iter()
                        .filter(|heading| {
                            heading.title.to_lowercase().contains(&query)
                                || heading.fragment.to_lowercase().contains(&query)
                        })
                        .take(RESULTS)
                        .map(|heading| Suggestion {
                            title: heading.title.clone(),
                            detail: Some(format!("#{}", heading.fragment)),
                            choice: Choice::Heading(Some(heading.fragment.clone())),
                        }),
                );
            }
            return suggestions;
        }
        let terms: Vec<_> = query.split_whitespace().collect();
        let mut ranked: [Vec<_>; 3] = Default::default();
        for path in self.folder.files() {
            let Some(relative) = path.strip_prefix(&picker.root).ok().and_then(Path::to_str) else {
                continue;
            };
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let lower = relative.to_lowercase();
            if !terms.iter().all(|term| lower.contains(term)) {
                continue;
            }
            let name_lower = name.to_lowercase();
            let rank = if name_lower == query {
                0
            } else if name_lower.starts_with(&query) {
                1
            } else {
                2
            };
            if ranked[rank].len() < RESULTS {
                ranked[rank].push(Suggestion {
                    title: name.to_owned(),
                    detail: Some(relative.to_owned()),
                    choice: Choice::File(path.to_owned()),
                });
                if query.is_empty() && ranked[rank].len() == RESULTS {
                    break;
                }
            }
        }
        ranked.into_iter().flatten().take(RESULTS).collect()
    }

    pub(super) fn link_picker_view(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let Some(picker) = &self.links.picker else {
            return self.small_button(
                "Find file or heading…",
                AppMessage::Links(Message::Pick),
                false,
            );
        };
        let title = picker.target.as_ref().map_or_else(
            || "Choose a file".to_owned(),
            |target| super::folder::name(&target.file),
        );
        let input = text_input(
            if picker.target.is_some() {
                "Filter headings"
            } else {
                "File name or path"
            },
            &picker.filter,
        )
        .id(FILTER)
        .on_input(|value| AppMessage::Links(Message::Filter(value)))
        .on_submit(AppMessage::Links(Message::Accept))
        .padding(8)
        .size(13)
        .style(styles::input(tokens));
        let input = FilterInput::new(input, |key| {
            Some(AppMessage::Links(match key {
                Named::ArrowUp => Message::Step(-1),
                Named::ArrowDown => Message::Step(1),
                Named::Escape => Message::ClosePicker,
                _ => return None,
            }))
        });
        let mut heading = row![text(title).size(12).color(tokens.muted).width(Fill)]
            .spacing(5)
            .align_y(Alignment::Center);
        if picker.target.is_some() {
            heading = heading.push(self.small_button(
                "Files",
                AppMessage::Links(Message::PickAnotherFile),
                false,
            ));
        }
        heading =
            heading.push(self.small_button("Done", AppMessage::Links(Message::ClosePicker), false));
        let mut content = column![heading, input].spacing(5);
        let suggestions = self.link_suggestions();
        if suggestions.is_empty() {
            content = content.push(text("No matching files").size(12).color(tokens.muted));
        }
        if let Some(target) = &picker.target {
            if target.headings.is_none() {
                content = content.push(text("Reading headings…").size(12).color(tokens.muted));
            } else if let Some(Err(error)) = &target.headings {
                content = content.push(text(error.to_string()).size(12).color(tokens.muted));
            } else if suggestions.len() == 1 {
                content = content.push(text("No matching headings").size(12).color(tokens.muted));
            }
        }
        for (index, suggestion) in suggestions.into_iter().enumerate() {
            content = content.push(link_choice(
                tokens,
                suggestion.title,
                suggestion.detail,
                AppMessage::Links(Message::Choice(suggestion.choice)),
                picker.selected == index,
            ));
        }
        content.into()
    }

    pub(super) fn backlinks_view(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let mut content = column![
            text("Linked from").font(styles::BODY_FONT).size(28),
            text(&self.session.display_name)
                .size(13)
                .color(tokens.muted)
        ]
        .spacing(10);
        if let Some(panel) = &self.links.backlinks {
            match &panel.result {
                None => content = content.push(text("Checking files in this place…").size(13)),
                Some(Err(error)) => content = content.push(text(error.to_string()).size(13)),
                Some(Ok(result)) => {
                    if result.entries.is_empty() {
                        content = content.push(
                            text(if result.partial {
                                "No links found in the files checked."
                            } else {
                                "No files in this place link to this document."
                            })
                            .size(13),
                        );
                    }
                    if result.partial {
                        content = content.push(
                            text("Some files or links could not be checked.")
                                .size(12)
                                .color(tokens.muted),
                        );
                    }
                    let mut rows = column![].spacing(3);
                    for (index, Backlink { path, snippet, .. }) in result.entries.iter().enumerate()
                    {
                        let name = path
                            .strip_prefix(&panel.root)
                            .unwrap_or(path)
                            .display()
                            .to_string();
                        rows = rows.push(link_choice(
                            tokens,
                            name,
                            Some(snippet.clone()),
                            AppMessage::Links(Message::OpenBacklink(Some(index))),
                            panel.selected == index,
                        ));
                    }
                    if !result.entries.is_empty() {
                        content = content.push(
                            container(
                                scrollable(rows)
                                    .id(BACKLINKS_LIST)
                                    .style(styles::scrollbar(tokens)),
                            )
                            .max_height(320),
                        );
                    }
                }
            }
        }
        content = content.push(self.small_button("Done", AppMessage::CloseOverlay, true));
        container(content)
            .width(520)
            .padding(28)
            .style(styles::card(tokens))
            .into()
    }

    pub(super) fn open_document_link(
        &mut self,
        path: PathBuf,
        jump: Option<Jump>,
    ) -> Task<AppMessage> {
        if self.busy || self.loading || !self.commit_cell() {
            return Task::none();
        }
        self.overlay = None;
        self.writing.close_details();
        self.links.close();
        if self.session.path.as_ref() == Some(&path) {
            return self.focus_link_target(jump);
        }
        if let Some(parked) = self.background.take_dirty(Some(&path)) {
            return self.resume_at(parked, jump.map(super::OpenPosition::Link), true);
        }
        self.busy = true;
        self.loading = true;
        Task::perform(document_links::resolve_path(path), move |result| {
            AppMessage::ResolvedLinked(result, jump.clone())
        })
    }

    pub(super) fn focus_link_target(&mut self, jump: Option<Jump>) -> Task<AppMessage> {
        jump.and_then(|jump| self.jump_to_document_link(jump))
            .unwrap_or_else(|| operation::focus(EDITOR))
    }

    pub(super) fn jump_to_document_link(&mut self, jump: Jump) -> Option<Task<AppMessage>> {
        match jump {
            Jump::Heading(fragment) => {
                let mut ids = HeadingIds::default();
                let index = self.outline.sections.iter().position(|section| {
                    ids.assign_for_block(
                        &section.title,
                        self.session.document.block_source(section.block),
                    ) == fragment
                });
                if let Some(index) = index {
                    self.overlay = None;
                    self.writing.close_details();
                    self.links.close_picker();
                    self.preserve_viewport_generation = None;
                    return Some(self.update_outline(super::outline::Message::Jump(index)));
                }
                self.notification = Some("No heading matches this link.".into());
            }
            Jump::Reference { offset, digest } => {
                let source = self.session.document.source();
                if source.is_char_boundary(offset) && blake3::hash(source.as_bytes()) == digest {
                    self.session.selection = Selection::caret(offset);
                    self.editor_focused = self.mode != EditorMode::Read;
                    self.session.document.break_history_group();
                    self.sync_editor();
                    return Some(if self.mode == EditorMode::Read {
                        viewport::reveal(self.session.document.block_at(offset))
                    } else {
                        operation::focus(EDITOR)
                    });
                }
                self.notification = Some("The linked file changed since the scan.".into());
            }
        }
        None
    }
}

fn link_choice<'a>(
    tokens: crate::theme::Tokens,
    title: String,
    detail: Option<String>,
    message: AppMessage,
    selected: bool,
) -> Element<'a, AppMessage> {
    let mut info = column![text(title).size(13).wrapping(text::Wrapping::None)].spacing(3);
    if let Some(detail) = detail {
        info = info.push(
            text(detail)
                .size(11)
                .color(tokens.muted)
                .wrapping(text::Wrapping::None),
        );
    }
    button(container(info).width(Fill).clip(true))
        .width(Fill)
        .padding([6, 9])
        .style(styles::button_style(
            tokens,
            if selected {
                Button::Selected
            } else {
                Button::Quiet
            },
        ))
        .on_press(message)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::application::{Pending, writing};

    async fn settle(app: &mut App, task: Task<AppMessage>) {
        use iced::futures::StreamExt;
        let mut tasks = vec![task];
        while let Some(task) = tasks.pop() {
            if let Some(mut effects) = iced_runtime::task::into_stream(task) {
                while let Some(effect) = effects.next().await {
                    if let iced_runtime::Action::Output(message) = effect {
                        tasks.push(app.update_message(message));
                    }
                }
            }
        }
    }

    async fn open_workspace(root: &Path, file: &Path) -> App {
        let (mut app, _) = App::new();
        let scan = app.open_workspace(root.to_owned());
        settle(&mut app, scan).await;
        let open = app.request(Pending::Open(Some(file.to_owned())));
        settle(&mut app, open).await;
        app
    }

    #[tokio::test]
    async fn picker_is_one_undo_step_and_link_jumps_keep_dirty_documents_open() {
        let directory = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(directory.path()).unwrap();
        std::fs::create_dir(root.join("Drafts")).unwrap();
        let entry = root.join("Drafts/Intro.md");
        let methods = root.join("Notes #2.md");
        std::fs::write(&entry, "Draft text.").unwrap();
        let target = "# Intro\n\nFirst.\n\n## Methods\n\nDetails.\n\n## Methods\n\nAlternate.\n";
        std::fs::write(&methods, target).unwrap();

        let mut app = open_workspace(&root, &entry).await;
        drop(app.update_writing(writing::Message::Details(writing::Kind::Link)));
        drop(app.update_document_links(Message::Pick));
        drop(app.update_document_links(Message::Filter("notes".into())));
        let select_file = app.update_document_links(Message::Accept);
        settle(&mut app, select_file).await;
        drop(app.update_document_links(Message::Filter("methods-1".into())));
        drop(app.update_document_links(Message::Step(1)));
        drop(app.update_document_links(Message::Accept));
        drop(app.update_writing(writing::Message::Accept));
        assert!(
            app.session
                .document
                .source()
                .starts_with("[Methods](<../Notes%20%232.md#methods-1>)")
        );
        let edited = app.session.document.source().to_owned();
        drop(app.update_message(AppMessage::Undo));
        assert_eq!(app.session.document.source(), "Draft text.");
        drop(app.update_message(AppMessage::Redo));
        assert_eq!(app.session.document.source(), edited);

        drop(app.update_message(AppMessage::SetMode(EditorMode::Read)));
        let link = "../Notes%20%232.md#methods-1";
        let open = app.open_destination(link.into());
        settle(&mut app, open).await;
        assert!(app.overlay.is_none());
        assert_eq!(app.session.path.as_ref(), Some(&methods));
        assert_eq!(app.session.document.source(), target);
        assert_eq!(app.mode, EditorMode::Read);
        assert_eq!(
            app.session.selection,
            Selection::caret(target.rfind("## Methods").unwrap())
        );
        assert!(
            app.background
                .dirty()
                .any(|session| session.path.as_ref() == Some(&entry)
                    && session.document.source() == edited)
        );

        let added = "An unsaved introduction.\n\n";
        app.session.document.replace(0..0, added).unwrap();
        app.sync_editor();
        app.changed();
        let return_to_entry = app.request(Pending::Open(Some(entry.clone())));
        settle(&mut app, return_to_entry).await;
        assert_eq!(app.session.document.source(), edited);
        assert!(app.session.document.can_undo());
        let open = app.open_destination(link.into());
        settle(&mut app, open).await;
        assert_eq!(app.session.path.as_ref(), Some(&methods));
        assert_eq!(app.session.document.source(), format!("{added}{target}"));
        assert!(app.session.dirty && app.session.document.can_undo());
        assert_eq!(
            app.session.selection,
            Selection::caret(added.len() + target.rfind("## Methods").unwrap())
        );
    }

    #[tokio::test]
    async fn backlinks_jump_to_live_results_and_drop_a_stale_offset() {
        let directory = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(directory.path()).unwrap();
        let draft = root.join("Draft.md");
        let notes = root.join("Notes.md");
        std::fs::write(&draft, "# Draft\n\nText.\n").unwrap();
        let source = "# Notes\n\nReference to [draft](Draft.md).\n";
        std::fs::write(&notes, source).unwrap();

        let mut app = open_workspace(&root, &draft).await;
        let scan = app.update_document_links(Message::ShowBacklinks);
        settle(&mut app, scan).await;
        assert_eq!(app.overlay, Some(Overlay::Backlinks));
        let open = app.update_document_links(Message::OpenBacklink(None));
        settle(&mut app, open).await;
        assert_eq!(app.session.path.as_ref(), Some(&notes));
        // Live projection canonicalizes the caret out of hidden Markdown.
        assert_eq!(
            app.session.selection,
            app.projection
                .normalize_caret(Selection::caret(source.find("[draft]").unwrap()))
        );

        let back = app.request(Pending::Open(Some(draft.clone())));
        settle(&mut app, back).await;
        let scan = app.update_document_links(Message::ShowBacklinks);
        settle(&mut app, scan).await;
        std::fs::write(&notes, "Another writer replaced this file.").unwrap();
        let open = app.update_document_links(Message::OpenBacklink(None));
        settle(&mut app, open).await;
        assert_eq!(app.session.path.as_ref(), Some(&notes));
        assert_eq!(
            app.session.document.source(),
            "Another writer replaced this file."
        );
        assert!(
            app.notification
                .as_deref()
                .is_some_and(|notice| notice.contains("changed since"))
        );
    }

    #[tokio::test]
    async fn backlink_resumes_dirty_notes_without_replacing_their_selection() {
        let directory = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(directory.path()).unwrap();
        let draft = root.join("Draft.md");
        let notes = root.join("Notes.md");
        std::fs::write(&draft, "# Draft\n").unwrap();
        std::fs::write(&notes, "# Notes\n\n[Draft](Draft.md).\n").unwrap();
        let mut app = open_workspace(&root, &notes).await;
        let extra = "Some unsaved writing.\n\n";
        app.session.document.replace(0..0, extra).unwrap();
        app.session.selection = Selection { anchor: 8, head: 4 };
        app.sync_editor();
        app.changed();
        let source = app.session.document.source().to_owned();
        let selection = app.session.selection;
        let open = app.request(Pending::Open(Some(draft)));
        settle(&mut app, open).await;

        let scan = app.update_document_links(Message::ShowBacklinks);
        settle(&mut app, scan).await;
        let follow = app.update_document_links(Message::OpenBacklink(None));
        settle(&mut app, follow).await;
        assert_eq!(app.session.path.as_ref(), Some(&notes));
        assert_eq!(app.session.document.source(), source);
        assert_eq!(app.session.selection, selection);
        assert!(app.session.dirty && app.session.document.can_undo());
    }

    #[tokio::test]
    async fn draft_uses_its_first_save_folder_to_suggest_and_follow_relative_links() {
        let directory = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(directory.path()).unwrap();
        let drafts = root.join("Drafts");
        std::fs::create_dir(&drafts).unwrap();
        let entry = drafts.join("Intro.md");
        let notes = root.join("Notes.md");
        std::fs::write(&entry, "Intro").unwrap();
        std::fs::write(&notes, "# Notes\n").unwrap();
        let mut app = open_workspace(&root, &entry).await;
        let new = app.request(Pending::New);
        settle(&mut app, new).await;
        assert_eq!(app.session.folder.as_ref(), Some(&drafts));
        assert!(app.session.path.is_none() && app.link_picker_available());
        assert!(!app.backlinks_available());

        drop(app.update_writing(writing::Message::Details(writing::Kind::Link)));
        drop(app.update_document_links(Message::Pick));
        drop(app.update_document_links(Message::Filter("notes".into())));
        let choose = app.update_document_links(Message::Accept);
        settle(&mut app, choose).await;
        drop(app.update_document_links(Message::Accept));
        drop(app.update_writing(writing::Message::Accept));
        assert_eq!(app.session.document.source(), "[Notes](<../Notes.md>)");
        let open = app.open_destination("../Notes.md".into());
        settle(&mut app, open).await;
        assert_eq!(app.session.path.as_ref(), Some(&notes));
        assert!(app.background.dirty().any(|session| session.path.is_none()
            && session.document.source() == "[Notes](<../Notes.md>)"));
    }
}
