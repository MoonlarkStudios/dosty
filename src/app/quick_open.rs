//! The switcher: one summoned surface for moving between documents.
//!
//! With an empty query it shows unsaved documents, recently active documents,
//! the open place's folders to step through, and the other places. A query
//! searches documents by name and path across the whole library, the open place
//! first. Everything it holds is derived when it opens and discarded when it
//! closes; other places are scanned on a worker only while it is open.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::advanced::widget;
use iced::keyboard::key::Named;
use iced::widget::{
    button, column, container, operation, row, scrollable, text, text_input, tooltip,
};
use iced::{Alignment, Element, Fill, Padding, Size, Task};

use super::session::SessionId;
use super::{App, EDITOR, Message as AppMessage, Overlay, Pending, chrome};
use crate::app::icon::{self, Icon};
use crate::editor::outline_filter::FilterInput;
use crate::io::folder as discovery;
use crate::theme::styles::{self, Button};

pub(super) const INPUT: &str = "quick-open-query";
const RESULTS: &str = "quick-open-results";
const MAX_RESULTS: usize = 100;
const MAX_RECENT: usize = 6;
/// Documents indexed from places other than the open one, across all of them.
const MAX_LIBRARY_DOCUMENTS: usize = 65_536;
const RECENT: &str = "Recent";
/// Height of a row that shows where its document is, and of one that does not.
const RESULT_HEIGHT: f32 = 48.0;
const PLAIN_RESULT_HEIGHT: f32 = 34.0;
const HEADING_HEIGHT: f32 = 28.0;

#[derive(Debug, Clone)]
pub enum Message {
    Open,
    Focus {
        request: u64,
        previous: Option<widget::Id>,
    },
    /// Documents found in the other places, scanned after the switcher opened.
    Library {
        request: u64,
        snapshots: Vec<discovery::Snapshot>,
    },
    Query(String),
    Step(i32),
    Hover(usize),
    Accept,
    /// Activate the row at this position.
    Pick(usize),
    /// Open a document of the open place.
    Choose(PathBuf),
    /// Step into the selected folder, or out of the one being browsed.
    StepIn,
    StepOut,
    Cancel,
}

struct Candidate {
    recency: usize,
    path: PathBuf,
    title: String,
    /// The folder that holds the document, as shown beside its title.
    location: String,
    /// `None` for the open place.
    place: Option<PathBuf>,
    name_lower: String,
    path_lower: String,
}

/// A document open in the background with unsaved changes.
struct Unsaved {
    session: SessionId,
    title: String,
    location: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Target {
    File(PathBuf),
    Session(SessionId),
    Folder(PathBuf),
    Place(PathBuf),
}

pub(super) struct Row {
    pub(super) target: Target,
    pub(super) title: String,
    pub(super) location: String,
    pub(super) unsaved: bool,
    /// Set on the first row of a section.
    pub(super) heading: Option<String>,
}

#[derive(Default)]
pub(super) struct State {
    root: Option<PathBuf>,
    /// The document on the page, left out of Recent.
    active: Option<PathBuf>,
    candidates: Vec<Candidate>,
    folders: Vec<PathBuf>,
    unsaved: Vec<Unsaved>,
    places: Vec<PathBuf>,
    /// The folder being browsed while the query is empty; `None` is the top.
    location: Option<PathBuf>,
    query: String,
    rows: Vec<Row>,
    matches: usize,
    selected: usize,
    previous: Option<widget::Id>,
    request: u64,
    /// A place was chosen here, so the open place changing is expected.
    switching: bool,
    cancel_scan: Option<Arc<AtomicBool>>,
}

/// A document's name as shown to the writer: without its Markdown extension.
pub(super) fn title(path: &Path) -> String {
    let name = if discovery::is_markdown(path) {
        path.file_stem()
    } else {
        path.file_name()
    };
    name.map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn portable(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

impl State {
    pub(super) fn clear(&mut self) {
        if let Some(cancel) = &self.cancel_scan {
            cancel.store(true, Ordering::Relaxed);
        }
        let request = self.request.wrapping_add(1);
        *self = Self {
            request,
            ..Self::default()
        };
    }

    fn candidate(path: &Path, root: &Path, other: bool, recency: usize) -> Option<Candidate> {
        let relative = path.strip_prefix(root).ok()?;
        let name = path.file_name()?.to_string_lossy().into_owned();
        let place = super::folder::name(root);
        let parent = relative.parent().map(portable).unwrap_or_default();
        let relative = portable(relative);
        Some(Candidate {
            recency,
            path: path.to_owned(),
            title: title(path),
            location: match (other, parent.is_empty()) {
                (_, true) => place.clone(),
                (false, false) => parent,
                (true, false) => format!("{place} / {parent}"),
            },
            name_lower: name.to_lowercase(),
            path_lower: if other {
                format!("{}/{}", place.to_lowercase(), relative.to_lowercase())
            } else {
                relative.to_lowercase()
            },
            place: other.then(|| root.to_owned()),
        })
    }

    fn sort(&mut self) {
        self.candidates.sort_by(|a, b| {
            a.recency
                .cmp(&b.recency)
                .then_with(|| a.place.is_some().cmp(&b.place.is_some()))
                .then_with(|| a.name_lower.cmp(&b.name_lower))
                .then_with(|| a.path_lower.cmp(&b.path_lower))
        });
    }

    /// Index the open place. Documents in `recent` (most recent first) lead
    /// the list and break ties within each match rank. Documents already
    /// indexed from other places are kept.
    fn index(&mut self, root: &Path, entries: &[discovery::Entry], recent: &[PathBuf]) {
        let recency: std::collections::HashMap<&Path, usize> = recent
            .iter()
            .enumerate()
            .map(|(rank, path)| (path.as_path(), rank))
            .collect();
        let recency = |path: &Path| recency.get(path).copied().unwrap_or(usize::MAX);
        self.candidates
            .retain(|candidate| candidate.place.is_some());
        self.candidates.extend(
            entries
                .iter()
                .filter(|entry| !entry.is_directory)
                .filter_map(|entry| {
                    Self::candidate(&entry.path, root, false, recency(&entry.path))
                }),
        );
        self.folders = entries
            .iter()
            .filter(|entry| entry.is_directory)
            .map(|entry| entry.path.clone())
            .collect();
        self.sort();
        self.list();
    }

    /// Add the documents of the other places, within a shared bound.
    fn index_library(&mut self, snapshots: &[discovery::Snapshot]) {
        self.candidates
            .retain(|candidate| candidate.place.is_none());
        let documents = snapshots.iter().flat_map(|snapshot| {
            snapshot
                .entries
                .iter()
                .filter(|entry| !entry.is_directory)
                .filter_map(|entry| Self::candidate(&entry.path, &snapshot.root, true, usize::MAX))
        });
        self.candidates
            .extend(documents.take(MAX_LIBRARY_DOCUMENTS));
        self.sort();
        self.list();
    }

    /// Rebuild the rows for the current query, or for the folder being browsed.
    fn list(&mut self) {
        let query = self.query.trim().to_lowercase();
        let query = if cfg!(target_os = "windows") {
            query.replace('\\', "/")
        } else {
            query
        };
        self.rows.clear();
        self.matches = 0;
        if query.is_empty() {
            self.browse();
        } else {
            self.search(&query);
        }
        self.matches = self.matches.max(self.rows.len());
        self.rows.truncate(MAX_RESULTS);
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    fn search(&mut self, query: &str) {
        let terms: Vec<_> = query.split_whitespace().collect();
        self.rows.extend(
            self.unsaved
                .iter()
                .filter(|unsaved| {
                    let title = unsaved.title.to_lowercase();
                    terms.iter().all(|term| title.contains(term))
                })
                .map(Unsaved::row),
        );
        let mut ranked: [Vec<usize>; 3] = Default::default();
        for (index, candidate) in self.candidates.iter().enumerate() {
            if !terms.iter().all(|term| candidate.path_lower.contains(term)) {
                continue;
            }
            self.matches += 1;
            let rank = if candidate.name_lower == query {
                0
            } else if candidate.name_lower.starts_with(query) {
                1
            } else {
                2
            };
            if ranked[rank].len() < MAX_RESULTS {
                ranked[rank].push(index);
            }
        }
        self.matches += self.rows.len();
        self.rows.extend(
            ranked
                .into_iter()
                .flatten()
                .map(|index| self.candidates[index].row()),
        );
    }

    fn browse(&mut self) {
        if let Some(folder) = self.location.clone() {
            let heading = self
                .root
                .as_deref()
                .and_then(|root| folder.strip_prefix(root).ok())
                .map_or_else(|| portable(&folder), portable);
            self.push_children(&folder, heading);
            return;
        }
        let mut section = Vec::from_iter(self.unsaved.iter().map(Unsaved::row));
        self.push_section("Unsaved", &mut section);
        let mut section: Vec<_> = self
            .candidates
            .iter()
            .filter(|candidate| candidate.recency != usize::MAX)
            .filter(|candidate| Some(&candidate.path) != self.active.as_ref())
            .take(MAX_RECENT)
            .map(Candidate::row)
            .collect();
        self.push_section(RECENT, &mut section);
        if let Some(root) = self.root.clone() {
            self.push_children(&root, super::folder::name(&root));
        }
        let mut section: Vec<_> = self
            .places
            .iter()
            .map(|place| Row {
                target: Target::Place(place.clone()),
                title: super::workspace::place_label(place, &self.places),
                location: String::new(),
                unsaved: false,
                heading: None,
            })
            .collect();
        self.push_section("Places", &mut section);
    }

    fn push_section(&mut self, heading: impl Into<String>, section: &mut Vec<Row>) {
        if let Some(first) = section.first_mut() {
            first.heading = Some(heading.into());
        }
        self.rows.append(section);
    }

    /// The folders, then the documents, directly inside `parent`.
    fn push_children(&mut self, parent: &Path, heading: String) {
        let mut folders: Vec<_> = self
            .folders
            .iter()
            .filter(|folder| folder.parent() == Some(parent))
            .map(|folder| Row {
                target: Target::Folder(folder.clone()),
                title: super::folder::name(folder),
                location: String::new(),
                unsaved: false,
                heading: None,
            })
            .collect();
        folders.sort_by_key(|row| row.title.to_lowercase());
        let mut documents: Vec<_> = self
            .candidates
            .iter()
            .filter(|candidate| candidate.place.is_none())
            .filter(|candidate| candidate.path.parent() == Some(parent))
            .map(|candidate| Row {
                location: String::new(),
                ..candidate.row()
            })
            .collect();
        documents.sort_by_key(|row| row.title.to_lowercase());
        folders.append(&mut documents);
        self.push_section(heading, &mut folders);
    }

    fn select(&mut self, target: Option<&Target>) {
        self.selected = target
            .and_then(|target| self.rows.iter().position(|row| &row.target == target))
            .unwrap_or(0);
    }

    fn selected_target(&self) -> Option<&Target> {
        self.rows.get(self.selected).map(|row| &row.target)
    }

    /// Vertical extent of the selected row, below the rows and headings
    /// before it.
    fn selected_extent(&self) -> (f32, f32) {
        let before = self.rows.iter().take(self.selected);
        let top: f32 = before.map(|row| row.heading_height() + row.height()).sum();
        self.rows
            .get(self.selected)
            .map_or((top, 0.0), |row| (top + row.heading_height(), row.height()))
    }
}

impl Row {
    fn height(&self) -> f32 {
        if self.location.is_empty() {
            PLAIN_RESULT_HEIGHT
        } else {
            RESULT_HEIGHT
        }
    }

    fn heading_height(&self) -> f32 {
        if self.heading.is_some() {
            HEADING_HEIGHT
        } else {
            0.0
        }
    }
}

impl Candidate {
    fn row(&self) -> Row {
        Row {
            target: Target::File(self.path.clone()),
            title: self.title.clone(),
            location: self.location.clone(),
            unsaved: false,
            heading: None,
        }
    }
}

impl Unsaved {
    fn row(&self) -> Row {
        Row {
            target: Target::Session(self.session),
            title: self.title.clone(),
            location: self.location.clone(),
            unsaved: true,
            heading: None,
        }
    }
}

impl App {
    /// Derive everything the switcher shows from the application as it stands.
    fn index_quick_open(&mut self) {
        let root = self.folder.root().map(Path::to_owned);
        self.quick_open.active = self.session.path.clone();
        self.quick_open.unsaved = self
            .background
            .dirty()
            .map(|session| Unsaved {
                session: session.id(),
                title: session
                    .path
                    .as_deref()
                    .map_or_else(|| title(Path::new(&session.display_name)), title),
                location: match (session.path.as_deref(), session.folder.as_deref()) {
                    (Some(path), _) => path.parent().map(super::folder::name).unwrap_or_default(),
                    (None, Some(folder)) => format!("Draft in {}", super::folder::name(folder)),
                    (None, None) => "Not saved yet".into(),
                },
            })
            .collect();
        self.quick_open.places = self
            .workspace
            .places()
            .iter()
            .filter(|place| Some(place.as_path()) != root.as_deref())
            .cloned()
            .collect();
        self.quick_open.root.clone_from(&root);
        match root {
            Some(root) => {
                let recent = self.workspace.recent_documents();
                self.quick_open.index(&root, self.folder.entries(), &recent);
            }
            None => self.quick_open.index(Path::new(""), &[], &[]),
        }
    }

    /// Scan the other places on a worker, one at a time, within a shared bound.
    fn scan_library(&mut self) -> Task<AppMessage> {
        let places = self.quick_open.places.clone();
        if places.is_empty() {
            return Task::none();
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.quick_open.cancel_scan = Some(Arc::clone(&cancel));
        let request = self.quick_open.request;
        Task::perform(
            async move {
                let mut snapshots = Vec::new();
                let mut budget = MAX_LIBRARY_DOCUMENTS;
                for place in places {
                    if budget == 0 || cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    // A place that cannot be read is simply absent from search.
                    if let Ok(mut snapshot) = discovery::scan(place, Arc::clone(&cancel)).await {
                        snapshot.entries.retain(|entry| !entry.is_directory);
                        snapshot.entries.truncate(budget);
                        budget -= snapshot.entries.len();
                        snapshots.push(snapshot);
                    }
                }
                snapshots
            },
            move |snapshots| AppMessage::QuickOpen(Message::Library { request, snapshots }),
        )
    }

    pub(super) fn refresh_quick_open(&mut self) -> Task<AppMessage> {
        if self.overlay != Some(Overlay::QuickOpen) {
            return Task::none();
        }
        let moved = self.folder.root() != self.quick_open.root.as_deref();
        if moved && !std::mem::take(&mut self.quick_open.switching) {
            return self.cancel_quick_open();
        }
        let selected = self.quick_open.selected_target().cloned();
        if moved {
            // Arrived at the place chosen here: browse it from the top.
            self.quick_open.location = None;
            self.quick_open
                .candidates
                .retain(|candidate| candidate.place.is_none());
        }
        self.index_quick_open();
        self.quick_open.select(selected.as_ref());
        let scan = if moved {
            self.scan_library()
        } else {
            Task::none()
        };
        Task::batch([scan, self.reveal_quick_open_choice()])
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
        self.preserve_viewport_generation =
            Some(self.session.document.collaboration().generation());
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
            self.quick_open.clear();
            self.index_quick_open();
            // Recent leads with the document before this one, so opening the
            // switcher and pressing Enter switches back.
            let previous = self
                .quick_open
                .rows
                .iter()
                .position(|row| row.heading.as_deref() == Some(RECENT));
            self.quick_open.selected = previous.unwrap_or(0);
            self.chrome.menu = None;
            self.overlay = Some(Overlay::QuickOpen);
            self.preserve_viewport_generation =
                Some(self.session.document.collaboration().generation());
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
            let focus = widget::operate(Focused(None))
                .map(move |previous| AppMessage::QuickOpen(Message::Focus { request, previous }));
            return Task::batch([focus, self.scan_library()]);
        }
        if self.overlay != Some(Overlay::QuickOpen) {
            return Task::none();
        }
        if self.quick_open.root.as_deref() != self.folder.root() && !self.quick_open.switching {
            return self.cancel_quick_open();
        }
        match message {
            Message::Open => {}
            Message::Focus { request, previous } => {
                if request == self.quick_open.request {
                    self.quick_open.previous = previous;
                    return operation::focus(INPUT);
                }
            }
            Message::Library { request, snapshots } => {
                if request == self.quick_open.request {
                    let selected = self.quick_open.selected_target().cloned();
                    self.quick_open.index_library(&snapshots);
                    self.quick_open.select(selected.as_ref());
                }
            }
            Message::Query(query) => {
                self.quick_open.query = query.chars().take(1024).collect();
                self.quick_open.selected = 0;
                self.quick_open.list();
                return operation::snap_to(RESULTS, scrollable::RelativeOffset::START);
            }
            Message::Step(direction) => {
                self.quick_open.selected = self
                    .quick_open
                    .selected
                    .saturating_add_signed(direction as isize)
                    .min(self.quick_open.rows.len().saturating_sub(1));
                return self.reveal_quick_open_choice();
            }
            Message::Hover(index) => {
                self.quick_open.selected = index.min(self.quick_open.rows.len().saturating_sub(1))
            }
            Message::Accept => {
                return self.update_quick_open(Message::Pick(self.quick_open.selected));
            }
            Message::Pick(index) => {
                let Some(target) = self
                    .quick_open
                    .rows
                    .get(index)
                    .map(|row| row.target.clone())
                else {
                    return Task::none();
                };
                return self.pick_quick_open(target);
            }
            Message::StepIn => {
                if let Some(Target::Folder(folder)) = self.quick_open.selected_target().cloned() {
                    return self.pick_quick_open(Target::Folder(folder));
                }
            }
            Message::StepOut => {
                let Some(folder) = self.quick_open.location.take() else {
                    return Task::none();
                };
                self.quick_open.location = folder
                    .parent()
                    .filter(|parent| Some(*parent) != self.quick_open.root.as_deref())
                    .map(Path::to_owned);
                self.quick_open.list();
                self.quick_open.select(Some(&Target::Folder(folder)));
                return self.reveal_quick_open_choice();
            }
            Message::Choose(path) if !self.busy && !self.loading => {
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
                if self.session.path.as_ref() == Some(&path) {
                    return focus;
                }
                // This is a known file choice. Reuse Files' loader;
                // reclassifying a stale path could turn a renamed directory
                // into an unintended switch of place.
                let open = self.update_folder(super::folder::Message::Tree(
                    crate::editor::file_tree::Action::Open(path),
                ));
                return Task::batch([focus, open]);
            }
            Message::Choose(_) => {}
            Message::Cancel => return self.cancel_quick_open(),
        }
        Task::none()
    }

    fn pick_quick_open(&mut self, target: Target) -> Task<AppMessage> {
        match target {
            Target::Folder(folder) => {
                self.quick_open.query.clear();
                self.quick_open.location = Some(folder);
                self.quick_open.selected = 0;
                self.quick_open.list();
                operation::snap_to(RESULTS, scrollable::RelativeOffset::START)
            }
            Target::Place(place) if !self.busy => {
                self.quick_open.switching = true;
                self.open_workspace(place)
            }
            Target::File(path) if self.folder.files().any(|file| file == path) => {
                self.update_quick_open(Message::Choose(path))
            }
            Target::File(path) if !self.busy && !self.loading => {
                // A document in another place. Its place opens once it is on
                // the page.
                let focus = self.cancel_quick_open();
                if !self.commit_cell() {
                    return focus;
                }
                Task::batch([focus, self.request(Pending::Open(Some(path)))])
            }
            Target::Session(session) if !self.busy && !self.loading => {
                let focus = self.cancel_quick_open();
                if !self.commit_cell() {
                    return focus;
                }
                match self.background.take_unsaved(session) {
                    Some(parked) => Task::batch([focus, self.resume(parked)]),
                    None => focus,
                }
            }
            Target::Place(_) | Target::File(_) | Target::Session(_) => Task::none(),
        }
    }

    fn reveal_quick_open_choice(&self) -> Task<AppMessage> {
        // Row heights are known without layout; keep keyboard navigation visible
        // without moving focus out of the query or changing the document caret.
        struct Reveal {
            top: f32,
            height: f32,
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
                    } else if self.top + self.height > translation.y + bounds.height {
                        self.top + self.height - bounds.height
                    } else {
                        translation.y
                    };
                }
            }
            fn finish(&self) -> widget::operation::Outcome<f32> {
                widget::operation::Outcome::Some(self.top.max(0.0))
            }
        }
        let (top, height) = self.quick_open.selected_extent();
        widget::operate(Reveal { top, height }).then(|offset| {
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
        let input = text_input("Search or jump to…", &self.quick_open.query)
            .id(INPUT)
            .on_input(|query| AppMessage::QuickOpen(Message::Query(query)))
            .on_submit(AppMessage::QuickOpen(Message::Accept))
            .padding(10)
            .size(14)
            .style(styles::input(tokens));
        // Left and Right move the query caret once there is a query to move in.
        let input = if self.quick_open.query.is_empty() {
            FilterInput::new(input, |key| {
                Some(AppMessage::QuickOpen(match key {
                    Named::ArrowUp => Message::Step(-1),
                    Named::ArrowDown => Message::Step(1),
                    Named::ArrowRight => Message::StepIn,
                    Named::ArrowLeft => Message::StepOut,
                    Named::Escape => Message::Cancel,
                    _ => return None,
                }))
            })
        } else {
            FilterInput::new(input, |key| {
                Some(AppMessage::QuickOpen(match key {
                    Named::ArrowUp => Message::Step(-1),
                    Named::ArrowDown => Message::Step(1),
                    Named::Escape => Message::Cancel,
                    _ => return None,
                }))
            })
        };
        let mut results = column![];
        for (position, entry) in self.quick_open.rows.iter().enumerate() {
            if let Some(heading) = &entry.heading {
                results = results.push(
                    container(
                        text(heading)
                            .size(11)
                            .color(tokens.muted)
                            .wrapping(text::Wrapping::None),
                    )
                    .clip(true)
                    .height(HEADING_HEIGHT)
                    .padding(Padding::new(10.0).top(10.0).bottom(0.0)),
                );
            }
            let mut label =
                column![text(&entry.title).size(13).wrapping(text::Wrapping::None)].spacing(2);
            if !entry.location.is_empty() {
                label = label.push(
                    text(&entry.location)
                        .size(11)
                        .color(tokens.muted)
                        .wrapping(text::Wrapping::None),
                );
            }
            let mark = match entry.target {
                _ if entry.unsaved => Some(Icon::Modified),
                Target::Folder(_) | Target::Place(_) => Some(Icon::Folder),
                Target::File(_) | Target::Session(_) => None,
            };
            let mut line = row![].spacing(8).align_y(Alignment::Center);
            if let Some(mark) = mark {
                line = line.push(icon::sized(mark, tokens.muted, 14.0));
            }
            line = line.push(container(label).clip(true).width(Fill));
            let choice = button(container(line).center_y(Fill))
                .padding([6, 10])
                .height(entry.height())
                .width(Fill)
                .on_press(AppMessage::QuickOpen(Message::Pick(position)))
                .style(styles::button_style(
                    tokens,
                    if position == self.quick_open.selected {
                        Button::Selected
                    } else {
                        Button::Quiet
                    },
                ));
            let choice: Element<'_, AppMessage> = match &entry.target {
                Target::File(path) | Target::Folder(path) | Target::Place(path) => tooltip(
                    choice,
                    text(path.display().to_string()).size(11),
                    tooltip::Position::Bottom,
                )
                .into(),
                Target::Session(_) => choice.into(),
            };
            results = results.push(
                iced::widget::mouse_area(choice)
                    .on_move(move |_| AppMessage::QuickOpen(Message::Hover(position))),
            );
        }
        if self.quick_open.rows.is_empty() {
            results = results.push(
                container(
                    text(if !self.quick_open.query.trim().is_empty() {
                        "No matching documents"
                    } else if self.quick_open.location.is_some() {
                        "This folder holds no documents"
                    } else {
                        "Add a place to keep your writing within reach"
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
                text("Quick Open").size(13),
                container(
                    text(
                        self.folder
                            .root()
                            .map(super::folder::name)
                            .unwrap_or_default()
                    )
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
            container(body).max_height((available - 100.0).clamp(80.0, 420.0)),
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
