//! Find text in the current folder and navigate through the normal file guards.

use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::advanced::widget;
use iced::keyboard::key::Named;
use iced::widget::{button, column, container, operation, row, scrollable, text, text_input};
use iced::{Alignment, Element, Fill, Task};

use super::{App, EDITOR, Message as AppMessage, OpenPosition, Overlay, Pending};
use crate::app::icon::Icon;
use crate::document::selection::Selection;
use crate::editor::outline_filter::FilterInput;
use crate::io::folder_search::{self, Report, Target};
use crate::io::storage::{FileError, LoadedFile};
use crate::theme::styles::{self, Button};

const INPUT: &str = "folder-search-query";
const RESULTS: &str = "folder-search-results";
const ROW_HEIGHT: f32 = 45.0;
const MAX_OPEN_SOURCE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub enum Message {
    Open,
    RememberFocus(u64, widget::Id),
    Query(String),
    Run,
    Complete(Result<Report, FileError>),
    Step(i32),
    Accept,
    Choose { revision: u64, index: usize },
    Opened(Result<(LoadedFile, Range<usize>), FileError>),
    Cancel,
}

#[derive(Default)]
pub(super) struct State {
    root: Option<PathBuf>,
    query: String,
    results: Option<Report>,
    error: Option<String>,
    selected: usize,
    previous_focus: Option<widget::Id>,
    picker: u64,
    revision: u64,
    running: Option<Arc<AtomicBool>>,
    queued: bool,
}

impl State {
    pub(super) fn clear(&mut self) {
        self.invalidate();
        *self = Self {
            // Keep the worker slot while cancellation completes, so opening and
            // closing repeatedly cannot enqueue unbounded disk work.
            running: self.running.take(),
            picker: self.picker.wrapping_add(1),
            revision: self.revision,
            ..Self::default()
        };
    }

    fn invalidate(&mut self) {
        if let Some(running) = &self.running {
            running.store(true, Ordering::Relaxed);
        }
        self.revision = self.revision.wrapping_add(1);
        self.queued = false;
        self.results = None;
        self.error = None;
        self.selected = 0;
    }

    fn searching(&self) -> bool {
        self.queued
            || self
                .running
                .as_ref()
                .is_some_and(|active| !active.load(Ordering::Relaxed))
    }
}

impl App {
    pub(super) fn cancel_folder_search(&mut self) -> Task<AppMessage> {
        if self.overlay != Some(Overlay::FolderSearch) {
            return Task::none();
        }
        let previous = self
            .folder_search
            .previous_focus
            .take()
            .unwrap_or_else(|| widget::Id::from(EDITOR));
        self.folder_search.clear();
        self.overlay = None;
        self.preserve_viewport_generation =
            Some(self.session.document.collaboration().generation());
        operation::focus(previous)
    }

    pub(super) fn update_folder_search(&mut self, message: Message) -> Task<AppMessage> {
        let message = match message {
            Message::Complete(result) => return self.finish_folder_search(result),
            Message::Opened(result) => {
                return match result {
                    Ok((file, range)) => self.complete_document_load(
                        Ok(file),
                        Some(OpenPosition::Search(range)),
                        true,
                    ),
                    Err(error) => self.complete_document_load(Err(error), None, true),
                };
            }
            Message::Open => return self.show_folder_search(),
            other => other,
        };
        if self.overlay != Some(Overlay::FolderSearch) {
            return Task::none();
        }
        if self.folder_search.root.as_deref() != self.folder.root() {
            return self.cancel_folder_search();
        }
        match message {
            Message::RememberFocus(picker, id) if picker == self.folder_search.picker => {
                self.folder_search.previous_focus = Some(id);
            }
            Message::Query(query) => {
                self.folder_search.invalidate();
                self.folder_search.query = query.chars().take(256).collect();
                return operation::snap_to(RESULTS, scrollable::RelativeOffset::START);
            }
            Message::Run => return self.run_folder_search(),
            Message::Step(amount) => {
                if let Some(report) = &self.folder_search.results {
                    self.folder_search.selected = self
                        .folder_search
                        .selected
                        .saturating_add_signed(amount as isize)
                        .min(report.hits.len().saturating_sub(1));
                    return reveal_result(self.folder_search.selected);
                }
            }
            Message::Accept => {
                if self
                    .folder_search
                    .results
                    .as_ref()
                    .is_some_and(|report| report.hits.get(self.folder_search.selected).is_some())
                {
                    return self.choose_folder_result(self.folder_search.selected);
                }
                if !self.folder_search.searching() {
                    return self.run_folder_search();
                }
            }
            Message::Choose { revision, index } if revision == self.folder_search.revision => {
                return self.choose_folder_result(index);
            }
            Message::Cancel => return self.cancel_folder_search(),
            _ => {}
        }
        Task::none()
    }

    fn show_folder_search(&mut self) -> Task<AppMessage> {
        if self.overlay == Some(Overlay::FolderSearch) {
            return self.cancel_folder_search();
        }
        if self.busy || self.loading || !matches!(self.overlay, None | Some(Overlay::Chrome)) {
            return Task::none();
        }
        let Some(root) = self.folder.root() else {
            return Task::none();
        };
        self.folder_search.clear();
        self.folder_search.root = Some(root.to_owned());
        self.chrome.menu = None;
        self.overlay = Some(Overlay::FolderSearch);
        self.preserve_viewport_generation =
            Some(self.session.document.collaboration().generation());
        let picker = self.folder_search.picker;
        widget::operate(widget::operation::focusable::find_focused())
            .map(move |id| AppMessage::FolderSearch(Message::RememberFocus(picker, id)))
            .chain(operation::focus(INPUT))
    }

    fn run_folder_search(&mut self) -> Task<AppMessage> {
        if self.folder_search.query.trim().is_empty() {
            return Task::none();
        }
        if let Some(running) = &self.folder_search.running {
            if running.load(Ordering::Relaxed) {
                self.folder_search.queued = true;
            }
            return Task::none();
        }
        let Some(root) = self.folder_search.root.clone() else {
            return Task::none();
        };
        self.folder_search.invalidate();
        let paths: Vec<_> = self
            .folder
            .files()
            .filter(|path| crate::io::folder::is_markdown(path))
            .take(folder_search::MAX_FILES + 1)
            .map(Path::to_owned)
            .collect();
        let mut known: HashSet<_> = paths
            .iter()
            .take(folder_search::MAX_FILES)
            .map(PathBuf::as_path)
            .collect();
        let mut remaining = MAX_OPEN_SOURCE_BYTES;
        let open_sources = std::iter::once(&self.session)
            .chain(self.background.dirty())
            .filter_map(|session| {
                let path = session.path().map(|value| value.as_path())?;
                if !known.remove(path) {
                    return None;
                }
                let source = session.document.source();
                let snapshot = (source.len() <= remaining).then(|| {
                    remaining -= source.len();
                    source.to_owned()
                });
                Some((path.to_owned(), snapshot))
            })
            .collect();
        let request = folder_search::Request {
            root,
            paths,
            open_sources,
            query: self.folder_search.query.clone(),
            incomplete_snapshot: self.folder.incomplete(),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        self.folder_search.running = Some(Arc::clone(&cancelled));
        Task::perform(folder_search::search(request, cancelled), |result| {
            AppMessage::FolderSearch(Message::Complete(result))
        })
    }

    fn finish_folder_search(&mut self, result: Result<Report, FileError>) -> Task<AppMessage> {
        let Some(cancelled) = self.folder_search.running.take() else {
            return Task::none();
        };
        if self.overlay != Some(Overlay::FolderSearch) {
            return Task::none();
        }
        if self.folder_search.root.as_deref() != self.folder.root() {
            return self.cancel_folder_search();
        }
        if self.folder_search.queued {
            self.folder_search.queued = false;
            return self.run_folder_search();
        }
        if !cancelled.load(Ordering::Relaxed) {
            match result {
                Ok(report) => self.folder_search.results = Some(report),
                Err(error) => self.folder_search.error = Some(error.to_string()),
            }
        }
        operation::snap_to(RESULTS, scrollable::RelativeOffset::START)
    }

    fn choose_folder_result(&mut self, index: usize) -> Task<AppMessage> {
        if self.busy || self.loading {
            return Task::none();
        }
        let Some(root) = self.folder_search.root.clone() else {
            return Task::none();
        };
        let Some(target) = self
            .folder_search
            .results
            .as_ref()
            .and_then(|report| report.hits.get(index))
            .map(|hit| hit.target.clone())
        else {
            return Task::none();
        };
        if !self.folder.has_file(&target.path) {
            self.folder_search.error = Some(folder_search::changed().to_string());
            return Task::none();
        }
        let active = self.session.path() == Some(&target.path);
        let stable = if active {
            let Some(range) = target.range_in(self.session.document.source()) else {
                self.folder_search.error = Some(folder_search::changed().to_string());
                return Task::none();
            };
            Some(self.session.document.collaboration().selection(Selection {
                anchor: range.start,
                head: range.end,
            }))
        } else {
            None
        };
        let query = self.folder_search.query.clone();
        if !self.commit_cell() {
            return Task::none();
        }
        let focus = self.cancel_folder_search();
        self.clear_grid();
        if let Some(stable) = stable {
            let Some(selection) = self
                .session
                .document
                .collaboration()
                .resolve_selection(&stable)
            else {
                self.notification = Some(folder_search::changed().to_string());
                return focus;
            };
            if self.session.document.source().get(selection.range()) != Some(query.as_str()) {
                self.notification = Some(folder_search::changed().to_string());
                return focus;
            }
            return self.jump_to_folder_match(selection.range());
        }
        let open = self.request(Pending::FolderMatch { root, target });
        Task::batch([focus, open])
    }

    pub(super) fn open_folder_match(&mut self, root: PathBuf, target: Target) -> Task<AppMessage> {
        if self.folder.root() != Some(root.as_path()) || !self.folder.has_file(&target.path) {
            self.notification = Some(folder_search::changed().to_string());
            return Task::none();
        }
        let dirty_session = self
            .background
            .dirty()
            .find(|session| session.path() == Some(&target.path))
            .map(|session| (session.id(), target.range_in(session.document.source())));
        if let Some((id, range)) = dirty_session {
            let Some(range) = range else {
                self.notification = Some(folder_search::changed().to_string());
                return Task::none();
            };
            let Some(parked) = self.background.take_unsaved(id) else {
                return Task::none();
            };
            return self.resume_at(parked, Some(OpenPosition::Search(range)), true);
        }
        self.busy = true;
        self.loading = true;
        Task::perform(target.open(root), |result| {
            AppMessage::FolderSearch(Message::Opened(result))
        })
    }

    pub(super) fn jump_to_folder_match(&mut self, range: Range<usize>) -> Task<AppMessage> {
        self.return_to_document();
        self.session.document.break_history_group();
        self.preserve_viewport_generation = None;
        self.editor_focused = true;
        self.reveal_source_match(range)
    }

    pub(super) fn folder_search_view(&self, height: f32) -> Element<'_, AppMessage> {
        let state = &self.folder_search;
        let tokens = self.resolved_theme.tokens;
        let input = text_input("Find text in Markdown files…", &state.query)
            .id(INPUT)
            .on_input(|query| AppMessage::FolderSearch(Message::Query(query)))
            .on_submit(AppMessage::FolderSearch(Message::Accept))
            .padding(10)
            .size(14)
            .style(styles::input(tokens));
        let input = FilterInput::new(input, |key| {
            Some(AppMessage::FolderSearch(match key {
                Named::ArrowUp => Message::Step(-1),
                Named::ArrowDown => Message::Step(1),
                Named::Escape => Message::Cancel,
                _ => return None,
            }))
        });
        let search = AppMessage::FolderSearch(Message::Run);
        let search_button = button(text("Search").size(13))
            .padding([10, 14])
            .on_press_maybe(
                (!state.query.trim().is_empty() && !state.searching()).then_some(search),
            )
            .style(styles::button_style(tokens, Button::Primary));
        let mut content = column![
            row![
                text("Find in Folder").size(15),
                container(
                    text(
                        state
                            .root
                            .as_deref()
                            .map(super::folder::name)
                            .unwrap_or_default()
                    )
                    .size(12)
                    .color(tokens.muted)
                    .wrapping(text::Wrapping::None),
                )
                .width(Fill)
                .align_right(Fill)
                .clip(true),
                self.icon_button(
                    Icon::Close,
                    "Close Find in Folder",
                    AppMessage::FolderSearch(Message::Cancel),
                    false
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![input, search_button]
                .spacing(8)
                .align_y(Alignment::Center),
        ]
        .spacing(8);
        if let Some(report) = &state.results {
            let mut results = column![];
            for (index, hit) in report.hits.iter().enumerate() {
                let path = state
                    .root
                    .as_ref()
                    .and_then(|root| hit.target.path.strip_prefix(root).ok())
                    .unwrap_or(&hit.target.path);
                let label = format!("{}:{}", path.display(), hit.line);
                results = results.push(
                    button(
                        container(
                            column![
                                text(&hit.excerpt).size(13).wrapping(text::Wrapping::None),
                                text(label)
                                    .size(11)
                                    .color(tokens.muted)
                                    .wrapping(text::Wrapping::None),
                            ]
                            .spacing(2),
                        )
                        .width(Fill)
                        .clip(true),
                    )
                    .padding([5, 10])
                    .width(Fill)
                    .height(ROW_HEIGHT)
                    .on_press(AppMessage::FolderSearch(Message::Choose {
                        revision: state.revision,
                        index,
                    }))
                    .style(styles::button_style(
                        tokens,
                        if state.selected == index {
                            Button::Selected
                        } else {
                            Button::Quiet
                        },
                    )),
                );
            }
            if !report.hits.is_empty() {
                content = content.push(
                    container(
                        scrollable(results)
                            .id(RESULTS)
                            .height(iced::Shrink)
                            .style(styles::scrollbar(tokens)),
                    )
                    .max_height((height - 153.0).clamp(48.0, 320.0)),
                );
            }
        }
        if let Some(status) = status(state) {
            content = content.push(text(status).size(12).color(tokens.muted));
        }
        container(content)
            .padding(14)
            .width(Fill)
            .max_width(560)
            .style(styles::context_menu(tokens))
            .into()
    }
}

fn status(state: &State) -> Option<String> {
    if let Some(error) = &state.error {
        return Some(error.clone());
    }
    if state.searching() {
        return Some("Searching…".into());
    }
    let report = state.results.as_ref()?;
    let mut status = match report.hits.len() {
        0 => "No matches".into(),
        1 => "1 match".into(),
        count => format!("{count} matches"),
    };
    if report.limited {
        status.push_str(" · Partial results");
    }
    if report.skipped > 0 {
        status.push_str(&format!(" · {} files skipped", report.skipped));
    }
    Some(status)
}

fn reveal_result(index: usize) -> Task<AppMessage> {
    struct Reveal(f32);

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
                let top = self.0;
                self.0 = translation
                    .y
                    .clamp((top + ROW_HEIGHT - bounds.height).min(top), top);
            }
        }

        fn finish(&self) -> widget::operation::Outcome<f32> {
            widget::operation::Outcome::Some(self.0.max(0.0))
        }
    }

    widget::operate(Reveal(index as f32 * ROW_HEIGHT)).then(|offset| {
        operation::scroll_to(
            RESULTS,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(offset),
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::folder::{Entry, Snapshot};
    use iced::widget::text_editor::{Action, Edit};

    fn app(root: &Path, files: &[&Path]) -> App {
        let (mut app, tasks) = App::new();
        drop(tasks);
        drop(
            app.update_folder(super::super::folder::Message::Scanned {
                generation: 0,
                result: Ok(Snapshot {
                    root: root.to_owned(),
                    entries: files
                        .iter()
                        .map(|path| Entry {
                            path: path.to_path_buf(),
                            is_directory: false,
                        })
                        .collect(),
                    limited: false,
                    warnings: Vec::new(),
                }),
            }),
        );
        app
    }

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

    async fn query(app: &mut App, phrase: &str) {
        drop(app.update_folder_search(Message::Open));
        drop(app.update_folder_search(Message::Query(phrase.into())));
        let search = app.update_folder_search(Message::Accept);
        settle(app, search).await;
    }

    #[tokio::test]
    async fn switches_files_and_parks_unsaved_work_before_jumping_to_match() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let active = root.join("Active.md");
        let target = root.join("Target.md");
        std::fs::write(&active, "saved").unwrap();
        std::fs::write(&target, "# Heading\n\nnotes before 🦀 café\r\n").unwrap();
        let mut app = app(&root, &[&active, &target]);
        app.set_document("saved".into(), Some(active.clone()), "Active.md".into());
        drop(app.update_message(AppMessage::Edit(Action::Edit(Edit::Insert('!')))));
        let unsaved = app.session.document.source().to_owned();

        query(&mut app, "🦀 café").await;
        assert_eq!(app.folder_search.results.as_ref().unwrap().hits.len(), 1);
        let open = app.update_folder_search(Message::Accept);
        settle(&mut app, open).await;
        assert_eq!(
            app.background
                .dirty_under(&active)
                .unwrap()
                .document
                .source(),
            unsaved
        );
        assert_eq!(
            app.session.path().map(|value| value.as_path()),
            Some(target.as_path())
        );
        assert_eq!(
            &app.session.document.source()[app.session.selection.range()],
            "🦀 café"
        );
        assert_eq!(std::fs::read_to_string(active).unwrap(), "saved");
    }

    #[tokio::test]
    async fn selects_unsaved_active_matches_and_rejects_changed_disk_results() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let active = root.join("Active.md");
        let target = root.join("Target.md");
        std::fs::write(&active, "old file").unwrap();
        std::fs::write(&target, "disk only").unwrap();
        let mut app = app(&root, &[&active, &target]);
        app.set_document("active 🦀".into(), Some(active.clone()), "Active.md".into());
        app.session.saved_source = "old file".into();
        app.session.dirty = true;

        query(&mut app, "🦀").await;
        drop(app.update_folder_search(Message::Accept));
        assert_eq!(
            app.session.path().map(|value| value.as_path()),
            Some(active.as_path())
        );
        assert_eq!(
            &app.session.document.source()[app.session.selection.range()],
            "🦀"
        );
        assert!(app.session.dirty);

        query(&mut app, "disk").await;
        std::fs::write(&target, "different disk text").unwrap();
        let open = app.update_folder_search(Message::Accept);
        settle(&mut app, open).await;
        assert_eq!(
            app.session.path().map(|value| value.as_path()),
            Some(active.as_path())
        );
        assert_eq!(app.session.document.source(), "active 🦀");
        assert!(
            app.notification
                .as_ref()
                .unwrap()
                .contains("changed since the search")
        );
    }

    #[tokio::test]
    async fn matches_dirty_background_source_and_resumes_its_undo_history() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let active = root.join("Active.md");
        let target = root.join("Target.md");
        std::fs::write(&active, "active disk").unwrap();
        std::fs::write(&target, "target disk text").unwrap();
        let mut app = app(&root, &[&active, &target]);
        app.set_document(
            "target disk text".into(),
            Some(target.clone()),
            "Target.md".into(),
        );
        drop(app.update_message(AppMessage::Edit(Action::Edit(Edit::Insert('🦀')))));
        let original_session = app.session.id();
        app.set_document("active disk".into(), Some(active), "Active.md".into());

        query(&mut app, "🦀").await;
        assert_eq!(app.folder_search.results.as_ref().unwrap().hits.len(), 1);
        let open = app.update_folder_search(Message::Accept);
        settle(&mut app, open).await;
        assert_eq!(app.session.id(), original_session);
        assert_eq!(
            &app.session.document.source()[app.session.selection.range()],
            "🦀"
        );
        assert!(app.session.dirty && app.session.document.can_undo());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "target disk text");
    }

    #[tokio::test]
    async fn query_change_replaces_inflight_search_without_publishing_stale_hits() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let target = root.join("Notes.md");
        std::fs::write(&target, "old first\nnew second\n").unwrap();
        let mut app = app(&root, &[&target]);
        drop(app.update_folder_search(Message::Open));
        drop(app.update_folder_search(Message::Query("old".into())));
        let old = app.update_folder_search(Message::Accept);
        drop(app.update_folder_search(Message::Query("new".into())));
        drop(app.update_folder_search(Message::Accept));
        assert!(app.folder_search.queued);
        settle(&mut app, old).await;
        let report = app.folder_search.results.as_ref().unwrap();
        assert_eq!(report.hits.len(), 1);
        assert_eq!(report.hits[0].excerpt, "new second");
        assert!(app.folder_search.running.is_none());
    }
}
