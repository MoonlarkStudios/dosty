//! Window-local folder browsing and checked document organization.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{
    button, column, container, mouse_area, opaque, operation, row, scrollable, space, text,
    text_input,
};
use iced::{Element, Event, Fill, Padding, Point, Size, Task, event, window};

use super::{App, EDITOR, Message as AppMessage, Overlay, Pending};
use crate::editor::file_tree::{self, FileTree};
use crate::io::{file_ops, folder, storage::FileError};
use crate::theme::styles::{self, Button};

const NAME_INPUT: &str = "folder-name";
const TREE: &str = "folder-file-tree";
const CHOICES: &str = "folder-choices";
const MAX_DESTINATIONS: usize = 256;

#[derive(Debug, Clone)]
pub enum Message {
    Pick,
    OpenPath(PathBuf),
    OpenRoot(PathBuf),
    Chosen {
        request: u64,
        path: Option<PathBuf>,
    },
    MovePicked {
        request: u64,
        source: PathBuf,
        path: Option<PathBuf>,
    },
    Classified {
        request: u64,
        path: PathBuf,
        result: Result<PathKind, FileError>,
    },
    Scanned {
        generation: u64,
        result: Result<folder::Snapshot, FileError>,
    },
    Refresh,
    Close,
    Tree(file_tree::Action),
    NewDocument(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    MoveTo(PathBuf),
    ChooseDestination(PathBuf),
    Destination(PathBuf),
    ShowLocation(PathBuf),
    Reveal(PathBuf),
    Revealed(Result<(), String>),
    Value(String),
    Accept,
    Cancel,
    Key {
        key: Named,
        shift: bool,
    },
    DismissMenu,
    SelectChoice(usize),
    UndoMove,
    Settled(u64),
    Finished {
        operation: u64,
        result: Result<Outcome, FileError>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum PathKind {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Document(PathBuf),
    Directory(PathBuf),
    Move(file_ops::MoveResult),
}

#[derive(Debug, Clone)]
enum NameAction {
    Document(PathBuf),
    Directory(PathBuf),
    Rename(PathBuf),
}

#[derive(Debug, Clone)]
enum Dialog {
    Menu {
        path: PathBuf,
        position: Point,
    },
    Name {
        action: NameAction,
        value: String,
        error: Option<String>,
    },
    Move {
        source: PathBuf,
        error: Option<String>,
    },
    Location(PathBuf),
}

#[derive(Debug, Clone)]
struct InFlight {
    id: u64,
    document: uuid::Uuid,
    replica: u64,
    moving: bool,
    undoing: bool,
}

struct Choice {
    label: String,
    message: Option<Message>,
}

#[derive(Default)]
pub(super) struct State {
    root: Option<PathBuf>,
    entries: Vec<folder::Entry>,
    visible: Vec<file_tree::Entry>,
    expanded: HashSet<PathBuf>,
    generation: u64,
    navigation: u64,
    cancel: Option<Arc<AtomicBool>>,
    scanning: bool,
    reveal_after_scan: bool,
    picker: bool,
    error: Option<String>,
    limited: bool,
    warnings: Vec<String>,
    dialog: Option<Dialog>,
    next_operation: u64,
    inflight: Option<InFlight>,
    undo: Option<(PathBuf, PathBuf)>,
    notice: Option<String>,
    highlighted: Option<PathBuf>,
    selected: usize,
}

pub(super) fn event_message(
    event: Event,
    status: event::Status,
    id: window::Id,
) -> Option<AppMessage> {
    if matches!(event, Event::Window(window::Event::Unfocused)) {
        return Some(AppMessage::Folder(Message::DismissMenu));
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
                | Named::Home
                | Named::End
                | Named::Tab
                | Named::Enter
                | Named::Space
                | Named::Escape
        )
    {
        return Some(AppMessage::Folder(Message::Key {
            key: *key,
            shift: modifiers.shift(),
        }));
    }
    super::event_message(event, status, id)
}

impl Drop for State {
    fn drop(&mut self) {
        self.cancel_scan();
    }
}

impl State {
    pub(super) fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }
    pub(super) fn files(&self) -> impl Iterator<Item = &Path> {
        self.entries
            .iter()
            .filter(|entry| !entry.is_directory)
            .map(|entry| entry.path.as_path())
    }
    pub(super) fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }
    pub(super) fn can_undo(&self) -> bool {
        self.undo.is_some() && self.inflight.is_none()
    }
    pub(super) fn is_menu(&self) -> bool {
        matches!(self.dialog, Some(Dialog::Menu { .. }))
    }
    pub(super) fn has_file(&self, path: &Path) -> bool {
        self.entries
            .iter()
            .any(|entry| !entry.is_directory && entry.path == path)
    }
    /// Whether Files opens when the next scan succeeds.
    pub(super) fn prepare_reveal(&mut self, reveal: bool) {
        self.reveal_after_scan = reveal;
    }
    /// Expanded folders below `root`, relative to it.
    pub(super) fn expanded_below(&self, root: &Path) -> Vec<PathBuf> {
        let mut expanded: Vec<_> = self
            .expanded
            .iter()
            .filter_map(|path| path.strip_prefix(root).ok())
            .filter(|relative| !relative.as_os_str().is_empty())
            .map(Path::to_owned)
            .collect();
        expanded.sort();
        expanded
    }
    /// Expand remembered folders that still exist in the current snapshot.
    pub(super) fn restore_expanded(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            if self
                .entries
                .iter()
                .any(|entry| entry.is_directory && entry.path == path)
            {
                self.expanded.insert(path);
            }
        }
        self.refresh_visible();
    }
    #[cfg(test)]
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    fn next_navigation(&mut self) -> u64 {
        self.navigation = self.navigation.wrapping_add(1);
        self.picker = false;
        self.navigation
    }

    fn cancel_scan(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.generation = self.generation.wrapping_add(1);
        self.scanning = false;
    }

    fn reveal(&mut self, path: &Path) {
        let Some(root) = &self.root else {
            return;
        };
        if !path.starts_with(root) {
            return;
        }
        for parent in path
            .ancestors()
            .skip(1)
            .take_while(|parent| parent.starts_with(root))
        {
            self.expanded.insert(parent.to_owned());
        }
    }

    fn refresh_visible(&mut self) {
        let Some(root) = &self.root else {
            self.visible.clear();
            return;
        };
        let mut children: HashMap<&Path, Vec<&folder::Entry>> = HashMap::new();
        for entry in &self.entries {
            if let Some(parent) = entry.path.parent() {
                children.entry(parent).or_default().push(entry);
            }
        }
        for siblings in children.values_mut() {
            siblings.sort_by(|a, b| {
                a.is_directory
                    .cmp(&b.is_directory)
                    .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
            });
        }
        let mut visible = Vec::new();
        let mut stack = Vec::new();
        if let Some(entries) = children.get(root.as_path()) {
            stack.extend(entries.iter().rev().map(|entry| (*entry, 0)));
        }
        while let Some((entry, depth)) = stack.pop() {
            let expanded = self.expanded.contains(&entry.path);
            visible.push(file_tree::Entry {
                path: entry.path.clone(),
                label: name(&entry.path),
                depth,
                directory: entry.is_directory,
                expanded,
            });
            if entry.is_directory
                && expanded
                && let Some(entries) = children.get(entry.path.as_path())
            {
                stack.extend(entries.iter().rev().map(|entry| (*entry, depth + 1)));
            }
        }
        self.visible = visible;
    }

    fn remap(&mut self, from: &Path, to: &Path) {
        self.expanded = self
            .expanded
            .iter()
            .map(|path| remap_path(path, from, to).unwrap_or_else(|| path.clone()))
            .collect();
        for entry in &mut self.entries {
            if let Some(path) = remap_path(&entry.path, from, to) {
                entry.path = path;
            }
        }
        self.reveal(to);
        self.refresh_visible();
    }
}

impl App {
    pub(super) fn update_folder(&mut self, message: Message) -> Task<AppMessage> {
        match message {
            Message::Scanned { generation, result } => {
                if generation != self.folder.generation {
                    return Task::none();
                }
                self.folder.scanning = false;
                self.folder.cancel = None;
                let reveal = std::mem::take(&mut self.folder.reveal_after_scan);
                match result {
                    Ok(snapshot) => {
                        let changed = self.folder.root.as_ref() != Some(&snapshot.root);
                        if changed {
                            self.folder.expanded.clear();
                        }
                        self.folder.expanded.insert(snapshot.root.clone());
                        self.folder.root = Some(snapshot.root);
                        self.folder.entries = snapshot.entries;
                        self.folder.limited = snapshot.limited;
                        self.folder.warnings = snapshot.warnings;
                        self.folder.error = None;
                        if let Some(path) = &self.path {
                            self.folder.reveal(path);
                        }
                        self.folder.refresh_visible();
                        if reveal {
                            self.reveal_navigation();
                        }
                        let root = self.folder.root.clone().expect("root was just set");
                        let workspace = self.workspace_scanned(&root);
                        return Task::batch([workspace, self.refresh_quick_open()]);
                    }
                    Err(error) => {
                        let error = error.to_string();
                        self.folder.error = Some(error.clone());
                        if self.folder.root.is_none() {
                            self.notification = Some(error);
                        }
                    }
                }
                return Task::none();
            }
            Message::Chosen { request, path } => {
                if request != self.folder.navigation {
                    return Task::none();
                }
                self.folder.picker = false;
                if self.busy || self.overlay.is_some() {
                    return Task::none();
                }
                if let Some(path) = path {
                    return self.open_workspace(path);
                }
                return Task::none();
            }
            Message::Classified {
                request,
                path,
                result,
            } => {
                if request != self.folder.navigation {
                    return Task::none();
                }
                if self.overlay.is_some() {
                    return Task::none();
                }
                match result {
                    Ok(PathKind::Directory) => return self.open_workspace(path),
                    Ok(PathKind::File) if !self.busy => {
                        if self.path.as_ref() == Some(&path) {
                            return Task::none();
                        }
                        if !self.commit_cell() {
                            return Task::none();
                        }
                        return self.request(Pending::Open(Some(path)));
                    }
                    Ok(PathKind::File) => {
                        self.notification = Some(
                            "Finish the current file operation before opening another document."
                                .into(),
                        )
                    }
                    Err(error) => self.notification = Some(error.to_string()),
                }
                return Task::none();
            }
            Message::MovePicked {
                request,
                source,
                path,
            } => {
                if request != self.folder.navigation {
                    return Task::none();
                }
                self.folder.picker = false;
                if self.busy || self.overlay.is_some() {
                    return Task::none();
                }
                if let Some(parent) = path {
                    let Some(filename) = source.file_name() else {
                        return Task::none();
                    };
                    let to = parent.join(filename);
                    return self.start_folder_move(source, to, false, true);
                }
                return Task::none();
            }
            Message::Finished { operation, result } => {
                return self.finish_folder_operation(operation, result);
            }
            Message::Revealed(result) => {
                if let Err(error) = result {
                    self.notification = Some(error);
                }
                return Task::none();
            }
            Message::DismissMenu => {
                if self.folder.is_menu() && matches!(self.overlay, Some(Overlay::Folder)) {
                    self.folder.dialog = None;
                    self.overlay = None;
                }
                return Task::none();
            }
            Message::Settled(operation) => {
                if operation == self.folder.next_operation {
                    self.folder.highlighted = None;
                }
                return Task::none();
            }
            _ => {}
        }
        // Async completions above must always be accepted. Commands cannot
        // replace another dialog or race document publication/import.
        if self.loading
            || (self.overlay.is_some() && !matches!(self.overlay, Some(Overlay::Folder)))
        {
            return Task::none();
        }
        if matches!(self.overlay, Some(Overlay::Folder))
            && !self.folder.is_menu()
            && !matches!(
                message,
                Message::Value(_)
                    | Message::Accept
                    | Message::Cancel
                    | Message::Destination(_)
                    | Message::Reveal(_)
                    | Message::ChooseDestination(_)
                    | Message::Key { .. }
                    | Message::SelectChoice(_)
            )
        {
            return Task::none();
        }
        match message {
            Message::Pick if !self.busy && !self.folder.picker => {
                self.folder.dialog = None;
                self.overlay = None;
                let request = self.folder.next_navigation();
                self.folder.picker = true;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .pick_folder()
                            .await
                            .map(|file| file.path().to_owned())
                    },
                    move |path| AppMessage::Folder(Message::Chosen { request, path }),
                );
            }
            Message::OpenPath(path) if !self.busy => {
                self.folder.dialog = None;
                self.overlay = None;
                let request = self.folder.next_navigation();
                let target = path.clone();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let metadata = std::fs::metadata(target)?;
                            if metadata.is_dir() {
                                Ok(PathKind::Directory)
                            } else if metadata.is_file() {
                                Ok(PathKind::File)
                            } else {
                                Err(FileError::Io("Choose a Markdown file or a folder.".into()))
                            }
                        })
                        .await
                        .map_err(|error| FileError::Io(error.to_string()))?
                    },
                    move |result| {
                        AppMessage::Folder(Message::Classified {
                            request,
                            path: path.clone(),
                            result,
                        })
                    },
                );
            }
            Message::OpenRoot(path) if !self.busy => {
                self.folder.next_navigation();
                self.folder.dialog = None;
                self.overlay = None;
                self.folder.reveal_after_scan = true;
                return self.scan_folder(path);
            }
            Message::Refresh if !self.busy => {
                self.folder.dialog = None;
                self.overlay = None;
                return self.folder_refresh();
            }
            Message::Close if !self.busy => {
                drop(self.cancel_quick_open());
                self.folder.cancel_scan();
                self.folder.next_navigation();
                self.folder.root = None;
                self.folder.reveal_after_scan = false;
                self.folder.entries.clear();
                self.folder.visible.clear();
                self.folder.expanded.clear();
                self.folder.dialog = None;
                self.folder.error = None;
                self.overlay = None;
                let workspace = self.workspace_closed();
                self.chrome.files.open = false;
                return Task::batch([workspace, operation::focus(EDITOR)]);
            }
            Message::Tree(action) if !self.busy => return self.folder_tree_action(action),
            Message::NewDocument(parent) if !self.busy => {
                self.folder.dialog = None;
                self.overlay = None;
                if !self.commit_cell() {
                    return Task::none();
                }
                return self.request(Pending::FolderNew(parent));
            }
            Message::NewFolder(parent) if !self.busy => {
                return self.folder_name(NameAction::Directory(parent));
            }
            Message::Rename(path) if !self.busy && self.folder.root.as_ref() != Some(&path) => {
                return self.folder_name(NameAction::Rename(path));
            }
            Message::MoveTo(source) if !self.busy && self.folder.root.as_ref() != Some(&source) => {
                if !self
                    .folder
                    .root
                    .as_ref()
                    .is_some_and(|root| source.starts_with(root))
                {
                    return self.pick_move_destination(source);
                }
                self.folder.dialog = Some(Dialog::Move {
                    source,
                    error: None,
                });
                self.overlay = Some(Overlay::Folder);
                self.first_folder_choice();
            }
            Message::ChooseDestination(source) if !self.busy => {
                return self.pick_move_destination(source);
            }
            Message::Destination(parent) if !self.busy => {
                if let Some(Dialog::Move { source, .. }) = &self.folder.dialog {
                    let source = source.clone();
                    let Some(filename) = source.file_name() else {
                        return Task::none();
                    };
                    let to = parent.join(filename);
                    return self.start_folder_move(source, to, false, false);
                }
            }
            Message::ShowLocation(path) if !self.busy => {
                self.folder.dialog = Some(Dialog::Location(path));
                self.overlay = Some(Overlay::Folder);
                self.first_folder_choice();
            }
            Message::Reveal(path) if !self.busy => {
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let directory = if std::fs::metadata(&path)
                                .map_err(|error| error.to_string())?
                                .is_dir()
                            {
                                path
                            } else {
                                path.parent().unwrap_or(&path).to_owned()
                            };
                            open::that_detached(directory).map_err(|error| error.to_string())
                        })
                        .await
                        .map_err(|error| error.to_string())?
                    },
                    |result| AppMessage::Folder(Message::Revealed(result)),
                );
            }
            Message::Value(value) if !self.busy => {
                if let Some(Dialog::Name {
                    value: field,
                    error,
                    ..
                }) = &mut self.folder.dialog
                {
                    *field = value;
                    *error = None;
                }
            }
            Message::Accept if !self.busy => return self.accept_folder_name(),
            Message::Cancel if !self.busy => {
                self.folder.dialog = None;
                if matches!(self.overlay, Some(Overlay::Folder)) {
                    self.overlay = None;
                }
                return operation::focus(
                    if self.chrome.sidebars_hidden || !self.chrome.files.open {
                        EDITOR
                    } else {
                        TREE
                    },
                );
            }
            Message::Key { key, shift } if !self.busy => return self.folder_key(key, shift),
            Message::SelectChoice(index)
                if !self.busy
                    && self
                        .folder_choices()
                        .get(index)
                        .is_some_and(|choice| choice.message.is_some()) =>
            {
                self.folder.selected = index;
            }
            Message::UndoMove if !self.busy => {
                if let Some((from, to)) = self.folder.undo.clone() {
                    return self.start_folder_move(to, from, true, false);
                }
            }
            _ => {}
        }
        Task::none()
    }

    pub(super) fn folder_new_document(&mut self, parent: PathBuf) -> Task<AppMessage> {
        self.folder_name(NameAction::Document(parent))
    }

    fn folder_name(&mut self, action: NameAction) -> Task<AppMessage> {
        let value = match &action {
            NameAction::Document(_) => "Untitled.md".into(),
            NameAction::Directory(_) => "New folder".into(),
            NameAction::Rename(path) => name(path),
        };
        self.folder.dialog = Some(Dialog::Name {
            action,
            value,
            error: None,
        });
        self.overlay = Some(Overlay::Folder);
        operation::focus(NAME_INPUT)
    }

    fn folder_tree_action(&mut self, action: file_tree::Action) -> Task<AppMessage> {
        match action {
            file_tree::Action::Focus => {
                self.editor_focused = false;
                return operation::focus(TREE);
            }
            file_tree::Action::Open(path) => {
                if self.path.as_ref() == Some(&path) {
                    return Task::none();
                }
                if !self.commit_cell() {
                    return Task::none();
                }
                return self.request(Pending::Open(Some(path)));
            }
            file_tree::Action::Toggle(path) => {
                if !self.folder.expanded.remove(&path) {
                    self.folder.expanded.insert(path);
                }
                self.folder.refresh_visible();
                return self.persist_workspace();
            }
            file_tree::Action::Expand(path) => {
                self.folder.expanded.insert(path);
                self.folder.refresh_visible();
                return self.persist_workspace();
            }
            file_tree::Action::Menu { path, position } => {
                self.folder.dialog = Some(Dialog::Menu { path, position });
                self.overlay = Some(Overlay::Folder);
                self.first_folder_choice();
            }
            file_tree::Action::Rename(path) => return self.update_folder(Message::Rename(path)),
            file_tree::Action::Move {
                source,
                destination,
            } => {
                let Some(filename) = source.file_name() else {
                    return Task::none();
                };
                let to = destination.join(filename);
                return self.start_folder_move(source, to, false, false);
            }
        }
        Task::none()
    }

    pub(super) fn scan_folder(&mut self, root: PathBuf) -> Task<AppMessage> {
        self.folder.cancel_scan();
        let generation = self.folder.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.folder.cancel = Some(Arc::clone(&cancel));
        self.folder.scanning = true;
        self.folder.error = None;
        Task::perform(folder::scan(root, cancel), move |result| {
            AppMessage::Folder(Message::Scanned { generation, result })
        })
    }

    pub(super) fn folder_refresh(&mut self) -> Task<AppMessage> {
        self.folder
            .root
            .clone()
            .map_or_else(Task::none, |root| self.scan_folder(root))
    }

    pub(super) fn folder_document_changed(&mut self) -> Task<AppMessage> {
        if let Some(path) = &self.path {
            self.folder.reveal(path);
        }
        self.folder.refresh_visible();
        if !self.busy
            && self
                .folder
                .root
                .as_ref()
                .zip(self.path.as_ref())
                .is_some_and(|(root, path)| path.starts_with(root))
        {
            self.folder_refresh()
        } else {
            Task::none()
        }
    }

    fn accept_folder_name(&mut self) -> Task<AppMessage> {
        let Some(Dialog::Name { action, value, .. }) = self.folder.dialog.clone() else {
            return Task::none();
        };
        let kind = match &action {
            NameAction::Directory(_) => false,
            NameAction::Document(_) => true,
            NameAction::Rename(path) => folder::is_markdown(path),
        };
        let value = match validate_name(&value, kind) {
            Ok(value) => value,
            Err(error) => {
                self.folder_error(error);
                return Task::none();
            }
        };
        match action {
            NameAction::Rename(from) => {
                let Some(parent) = from.parent() else {
                    return Task::none();
                };
                let to = parent.join(value);
                self.start_folder_move(from, to, false, false)
            }
            NameAction::Document(parent) => self.start_folder_create(parent, value, false),
            NameAction::Directory(parent) => self.start_folder_create(parent, value, true),
        }
    }

    fn begin_folder_operation(&mut self, moving: bool, undoing: bool) -> u64 {
        self.folder.cancel_scan();
        // A native picker or path classification started before the move must
        // not change its navigation scope while the worker owns the snapshot.
        self.folder.next_navigation();
        self.folder.next_operation = self.folder.next_operation.wrapping_add(1);
        let id = self.folder.next_operation;
        self.folder.inflight = Some(InFlight {
            id,
            document: self.document.collaboration().id(),
            replica: self.document.collaboration().replica_id(),
            moving,
            undoing,
        });
        self.busy = true;
        self.loading = moving;
        id
    }

    fn start_folder_create(
        &mut self,
        parent: PathBuf,
        name: String,
        directory: bool,
    ) -> Task<AppMessage> {
        if !self
            .folder
            .root
            .as_ref()
            .is_some_and(|root| parent.starts_with(root))
        {
            self.folder_error("Choose a folder in the open directory.".into());
            return Task::none();
        }
        let operation = self.begin_folder_operation(false, false);
        Task::perform(
            async move {
                if directory {
                    file_ops::create_folder(parent, name)
                        .await
                        .map(Outcome::Directory)
                } else {
                    file_ops::create_document(parent, name)
                        .await
                        .map(Outcome::Document)
                }
            },
            move |result| AppMessage::Folder(Message::Finished { operation, result }),
        )
    }

    fn start_folder_move(
        &mut self,
        from: PathBuf,
        to: PathBuf,
        undoing: bool,
        external_destination: bool,
    ) -> Task<AppMessage> {
        let root_move = self.folder.root.as_ref().is_some_and(|root| &from == root);
        let inside = self
            .folder
            .root
            .as_ref()
            .is_some_and(|root| from.starts_with(root) && to.starts_with(root));
        let active = self.path.as_ref() == Some(&from);
        let active_rename = active && from.parent() == to.parent();
        let authorized_undo = undoing
            && self
                .folder
                .undo
                .as_ref()
                .is_some_and(|(old, new)| old == &to && new == &from);
        if root_move
            || !(inside
                || active_rename
                || (external_destination
                    && (active
                        || self
                            .folder
                            .root
                            .as_ref()
                            .is_some_and(|root| from.starts_with(root))))
                || authorized_undo)
        {
            self.folder_error("Move items within the open folder, or choose a destination for the current document.".into());
            return Task::none();
        }
        if from == to {
            self.folder.dialog = None;
            self.overlay = None;
            return Task::none();
        }
        if to.starts_with(&from) {
            self.folder_error("A folder cannot be moved into itself.".into());
            return Task::none();
        }
        if !self.commit_cell() {
            return Task::none();
        }
        let active = self.path.clone().map(|path| file_ops::ActiveDocument {
            path,
            saved_source: self.saved_source.clone(),
            saved_version: self.saved_version.clone(),
            source: self.document.source().to_owned(),
            history: self.images.history(),
        });
        let operation = self.begin_folder_operation(true, undoing);
        Task::perform(file_ops::move_item(from, to, active), move |result| {
            AppMessage::Folder(Message::Finished {
                operation,
                result: result.map(Outcome::Move),
            })
        })
    }

    fn finish_folder_operation(
        &mut self,
        operation: u64,
        result: Result<Outcome, FileError>,
    ) -> Task<AppMessage> {
        let Some(inflight) = self.folder.inflight.take() else {
            return Task::none();
        };
        if inflight.id != operation {
            self.folder.inflight = Some(inflight);
            return Task::none();
        }
        self.busy = false;
        if inflight.moving {
            self.loading = false;
        }
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.folder_error(error.to_string());
                return Task::none();
            }
        };
        let mut open = None;
        let mut remembered = Task::none();
        let highlighted = match outcome {
            Outcome::Document(path) => {
                if !self.folder.entries.iter().any(|entry| entry.path == path) {
                    self.folder.entries.push(folder::Entry {
                        path: path.clone(),
                        is_directory: false,
                    });
                }
                self.folder.notice = Some(format!("Created {}", name(&path)));
                self.folder.undo = None;
                open = Some(path.clone());
                path
            }
            Outcome::Directory(path) => {
                if !self.folder.entries.iter().any(|entry| entry.path == path) {
                    self.folder.entries.push(folder::Entry {
                        path: path.clone(),
                        is_directory: true,
                    });
                }
                self.folder.notice = Some(format!("Created {}", name(&path)));
                self.folder.undo = None;
                self.folder.expanded.insert(path.clone());
                path
            }
            Outcome::Move(result) => {
                if let Some(warning) = result.warning {
                    self.notification = Some(warning);
                }
                self.folder.remap(&result.from, &result.to);
                remembered = self.workspace_moved(&result.from, &result.to);
                if self.document.collaboration().id() == inflight.document
                    && self.document.collaboration().replica_id() == inflight.replica
                    && let Some(path) = result.active_path
                {
                    self.display_name = name(&path);
                    self.path = Some(path);
                    self.recovery.path_changed();
                }
                self.folder.notice = Some(if inflight.undoing {
                    "Move undone".into()
                } else if result.from.parent() == result.to.parent() {
                    format!("Renamed to {}", name(&result.to))
                } else {
                    format!(
                        "Moved to {}",
                        result.to.parent().map_or_else(String::new, name)
                    )
                });
                self.folder.undo = (!inflight.undoing).then_some((result.from, result.to.clone()));
                result.to
            }
        };
        self.folder.highlighted = Some(highlighted.clone());
        self.folder.reveal(&highlighted);
        self.folder.refresh_visible();
        self.folder.dialog = None;
        if matches!(self.overlay, Some(Overlay::Folder)) {
            self.overlay = None;
        }
        let refresh = self.folder_refresh();
        let settle = Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(850)).await;
            },
            move |_| AppMessage::Folder(Message::Settled(operation)),
        );
        if let Some(path) = open {
            return Task::batch([
                refresh,
                settle,
                remembered,
                self.request(Pending::Open(Some(path))),
            ]);
        }
        Task::batch([refresh, settle, remembered])
    }

    fn folder_error(&mut self, error: String) {
        match &mut self.folder.dialog {
            Some(Dialog::Name { error: field, .. } | Dialog::Move { error: field, .. }) => {
                *field = Some(error)
            }
            _ => self.notification = Some(error),
        }
    }

    pub(super) fn folder_view(&self) -> Element<'_, AppMessage> {
        if self.folder.root.is_none() {
            return space().into();
        }
        let tokens = self.resolved_theme.tokens;
        let mut content = column![].spacing(10);
        content = content.push(
            FileTree::new(
                &self.folder.visible,
                self.path.as_deref(),
                tokens,
                |action| AppMessage::Folder(Message::Tree(action)),
            )
            .root(
                self.folder
                    .root()
                    .expect("folder view requires a workspace"),
            )
            .highlighted(self.folder.highlighted.as_deref())
            .enabled(!self.busy && self.overlay.is_none()),
        );
        if self.folder.scanning {
            content = content.push(text("Finding Markdown files…").size(11).color(tokens.muted));
        }
        if self.folder.root.is_some()
            && !self.folder.scanning
            && !self.folder.entries.iter().any(|entry| !entry.is_directory)
        {
            content = content.push(
                text("No Markdown files in this folder.")
                    .size(12)
                    .color(tokens.muted),
            );
        }
        if let Some(error) = &self.folder.error {
            content = content.push(text(error).size(11).color(tokens.danger));
        }
        if self.folder.limited {
            content = content.push(text("This folder is large; some entries are not shown. Open a smaller folder to see more.").size(11).color(tokens.muted));
        }
        if let Some(warning) = self.folder.warnings.first() {
            content = content.push(text(warning).size(11).color(tokens.muted));
        }
        content.height(Fill).into()
    }

    pub(super) fn folder_menu_view(&self, size: Size) -> Element<'_, AppMessage> {
        let Some(Dialog::Menu { path, position }) = &self.folder.dialog else {
            return text("").into();
        };
        let tokens = self.resolved_theme.tokens;
        let mut content = column![text(name(path)).size(12).color(tokens.muted)].spacing(3);
        let choices = self.folder_choices();
        for (index, choice) in choices.iter().enumerate() {
            content = content.push(self.folder_choice_button(choice, index));
        }
        let width = 236_f32.min((size.width - 16.0).max(1.0));
        let height = (48.0 + choices.len() as f32 * 36.0).min((size.height - 16.0).max(1.0));
        let x = position.x.min((size.width - width - 8.0).max(8.0)).max(8.0);
        let y = if position.y + height + 8.0 > size.height {
            position.y - height
        } else {
            position.y
        }
        .min((size.height - height - 8.0).max(8.0))
        .max(8.0);
        container(opaque(
            container(
                scrollable(content)
                    .id(CHOICES)
                    .style(styles::scrollbar(tokens)),
            )
            .width(width)
            .max_height(height)
            .padding(8)
            .style(styles::context_menu(tokens)),
        ))
        .padding(Padding {
            top: y,
            left: x,
            right: 0.0,
            bottom: 0.0,
        })
        .into()
    }

    pub(super) fn folder_overlay_view(&self, available_height: f32) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let mut content = column![].spacing(16);
        match &self.folder.dialog {
            Some(Dialog::Name {
                action,
                value,
                error,
            }) => {
                let (title, parent) = match action {
                    NameAction::Document(parent) => ("New document", parent.as_path()),
                    NameAction::Directory(parent) => ("New folder", parent.as_path()),
                    NameAction::Rename(path) => ("Rename", path.parent().unwrap_or(path)),
                };
                content = content
                    .push(text(title).font(styles::BODY_FONT).size(28))
                    .push(text(parent.to_string_lossy()).size(12).color(tokens.muted));
                let mut input = text_input("Name", value)
                    .id(NAME_INPUT)
                    .style(styles::input(tokens))
                    .padding(10);
                if !self.busy {
                    input = input
                        .on_input(|value| AppMessage::Folder(Message::Value(value)))
                        .on_submit(AppMessage::Folder(Message::Accept));
                }
                content = content.push(input);
                if let Some(error) = error {
                    content = content.push(text(error).size(12).color(tokens.danger));
                }
                content = content.push(
                    row![
                        self.folder_dialog_button("Cancel", Message::Cancel, false),
                        self.folder_dialog_button(
                            if self.busy { "Working…" } else { "Apply" },
                            Message::Accept,
                            true
                        )
                    ]
                    .spacing(8),
                );
            }
            Some(Dialog::Move { source, error }) => {
                content = content.push(
                    text(format!("Move {}", name(source)))
                        .font(styles::BODY_FONT)
                        .size(26),
                );
                let mut destinations = column![].spacing(0);
                for (index, choice) in self.folder_choices().iter().enumerate() {
                    destinations = destinations.push(self.folder_choice_button(choice, index));
                }
                content = content.push(
                    scrollable(destinations)
                        .id(CHOICES)
                        .height(280)
                        .style(styles::scrollbar(tokens)),
                );
                if let Some(error) = error {
                    content = content.push(text(error).size(12).color(tokens.danger));
                }
            }
            Some(Dialog::Location(path)) => {
                content = content
                    .push(text("Location").font(styles::BODY_FONT).size(28))
                    .push(text(path.to_string_lossy()).size(13));
                for (index, choice) in self.folder_choices().iter().enumerate() {
                    content = content.push(self.folder_choice_button(choice, index));
                }
            }
            _ => return text("").into(),
        }
        container(content)
            .width(480)
            .max_height(available_height.max(120.0))
            .padding(24)
            .style(styles::card(tokens))
            .into()
    }

    fn folder_dialog_button<'a>(
        &self,
        label: &'a str,
        message: Message,
        primary: bool,
    ) -> Element<'a, AppMessage> {
        let mut control =
            button(text(label).size(13))
                .padding([8, 12])
                .style(styles::button_style(
                    self.resolved_theme.tokens,
                    if primary {
                        Button::Primary
                    } else {
                        Button::Quiet
                    },
                ));
        if !self.busy {
            control = control.on_press(AppMessage::Folder(message));
        }
        control.into()
    }

    fn folder_choice_button(&self, choice: &Choice, index: usize) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let label = container(
            text(choice.label.clone())
                .size(13)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Fill)
        .clip(true);
        let mut control =
            button(label)
                .width(Fill)
                .height(36)
                .padding([8, 10])
                .style(styles::button_style(
                    tokens,
                    if self.folder.selected == index && choice.message.is_some() {
                        Button::Selected
                    } else {
                        Button::Quiet
                    },
                ));
        if !self.busy
            && let Some(message) = &choice.message
        {
            control = control.on_press(AppMessage::Folder(message.clone()));
        }
        mouse_area(control)
            .on_enter(AppMessage::Folder(Message::SelectChoice(index)))
            .into()
    }

    fn pick_move_destination(&mut self, source: PathBuf) -> Task<AppMessage> {
        let known = self.path.as_ref() == Some(&source)
            || self
                .folder
                .root
                .as_ref()
                .is_some_and(|root| source.starts_with(root));
        if !known || self.folder.picker {
            return Task::none();
        }
        self.folder.dialog = None;
        self.overlay = None;
        let request = self.folder.next_navigation();
        self.folder.picker = true;
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .pick_folder()
                    .await
                    .map(|file| file.path().to_owned())
            },
            move |path| {
                AppMessage::Folder(Message::MovePicked {
                    request,
                    source: source.clone(),
                    path,
                })
            },
        )
    }

    fn folder_choices(&self) -> Vec<Choice> {
        match &self.folder.dialog {
            Some(Dialog::Menu { path, .. }) => {
                let directory = self.folder.root.as_ref() == Some(path)
                    || self
                        .folder
                        .entries
                        .iter()
                        .any(|entry| entry.path == *path && entry.is_directory);
                let root = self.folder.root.as_ref() == Some(path);
                let mut actions = Vec::new();
                if directory {
                    actions.extend([
                        ("New document…", Message::NewDocument(path.clone())),
                        ("New folder…", Message::NewFolder(path.clone())),
                    ]);
                }
                if !root {
                    actions.extend([
                        ("Rename", Message::Rename(path.clone())),
                        ("Move to…", Message::MoveTo(path.clone())),
                    ]);
                }
                actions.push(("Show location", Message::ShowLocation(path.clone())));
                if root {
                    actions.extend([
                        ("Refresh folder", Message::Refresh),
                        ("Close folder", Message::Close),
                    ]);
                }
                actions
                    .into_iter()
                    .map(|(label, message)| Choice {
                        label: label.into(),
                        message: Some(message),
                    })
                    .collect()
            }
            Some(Dialog::Move { source, .. }) => {
                let mut choices = vec![Choice {
                    label: "Choose folder…".into(),
                    message: Some(Message::ChooseDestination(source.clone())),
                }];
                let existing: HashSet<&Path> = self
                    .folder
                    .entries
                    .iter()
                    .map(|entry| entry.path.as_path())
                    .collect();
                let dirs = self.folder.root.iter().chain(
                    self.folder
                        .entries
                        .iter()
                        .filter(|entry| entry.is_directory)
                        .map(|entry| &entry.path),
                );
                for destination in dirs.take(MAX_DESTINATIONS) {
                    if destination.starts_with(source) {
                        continue;
                    }
                    let label = self
                        .folder
                        .root
                        .as_ref()
                        .and_then(|root| destination.strip_prefix(root).ok())
                        .filter(|path| !path.as_os_str().is_empty())
                        .map_or_else(
                            || name(destination),
                            |relative| {
                                format!(
                                    "{} / {}",
                                    self.folder.root.as_deref().map_or_else(String::new, name),
                                    relative.display()
                                )
                            },
                        );
                    let target = source
                        .file_name()
                        .map(|filename| destination.join(filename));
                    let collision = target
                        .as_ref()
                        .is_some_and(|target| existing.contains(target.as_path()));
                    let same = source.parent() == Some(destination.as_path());
                    choices.push(Choice {
                        label: if same {
                            format!("{label} · current folder")
                        } else if collision {
                            format!("{label} · name exists")
                        } else {
                            label
                        },
                        message: (!same && !collision)
                            .then(|| Message::Destination(destination.clone())),
                    });
                }
                choices.push(Choice {
                    label: "Cancel".into(),
                    message: Some(Message::Cancel),
                });
                choices
            }
            Some(Dialog::Location(path)) => vec![
                Choice {
                    label: "Show in folder".into(),
                    message: Some(Message::Reveal(path.clone())),
                },
                Choice {
                    label: "Done".into(),
                    message: Some(Message::Cancel),
                },
            ],
            _ => Vec::new(),
        }
    }

    fn first_folder_choice(&mut self) {
        self.folder.selected = self
            .folder_choices()
            .iter()
            .position(|choice| choice.message.is_some())
            .unwrap_or(0);
    }

    fn folder_key(&mut self, key: Named, shift: bool) -> Task<AppMessage> {
        if !matches!(self.overlay, Some(Overlay::Folder)) {
            return Task::none();
        }
        if key == Named::Escape {
            return self.update_folder(Message::Cancel);
        }
        // The native name input owns typing, caret motion and on_submit.
        if matches!(self.folder.dialog, Some(Dialog::Name { .. })) {
            return Task::none();
        }
        let choices = self.folder_choices();
        let enabled: Vec<_> = choices
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| choice.message.as_ref().map(|_| index))
            .collect();
        if enabled.is_empty() {
            return Task::none();
        }
        let current = enabled
            .iter()
            .position(|index| *index == self.folder.selected)
            .unwrap_or(0);
        self.folder.selected = enabled[current];
        self.folder.selected = match key {
            Named::ArrowUp => enabled[(current + enabled.len() - 1) % enabled.len()],
            Named::ArrowDown => enabled[(current + 1) % enabled.len()],
            Named::Tab => {
                enabled[(current + if shift { enabled.len() - 1 } else { 1 }) % enabled.len()]
            }
            Named::Home => enabled[0],
            Named::End => *enabled.last().expect("nonempty choices"),
            Named::Enter | Named::Space => {
                if let Some(message) = choices[self.folder.selected].message.clone() {
                    return self.update_folder(message);
                }
                return Task::none();
            }
            _ => return Task::none(),
        };
        let y = if choices.len() > 1 {
            self.folder.selected as f32 / (choices.len() - 1) as f32
        } else {
            0.0
        };
        operation::snap_to(CHOICES, scrollable::RelativeOffset { x: 0.0, y })
    }
}

pub(super) fn name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn remap_path(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    path.strip_prefix(from).ok().map(|suffix| to.join(suffix))
}

fn validate_name(value: &str, markdown: bool) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return Err("Enter a name other than . or ..".into());
    }
    if trimmed.contains(['/', '\\', '\0']) {
        return Err("Names cannot contain a path separator.".into());
    }
    let mut name = trimmed.to_owned();
    if markdown
        && !["md", "markdown"].iter().any(|extension| {
            Path::new(&name)
                .extension()
                .is_some_and(|actual| actual.eq_ignore_ascii_case(extension))
        })
    {
        name.push_str(".md");
    }
    if name.len() > 240 {
        return Err("Use a name shorter than 240 bytes.".into());
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::selection::Selection;

    fn app() -> App {
        let (mut app, task) = App::new();
        drop(task);
        app.set_document(
            "# Saved\n\nCafé 海".into(),
            Some(PathBuf::from("/writing/note.md")),
            "note.md".into(),
        );
        app.selection = Selection {
            anchor: app.document.source().len(),
            head: 2,
        };
        app.sync_editor();
        app
    }

    #[test]
    fn workspace_appears_only_after_a_successful_scan_and_reports_failures() {
        let mut app = app();
        let source = app.document.source().to_owned();
        let selection = app.selection;
        drop(app.update_folder(Message::OpenRoot("/missing".into())));
        assert!(!app.chrome.files.open && app.folder.root.is_none());
        drop(app.update_folder(Message::Scanned {
            generation: app.folder.generation,
            result: Err(FileError::Io("Folder unavailable".into())),
        }));
        assert!(!app.chrome.files.open && app.folder.root.is_none());
        assert!(
            app.notification
                .as_deref()
                .is_some_and(|notice| notice.contains("Folder unavailable"))
        );
        drop(app.update_folder(Message::OpenRoot("/writing".into())));
        drop(app.update_folder(Message::Scanned {
            generation: app.folder.generation,
            result: Ok(folder::Snapshot {
                root: "/writing".into(),
                entries: Vec::new(),
                limited: false,
                warnings: Vec::new(),
            }),
        }));
        assert!(app.chrome.files.open && app.folder.root().is_some());
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, selection);
    }

    #[test]
    fn stale_scan_cannot_replace_a_new_folder_or_reopen_closed_navigation() {
        let mut app = app();
        app.folder.generation = 4;
        drop(app.update_folder(Message::Scanned {
            generation: 3,
            result: Ok(folder::Snapshot {
                root: "/old".into(),
                entries: vec![],
                limited: false,
                warnings: vec![],
            }),
        }));
        assert!(app.folder.root.is_none());
        drop(app.update_folder(Message::Close));
        drop(app.update_folder(Message::Scanned {
            generation: 4,
            result: Ok(folder::Snapshot {
                root: "/old".into(),
                entries: vec![],
                limited: false,
                warnings: vec![],
            }),
        }));
        assert!(app.folder.root.is_none());
    }

    #[test]
    fn folder_scan_and_disclosure_preserve_source_direction_and_history() {
        let mut app = app();
        let source = app.document.source().to_owned();
        let selection = app.selection;
        let id = app.document.collaboration().id();
        drop(app.update_folder(Message::Scanned {
            generation: 0,
            result: Ok(folder::Snapshot {
                root: "/writing".into(),
                entries: vec![
                    folder::Entry {
                        path: "/writing/empty".into(),
                        is_directory: true,
                    },
                    folder::Entry {
                        path: "/writing/note.md".into(),
                        is_directory: false,
                    },
                ],
                limited: false,
                warnings: vec![],
            }),
        }));
        drop(app.update_folder(Message::Tree(file_tree::Action::Toggle(
            "/writing/empty".into(),
        ))));
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, selection);
        assert_eq!(app.document.collaboration().id(), id);
        assert!(!app.dirty);
        assert!(
            app.folder
                .visible
                .iter()
                .any(|entry| entry.directory && entry.path.ends_with("empty"))
        );
    }

    #[test]
    fn failed_scan_retains_the_previous_folder_and_document() {
        let mut app = app();
        app.folder.root = Some("/writing".into());
        let selection = app.selection;
        drop(app.update_folder(Message::Scanned {
            generation: 0,
            result: Err(FileError::Io("Drive unavailable".into())),
        }));
        assert_eq!(app.folder.root.as_deref(), Some(Path::new("/writing")));
        assert!(app.folder.error.is_some());
        assert_eq!(app.selection, selection);
    }

    #[test]
    fn file_switch_uses_the_unsaved_guard_and_leaves_active_row_until_success() {
        let mut app = app();
        app.dirty = true;
        let selection = app.selection;
        drop(app.update_folder(Message::Tree(file_tree::Action::Open(
            "/writing/other.md".into(),
        ))));
        assert!(matches!(app.overlay, Some(Overlay::Unsaved)));
        assert_eq!(app.path.as_deref(), Some(Path::new("/writing/note.md")));
        assert_eq!(app.selection, selection);
    }

    #[test]
    fn active_path_remap_keeps_unsaved_source_selection_and_undo_session() {
        let mut app = app();
        let id = app.document.collaboration().id();
        let replica = app.document.collaboration().replica_id();
        let source = app.document.source().to_owned();
        let selection = app.selection;
        app.dirty = true;
        app.folder.root = Some("/writing".into());
        app.folder.next_operation = 1;
        app.folder.inflight = Some(InFlight {
            id: 1,
            document: id,
            replica,
            moving: true,
            undoing: false,
        });
        app.busy = true;
        app.loading = true;
        drop(app.update_folder(Message::Finished {
            operation: 1,
            result: Ok(Outcome::Move(file_ops::MoveResult {
                from: "/writing/note.md".into(),
                to: "/writing/drafts/note.md".into(),
                active_path: Some("/writing/drafts/note.md".into()),
                warning: None,
            })),
        }));
        assert_eq!(
            app.path.as_deref(),
            Some(Path::new("/writing/drafts/note.md"))
        );
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, selection);
        assert_eq!(app.document.collaboration().replica_id(), replica);
        assert!(app.dirty);
        assert!(!app.busy);
        assert!(!app.loading);
        assert!(app.folder.can_undo());
    }

    #[test]
    fn checked_move_freezes_source_and_rejects_old_picker_and_scan_completions() {
        let mut app = app();
        app.folder.root = Some("/writing".into());
        app.folder.navigation = 9;
        app.folder.generation = 4;
        app.folder.picker = true;
        let source = app.document.source().to_owned();
        let selection = app.selection;
        drop(app.update_folder(Message::Tree(file_tree::Action::Move {
            source: "/writing/note.md".into(),
            destination: "/writing/drafts".into(),
        })));
        assert!(app.busy && app.loading);
        assert!(!app.folder.picker);
        let operation = app.folder.inflight.as_ref().unwrap().id;
        drop(app.update_folder(Message::Chosen {
            request: 9,
            path: Some("/other".into()),
        }));
        drop(app.update_folder(Message::Scanned {
            generation: 4,
            result: Ok(folder::Snapshot {
                root: "/other".into(),
                entries: vec![],
                limited: false,
                warnings: vec![],
            }),
        }));
        drop(app.update_folder(Message::Finished {
            operation: operation.wrapping_sub(1),
            result: Err(FileError::Io("Stale completion".into())),
        }));
        assert!(app.busy && app.loading);
        assert_eq!(app.folder.root.as_deref(), Some(Path::new("/writing")));
        drop(app.update_folder(Message::Finished {
            operation,
            result: Err(FileError::Io("Destination already exists".into())),
        }));
        assert!(!app.busy && !app.loading);
        assert_eq!(app.path.as_deref(), Some(Path::new("/writing/note.md")));
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, selection);
        assert!(app.folder.highlighted.is_none());
        assert!(!app.folder.can_undo());
    }

    #[test]
    fn standalone_rename_preserves_writing_only_mode_and_selection() {
        let mut app = app();
        app.chrome.sidebars_hidden = true;
        app.chrome.files.open = false;
        let selection = app.selection;
        let source = app.document.source().to_owned();
        drop(app.update_folder(Message::Rename("/writing/note.md".into())));
        assert!(matches!(app.overlay, Some(Overlay::Folder)));
        assert!(app.chrome.sidebars_hidden && !app.chrome.files.open);
        drop(app.update_folder(Message::Value("Renamed".into())));
        // The input's on_submit owns Enter; the global menu route cannot
        // submit twice or consume spaces from the filename field.
        drop(app.update_folder(Message::Key {
            key: Named::Enter,
            shift: false,
        }));
        assert!(!app.busy);
        drop(app.update_folder(Message::Accept));
        assert!(app.busy && app.loading);
        assert!(app.chrome.sidebars_hidden && !app.chrome.files.open);
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, selection);
        assert_eq!(app.path.as_deref(), Some(Path::new("/writing/note.md")));
    }

    #[test]
    fn move_picker_keyboard_skips_invalid_destinations_and_can_cancel() {
        let mut app = app();
        app.chrome.sidebars_hidden = true;
        app.chrome.files.open = false;
        app.folder.root = Some("/writing".into());
        app.folder.entries = vec![
            folder::Entry {
                path: "/writing/drafts".into(),
                is_directory: true,
            },
            folder::Entry {
                path: "/writing/drafts/note.md".into(),
                is_directory: false,
            },
            folder::Entry {
                path: "/writing/archive".into(),
                is_directory: true,
            },
        ];
        let selection = app.selection;
        drop(app.update_folder(Message::MoveTo("/writing/note.md".into())));
        assert_eq!(app.folder.selected, 0);
        drop(app.update_folder(Message::Key {
            key: Named::ArrowDown,
            shift: false,
        }));
        let choices = app.folder_choices();
        assert!(matches!(
            &choices[app.folder.selected].message,
            Some(Message::Destination(path)) if path == Path::new("/writing/archive")
        ));
        drop(app.update_folder(Message::Key {
            key: Named::End,
            shift: false,
        }));
        drop(app.update_folder(Message::Key {
            key: Named::Enter,
            shift: false,
        }));
        assert!(app.overlay.is_none());
        assert!(app.folder.dialog.is_none());
        assert!(!app.busy);
        assert!(app.chrome.sidebars_hidden && !app.chrome.files.open);
        assert_eq!(app.selection, selection);
    }

    #[test]
    fn pending_picker_cannot_replace_another_modal_or_clear_a_newer_picker() {
        let mut app = app();
        app.folder.navigation = 2;
        app.folder.picker = true;
        drop(app.update_folder(Message::Chosen {
            request: 1,
            path: None,
        }));
        assert!(app.folder.picker);
        app.overlay = Some(Overlay::Unsaved);
        drop(app.update_folder(Message::MovePicked {
            request: 2,
            source: "/writing/note.md".into(),
            path: Some("/other".into()),
        }));
        assert!(!app.folder.picker);
        assert!(matches!(app.overlay, Some(Overlay::Unsaved)));
        assert!(app.folder.inflight.is_none());
        drop(app.update_folder(Message::Classified {
            request: 2,
            path: "/other/second.md".into(),
            result: Ok(PathKind::File),
        }));
        assert!(!app.busy);
        assert!(matches!(app.overlay, Some(Overlay::Unsaved)));
        assert_eq!(app.path.as_deref(), Some(Path::new("/writing/note.md")));
    }

    #[test]
    fn late_move_completion_cannot_remap_a_replaced_document_session() {
        let mut app = app();
        let operation = app.begin_folder_operation(true, false);
        app.set_document(
            "Replacement".into(),
            Some("/other/replacement.md".into()),
            "replacement.md".into(),
        );
        drop(app.update_folder(Message::Finished {
            operation,
            result: Ok(Outcome::Move(file_ops::MoveResult {
                from: "/writing/note.md".into(),
                to: "/writing/renamed.md".into(),
                active_path: Some("/writing/renamed.md".into()),
                warning: None,
            })),
        }));
        assert_eq!(
            app.path.as_deref(),
            Some(Path::new("/other/replacement.md"))
        );
        assert_eq!(app.document.source(), "Replacement");
        assert_eq!(app.display_name, "replacement.md");
        assert!(!app.busy && !app.loading);
    }

    #[test]
    fn path_and_filename_validation_do_not_escape_the_folder() {
        assert!(validate_name("..", true).is_err());
        assert!(validate_name("../note", true).is_err());
        assert!(validate_name("a\\b", false).is_err());
        assert_eq!(validate_name("Café", true).unwrap(), "Café.md");
        assert_eq!(
            validate_name("note.MARKDOWN", true).unwrap(),
            "note.MARKDOWN"
        );
        assert_eq!(
            remap_path(
                Path::new("/writing/drafts/note.md"),
                Path::new("/writing/drafts"),
                Path::new("/writing/research")
            ),
            Some(PathBuf::from("/writing/research/note.md"))
        );
        assert!(
            remap_path(
                Path::new("/writing/drafts-2/note.md"),
                Path::new("/writing/drafts"),
                Path::new("/writing/research")
            )
            .is_none()
        );
    }
}
