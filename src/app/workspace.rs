//! The library of places.
//!
//! A folder added to Dosty becomes a place in the library and stays there until
//! it is removed. One place is open at a time: Dosty reopens the last one on
//! launch and restores which document was active and where its caret was, which
//! folders were expanded, and which sidebars were open. The open place follows
//! the document on the page. State is written to the per-user data directory on workers; see
//! [`crate::io::workspace_state`].

use std::path::{Path, PathBuf};

use iced::{Task, window};

use super::{App, Message as AppMessage, Pending};
use crate::document::selection::Selection;
use crate::io::storage::FileError;
use crate::io::workspace_state::{self, Registry, Workspace};

const LIBRARY_FULL: &str = "The project list is full. Remove a project to remember this folder.";

#[derive(Debug, Clone)]
pub enum Message {
    RegistryLoaded {
        registry: Registry,
        restore: bool,
    },
    Opened {
        request: u64,
        requested: PathBuf,
        result: Result<Workspace, FileError>,
    },
    OpenPlace(usize),
    Saved(Result<(), FileError>),
}

pub(super) struct State {
    /// Where workspace state is stored. Unit tests leave it unset so they never
    /// read or write the writer's real configuration.
    directory: Option<PathBuf>,
    registry: Registry,
    /// Writes wait for the stored registry so an early save cannot erase it.
    loaded: bool,
    current: Option<Workspace>,
    /// Workspaces that were switched away from or closed and still need their
    /// final state written.
    outgoing: Vec<Workspace>,
    opening: Option<Opening>,
    resolving: Option<OpenPurpose>,
    request: u64,
    writing: bool,
    pending: bool,
    close_after_save: Option<window::Id>,
}

struct Opening {
    workspace: Workspace,
    purpose: OpenPurpose,
    /// The workspace whose documents may be replaced by this one's.
    previous_root: Option<PathBuf>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum OpenPurpose {
    Restore,
    #[default]
    Switch,
    NewDraft,
}

impl Default for State {
    fn default() -> Self {
        Self {
            directory: if cfg!(test) {
                None
            } else {
                workspace_state::directory()
            },
            registry: Registry::default(),
            loaded: false,
            current: None,
            outgoing: Vec::new(),
            opening: None,
            resolving: None,
            request: 0,
            writing: false,
            pending: false,
            close_after_save: None,
        }
    }
}

impl State {
    pub(super) fn places(&self) -> &[PathBuf] {
        &self.registry.places
    }

    /// Whether the library has been read and no place is still being opened,
    /// so nothing remembered is about to arrive.
    pub(super) fn settled(&self) -> bool {
        self.loaded && !self.is_opening()
    }

    pub(super) fn is_opening(&self) -> bool {
        self.resolving.is_some() || self.opening.is_some()
    }

    pub(super) fn has_project_metadata(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|workspace| workspace.project.is_some())
    }

    pub(super) fn project_name(&self, root: &Path) -> String {
        self.current
            .as_ref()
            .filter(|workspace| workspace.root == root)
            .and_then(|workspace| workspace.project.as_ref()?.name.as_ref())
            .or_else(|| self.registry.project_names.get(root))
            .cloned()
            .unwrap_or_else(|| super::folder::name(root))
    }

    pub(super) fn project_label(&self, root: &Path) -> String {
        qualified_label(root, self.places(), |path| self.project_name(path))
    }

    pub(super) fn recent_projects(&self) -> impl Iterator<Item = (usize, &Path)> {
        let mut seen = std::collections::HashSet::new();
        self.registry
            .last
            .iter()
            .chain(&self.registry.activity)
            .chain(self.places())
            .filter(move |root| seen.insert(*root))
            .filter_map(|root| {
                let index = self.places().iter().position(|path| path == root)?;
                Some((index, root.as_path()))
            })
    }

    #[cfg(test)]
    pub(super) fn remember_for_preview(&mut self, roots: &[&str]) {
        for root in roots {
            let _ = self.registry.opened(Path::new(root));
        }
        self.loaded = true;
    }

    pub(super) fn pins(&self, root: &Path) -> &[PathBuf] {
        self.current
            .as_ref()
            .filter(|workspace| workspace.root == root)
            .map_or(&[], |workspace| workspace.pins.as_slice())
    }

    pub(super) fn pin_status(&self, path: &Path) -> Option<bool> {
        let workspace = self.current.as_ref()?;
        workspace.pin_status(path.strip_prefix(&workspace.root).ok()?)
    }

    /// Absolute paths of the current workspace's documents, most recent first.
    pub(super) fn recent_documents(&self) -> Vec<PathBuf> {
        self.current.as_ref().map_or_else(Vec::new, |workspace| {
            workspace
                .documents
                .iter()
                .map(|document| workspace.root.join(&document.path))
                .collect()
        })
    }
}

/// A short menu label: the folder name, qualified by its path only when two
/// places share a name.
pub(super) fn place_label(root: &Path, places: &[PathBuf]) -> String {
    qualified_label(root, places, super::folder::name)
}

fn qualified_label(root: &Path, places: &[PathBuf], name: impl Fn(&Path) -> String) -> String {
    let label = name(root);
    if places
        .iter()
        .filter(|other| other.as_path() != root)
        .all(|other| name(other) != label)
    {
        return label;
    }
    format!("{label} — {}", project_path(root))
}

pub(super) fn project_path(path: &Path) -> String {
    let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_owned());
    match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(relative) if relative.as_os_str().is_empty() => "~".to_owned(),
        Some(relative) => format!("~{}{}", std::path::MAIN_SEPARATOR, relative.display()),
        None => path.display().to_string(),
    }
}

impl App {
    /// Read the registry on launch. Without a document or folder argument, the
    /// last workspace is reopened.
    pub(super) fn workspace_launch(&self, restore: bool) -> Task<AppMessage> {
        let directory = self.workspace.directory.clone();
        Task::perform(workspace_state::load_registry(directory), move |registry| {
            AppMessage::Workspace(Message::RegistryLoaded { registry, restore })
        })
    }

    pub(super) fn update_workspace(&mut self, message: Message) -> Task<AppMessage> {
        match message {
            Message::RegistryLoaded { registry, restore } => {
                let mut registry = registry;
                // A folder opened before the registry arrived stays open.
                let kept = self
                    .workspace
                    .current
                    .as_ref()
                    .is_none_or(|workspace| registry.opened(&workspace.root));
                if !kept {
                    self.notification = Some(LIBRARY_FULL.into());
                }
                if let Some(workspace) = &self.workspace.current
                    && let Some(name) = workspace
                        .project
                        .as_ref()
                        .and_then(|project| project.name.as_ref())
                {
                    registry
                        .project_names
                        .insert(workspace.root.clone(), name.clone());
                }
                self.workspace.registry = registry;
                self.workspace.loaded = true;
                let reopen = self.workspace.registry.last.clone().filter(|_| {
                    restore
                        && self.page == super::home::Page::Startup
                        && self.folder.root().is_none()
                        && !self.workspace.is_opening()
                        && !self.busy
                        && self.session.is_unbound()
                        && !self.session.dirty
                });
                let save = if std::mem::take(&mut self.workspace.pending) {
                    self.persist_workspace()
                } else {
                    Task::none()
                };
                let open = reopen.map_or_else(Task::none, |root| {
                    self.open_workspace_for(root, OpenPurpose::Restore)
                });
                Task::batch([save, open])
            }
            Message::Opened {
                request,
                requested,
                result,
            } => {
                if request != self.workspace.request {
                    return Task::none();
                }
                let purpose = self.workspace.resolving.take().unwrap_or_default();
                if purpose == OpenPurpose::Restore && self.page != super::home::Page::Startup {
                    return Task::none();
                }
                match result {
                    Ok(workspace) => {
                        self.folder.prepare_reveal(!workspace.files_hidden);
                        let root = workspace.root.clone();
                        self.workspace.opening = Some(Opening {
                            workspace,
                            purpose,
                            previous_root: self.folder.root().map(Path::to_owned),
                        });
                        self.scan_folder(root)
                    }
                    Err(error) => {
                        tracing::info!("workspace {} unavailable: {error}", requested.display());
                        let remembered = self.workspace.registry.places.contains(&requested);
                        self.notification = Some(if remembered {
                            format!(
                                "“{}” could not be reopened. It may have been moved, renamed, or disconnected.",
                                place_label(&requested, &[])
                            )
                        } else {
                            error.to_string()
                        });
                        if remembered {
                            self.workspace.registry.forget(&requested);
                            return self.persist_workspace();
                        }
                        Task::none()
                    }
                }
            }
            Message::OpenPlace(index) => {
                let Some(root) = self.workspace.registry.places.get(index).cloned() else {
                    return Task::none();
                };
                if self.busy {
                    return Task::none();
                }
                if self.folder.root() == Some(root.as_path()) {
                    self.reveal_navigation();
                    return self.open_project_document(&root);
                }
                self.open_workspace(root)
            }
            Message::Saved(result) => {
                self.workspace.writing = false;
                if let Err(error) = result {
                    // Workspace state is a convenience cache; writing must
                    // never interrupt writing documents.
                    tracing::warn!("could not remember workspace state: {error}");
                }
                if let Some(id) = self.workspace.close_after_save.take() {
                    return self.close_after_workspace_save(id);
                }
                if std::mem::take(&mut self.workspace.pending) {
                    return self.persist_workspace();
                }
                Task::none()
            }
        }
    }

    /// Resolve a folder and read its remembered state, then scan it.
    pub(super) fn open_workspace(&mut self, path: PathBuf) -> Task<AppMessage> {
        self.open_workspace_for(path, OpenPurpose::Switch)
    }

    pub(super) fn open_project_workspace(
        &mut self,
        path: PathBuf,
        new_draft: bool,
    ) -> Task<AppMessage> {
        let purpose = if new_draft {
            OpenPurpose::NewDraft
        } else {
            OpenPurpose::Switch
        };
        self.open_workspace_for(path, purpose)
    }

    fn open_workspace_for(&mut self, path: PathBuf, purpose: OpenPurpose) -> Task<AppMessage> {
        if self.workspace.opening.take().is_some() {
            self.folder.cancel_scan();
        }
        self.workspace.resolving = Some(purpose);
        self.workspace.request = self.workspace.request.wrapping_add(1);
        let request = self.workspace.request;
        let directory = self.workspace.directory.clone();
        Task::perform(
            workspace_state::open(directory, path.clone()),
            move |result| {
                AppMessage::Workspace(Message::Opened {
                    request,
                    requested: path.clone(),
                    result,
                })
            },
        )
    }

    pub(super) fn workspace_scan_failed(&mut self) -> bool {
        self.workspace.opening.take().is_some()
    }

    /// Adopt a successfully scanned root as the current workspace, applying
    /// its remembered state when it was opened through [`Self::open_workspace`].
    pub(super) fn workspace_scanned(&mut self, root: &Path, complete: bool) -> Task<AppMessage> {
        let opening = self
            .workspace
            .opening
            .take_if(|opening| opening.workspace.root == root);
        if self
            .workspace
            .current
            .as_ref()
            .is_some_and(|current| current.root == root)
        {
            if let Some(opening) = opening {
                self.remember_project_name(&opening.workspace);
                if let Some(current) = &mut self.workspace.current {
                    current.project = opening.workspace.project;
                }
                let page = self.open_project_document(root);
                return Task::batch([page, self.persist_workspace()]);
            }
            return if complete && self.prune_workspace_pins() {
                self.persist_workspace()
            } else {
                Task::none()
            };
        }
        self.capture_workspace();
        if let Some(outgoing) = self.workspace.current.take() {
            self.workspace.outgoing.push(outgoing);
        }
        let (workspace, previous_root, purpose) = opening.map_or_else(
            || (Workspace::new(root.to_owned()), None, OpenPurpose::Switch),
            |opening| (opening.workspace, opening.previous_root, opening.purpose),
        );
        let remembered = workspace != Workspace::new(root.to_owned());
        if remembered {
            self.folder
                .restore_expanded(workspace.expanded.iter().map(|path| root.join(path)));
            self.chrome.files.open = !workspace.files_hidden;
            self.chrome.outline.open = workspace.outline_open;
        }
        // Reopen where the writer left off only when nothing would be lost or
        // displaced: an empty untitled page, or a clean document belonging to
        // the workspace being left.
        let replaceable = (purpose != OpenPurpose::Restore
            || self.page == super::home::Page::Startup)
            && !self.session.dirty
            && !self.busy
            && match self.session.path() {
                None => self.session.is_unbound() && self.session.document.source().is_empty(),
                Some(path) => previous_root.is_some_and(|previous| path.starts_with(previous)),
            };
        let reopen = replaceable
            .then(|| {
                workspace
                    .documents
                    .iter()
                    .map(|document| root.join(&document.path))
                    .find(|path| self.folder.has_file(path))
            })
            .flatten()
            .filter(|path| self.session.path() != Some(path));
        if !self.workspace.registry.opened(root) {
            self.notification = Some(LIBRARY_FULL.into());
        }
        self.remember_project_name(&workspace);
        self.workspace.current = Some(workspace);
        if complete {
            self.prune_workspace_pins();
        }
        let open = if purpose == OpenPurpose::NewDraft
            && !self.busy
            && self.overlay.is_none()
            && self.commit_cell()
        {
            self.install_session(super::session::Session::draft(Some(root.to_owned())));
            self.activate(0, false)
        } else {
            if reopen.is_none()
                && !self.is_home()
                && self.session.is_unbound()
                && !self.session.dirty
                && self.session.document.source().is_empty()
            {
                self.session.folder = Some(root.to_owned());
            }
            if let Some(path) = reopen {
                self.request(Pending::Open(Some(path)))
            } else if purpose == OpenPurpose::Switch {
                self.open_project_document(root)
            } else {
                Task::none()
            }
        };
        Task::batch([self.persist_workspace(), open])
    }

    fn open_project_document(&mut self, root: &Path) -> Task<AppMessage> {
        if self.return_to_document() || !self.is_home() {
            return Task::none();
        }
        self.page = super::home::Page::Document;
        self.session.folder = Some(root.to_owned());
        if self.overlay.is_none() {
            self.activate(0, false)
        } else {
            Task::none()
        }
    }

    fn remember_project_name(&mut self, workspace: &Workspace) {
        let names = &mut self.workspace.registry.project_names;
        if let Some(name) = workspace
            .project
            .as_ref()
            .and_then(|project| project.name.as_ref())
        {
            names.insert(workspace.root.clone(), name.clone());
        } else {
            names.remove(&workspace.root);
        }
    }

    /// Only a complete scan can confirm that a pin is no longer available.
    fn prune_workspace_pins(&mut self) -> bool {
        let Some(workspace) = &mut self.workspace.current else {
            return false;
        };
        let before = workspace.pins.len();
        workspace
            .pins
            .retain(|path| self.folder.has_file(&workspace.root.join(path)));
        workspace.pins.len() != before
    }

    pub(super) fn workspace_pin(&mut self, path: &Path, pinned: bool) -> Task<AppMessage> {
        let Some(workspace) = &mut self.workspace.current else {
            return Task::none();
        };
        let Ok(relative) = path.strip_prefix(&workspace.root) else {
            return Task::none();
        };
        match workspace.set_pin(relative, pinned) {
            Ok(()) => self.persist_workspace(),
            Err(error) => {
                self.notification = Some(error.into());
                Task::none()
            }
        }
    }

    /// Remove the open place from the library. Its remembered state is
    /// forgotten with it; the folder and its files are untouched.
    pub(super) fn place_removed(&mut self) -> Task<AppMessage> {
        self.workspace.opening = None;
        if let Some(removed) = self.workspace.current.take() {
            self.workspace.registry.forget(&removed.root);
        }
        self.workspace.registry.last = None;
        self.persist_workspace()
    }

    /// Open the place that holds the page's document, so the sidebar always
    /// shows where the writer is. A document outside every place leaves the
    /// open place as it is.
    pub(super) fn follow_document(&mut self) -> Task<AppMessage> {
        let Some(path) = self.session.path() else {
            return Task::none();
        };
        if self
            .folder
            .root()
            .is_some_and(|root| path.starts_with(root))
        {
            return Task::none();
        }
        // The deepest place wins when one place sits inside another.
        let place = self
            .workspace
            .registry
            .places
            .iter()
            .filter(|place| path.starts_with(place))
            .max_by_key(|place| place.components().count())
            .cloned();
        place.map_or_else(Task::none, |root| self.open_workspace(root))
    }

    /// Follow a checked move or rename below the current workspace.
    pub(super) fn workspace_moved(&mut self, from: &Path, to: &Path) -> Task<AppMessage> {
        let Some(workspace) = &mut self.workspace.current else {
            return Task::none();
        };
        let (Ok(from), Ok(to)) = (
            from.strip_prefix(&workspace.root),
            to.strip_prefix(&workspace.root),
        ) else {
            return Task::none();
        };
        workspace.moved(from, to);
        self.persist_workspace()
    }

    /// Record the active document's selection before another document replaces it.
    pub(super) fn workspace_document_leaving(&mut self) {
        self.capture_workspace();
    }

    /// Put the caret back where it was when this document was last active.
    /// Offsets are clamped to the loaded source, which may have changed since.
    pub(super) fn restore_workspace_caret(&mut self) -> bool {
        let Some((workspace, path)) = self.workspace.current.as_ref().zip(self.session.path())
        else {
            return false;
        };
        let Some(document) = path
            .strip_prefix(&workspace.root)
            .ok()
            .and_then(|relative| workspace.document(relative))
        else {
            return false;
        };
        let selection = Selection {
            anchor: document.anchor,
            head: document.head,
        }
        .clamp(self.session.document.source());
        if selection == Selection::default() {
            return false;
        }
        self.session.selection = self.projection.normalize_caret(selection);
        self.editor_focused = true;
        self.sync_editor();
        true
    }

    fn capture_workspace(&mut self) {
        let Some(workspace) = &mut self.workspace.current else {
            return;
        };
        workspace.expanded = self.folder.expanded_below(&workspace.root);
        workspace.files_hidden = !self.chrome.files.open;
        workspace.outline_open = self.chrome.outline.open;
        if let Some(relative) = self
            .session
            .path()
            .and_then(|path| path.strip_prefix(&workspace.root).ok())
        {
            workspace.visited(
                relative,
                self.session.selection.anchor,
                self.session.selection.head,
            );
        }
    }

    /// Write the registry and workspace state, coalescing overlapping requests
    /// so an older snapshot can never land after a newer one.
    pub(super) fn persist_workspace(&mut self) -> Task<AppMessage> {
        if !self.workspace.loaded || self.workspace.writing {
            self.workspace.pending = true;
            return Task::none();
        }
        self.workspace.writing = true;
        Task::perform(self.workspace_snapshot(), |result| {
            AppMessage::Workspace(Message::Saved(result))
        })
    }

    fn workspace_snapshot(
        &mut self,
    ) -> impl Future<Output = Result<(), FileError>> + Send + 'static {
        self.capture_workspace();
        let mut workspaces = std::mem::take(&mut self.workspace.outgoing);
        workspaces.extend(self.workspace.current.clone());
        workspace_state::save(
            self.workspace.directory.clone(),
            self.workspace.registry.clone(),
            workspaces,
        )
    }

    /// Remember the workspace, then close the window. An in-flight write
    /// finishes first so the final state is the one that remains.
    pub(super) fn close_after_workspace_save(&mut self, id: window::Id) -> Task<AppMessage> {
        if !self.workspace.loaded {
            return window::close(id);
        }
        if self.workspace.writing {
            self.workspace.close_after_save = Some(id);
            return Task::none();
        }
        self.workspace.writing = true;
        Task::perform(self.workspace_snapshot(), |result| {
            AppMessage::Workspace(Message::Saved(result))
        })
        .chain(window::close(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::application::folder;
    use crate::io::folder::{Entry, Snapshot};

    fn scanned(app: &mut App, root: &str, files: &[&str], directories: &[&str]) {
        let root = PathBuf::from(root);
        let entries = directories
            .iter()
            .map(|path| (path, true))
            .chain(files.iter().map(|path| (path, false)))
            .map(|(path, is_directory)| Entry {
                path: root.join(path),
                is_directory,
            })
            .collect();
        drop(app.update_folder(folder::Message::Scanned {
            generation: app.folder.generation(),
            result: Ok(Snapshot {
                root,
                entries,
                limited: false,
                warnings: Vec::new(),
            }),
        }));
    }

    fn opened(app: &mut App, workspace: Workspace) {
        let request = app.workspace.request;
        let requested = workspace.root.clone();
        drop(app.update_workspace(Message::Opened {
            request,
            requested,
            result: Ok(workspace),
        }));
    }

    #[test]
    fn remembered_layout_and_document_are_restored_into_an_empty_window() {
        let (mut app, _) = App::new();
        let mut remembered = Workspace::new("/writing".into());
        remembered.outline_open = true;
        remembered.files_hidden = false;
        remembered.expanded = vec!["Drafts".into()];
        remembered.visited(Path::new("Missing.md"), 0, 0);
        remembered.visited(Path::new("Drafts/Essay.md"), 2, 6);
        opened(&mut app, remembered);
        scanned(
            &mut app,
            "/writing",
            &["Drafts/Essay.md", "Notes.md"],
            &["Drafts"],
        );

        assert!(app.chrome.outline.open && app.chrome.files.open);
        assert_eq!(
            app.folder.expanded_below(Path::new("/writing")),
            [PathBuf::from("Drafts")]
        );
        assert!(app.loading, "the remembered document is opening");
        assert_eq!(app.workspace.registry.places, [PathBuf::from("/writing")]);
        assert_eq!(
            app.workspace.registry.last.as_deref(),
            Some(Path::new("/writing"))
        );
    }

    #[test]
    fn unsaved_writing_is_never_displaced_by_a_restored_document() {
        let (mut app, _) = App::new();
        app.set_document("Unsaved thought".into(), None, "Untitled.md".into());
        app.session.dirty = true;
        let mut remembered = Workspace::new("/writing".into());
        remembered.visited(Path::new("Notes.md"), 0, 0);
        opened(&mut app, remembered);
        scanned(&mut app, "/writing", &["Notes.md"], &[]);
        assert!(!app.loading && app.overlay.is_none());
        assert_eq!(app.session.document.source(), "Unsaved thought");
    }

    #[test]
    fn the_caret_returns_to_where_the_document_was_left() {
        let (mut app, _) = App::new();
        scanned(&mut app, "/writing", &["Notes.md"], &[]);
        app.set_document(
            "# Notes\n\nFirst 日本 line\n".into(),
            Some("/writing/Notes.md".into()),
            "Notes.md".into(),
        );
        // A selection inside a multibyte character is clamped to a boundary.
        app.workspace
            .current
            .as_mut()
            .unwrap()
            .visited(Path::new("Notes.md"), 9, 22);
        assert!(app.restore_workspace_caret());
        assert_eq!(app.session.selection.anchor, 9);
        assert!(
            app.session
                .document
                .source()
                .is_char_boundary(app.session.selection.head)
        );
        assert!(app.session.selection.head <= app.session.document.source().len());

        app.session.selection = Selection::caret(3);
        app.workspace_document_leaving();
        let document = app
            .workspace
            .current
            .as_ref()
            .unwrap()
            .document(Path::new("Notes.md"))
            .unwrap()
            .clone();
        assert_eq!((document.anchor, document.head), (3, 3));
    }

    #[test]
    fn moves_removal_and_unavailable_folders_update_the_library() {
        let (mut app, _) = App::new();
        assert!(app.workspace.registry.opened(Path::new("/gone")));
        scanned(&mut app, "/writing", &["Drafts/a.md"], &["Drafts"]);
        app.workspace
            .current
            .as_mut()
            .unwrap()
            .visited(Path::new("Drafts/a.md"), 0, 0);
        drop(app.workspace_moved(Path::new("/writing/Drafts"), Path::new("/writing/Archive")));
        assert_eq!(
            app.workspace.recent_documents(),
            [PathBuf::from("/writing/Archive/a.md")]
        );

        drop(app.update_folder(folder::Message::Remove));
        assert!(app.workspace.current.is_none());
        assert_eq!(app.workspace.registry.last, None);
        assert_eq!(
            app.workspace.registry.places,
            [PathBuf::from("/gone")],
            "a removed place leaves the library; other places stay"
        );
        assert!(app.workspace.outgoing.is_empty());

        drop(app.update_workspace(Message::Opened {
            request: app.workspace.request,
            requested: "/gone".into(),
            result: Err(FileError::Io("missing".into())),
        }));
        assert!(app.workspace.registry.places.is_empty());
        assert!(
            app.notification
                .as_deref()
                .is_some_and(|notice| notice.contains("“gone” could not be reopened"))
        );
    }

    #[test]
    fn the_open_place_follows_the_document_on_the_page() {
        let (mut app, _) = App::new();
        for place in [
            "/library/essays",
            "/library/essays/drafts",
            "/library/notes",
        ] {
            assert!(app.workspace.registry.opened(Path::new(place)));
        }
        scanned(&mut app, "/library/notes", &["a.md"], &[]);
        let follows = |app: &mut App, path: Option<&str>| {
            app.set_document("Text".into(), path.map(PathBuf::from), "Text.md".into());
            let before = app.workspace.request;
            drop(app.follow_document());
            app.workspace.request != before
        };

        assert!(!follows(&mut app, None), "a draft has no place to follow");
        assert!(!follows(&mut app, Some("/library/notes/a.md")));
        assert!(!follows(&mut app, Some("/elsewhere/loose.md")));
        assert!(!follows(&mut app, Some("/library/notes-old/b.md")));
        assert!(follows(&mut app, Some("/library/essays/one.md")));
        assert!(follows(&mut app, Some("/library/essays/drafts/two.md")));
    }

    #[test]
    fn stale_open_results_are_ignored() {
        let (mut app, _) = App::new();
        drop(app.open_workspace("/first".into()));
        let stale = app.workspace.request;
        drop(app.open_workspace("/second".into()));
        drop(app.update_workspace(Message::Opened {
            request: stale,
            requested: "/first".into(),
            result: Ok(Workspace::new("/first".into())),
        }));
        assert!(app.workspace.opening.is_none());
    }

    #[test]
    fn writes_wait_for_the_registry_and_never_overlap() {
        let (mut app, _) = App::new();
        drop(app.persist_workspace());
        assert!(app.workspace.pending && !app.workspace.writing);
        drop(app.update_workspace(Message::RegistryLoaded {
            registry: Registry::default(),
            restore: false,
        }));
        assert!(app.workspace.writing && !app.workspace.pending);
        drop(app.persist_workspace());
        assert!(app.workspace.pending);
        drop(app.update_workspace(Message::Saved(Ok(()))));
        assert!(app.workspace.writing && !app.workspace.pending);
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

    fn launched(data: &Path) -> App {
        let (mut app, tasks) = App::new();
        drop(tasks);
        app.workspace.directory = Some(data.to_owned());
        app
    }

    #[tokio::test]
    async fn legacy_projects_open_without_metadata_until_setup_is_confirmed() {
        use super::super::{Overlay, projects};

        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(folder.path()).unwrap();
        std::fs::create_dir(root.join("Drafts")).unwrap();
        std::fs::write(root.join("Drafts/Note.md"), "# Draft\n").unwrap();
        let mut registry = Registry::default();
        assert!(registry.opened(&root));
        workspace_state::save(Some(data.path().to_owned()), registry, vec![])
            .await
            .unwrap();
        let mut app = launched(data.path());
        let launch = app.workspace_launch(true);
        settle(&mut app, launch).await;
        assert_eq!(app.folder.root(), Some(root.as_path()));
        assert!(
            app.is_home(),
            "a remembered project with no resumable document shows Welcome"
        );
        assert!(app.folder.has_file(&root.join("Drafts/Note.md")));
        let manifest = root.join(".dosty/project.toml");
        assert!(!manifest.exists());

        let setup = app.update_message(AppMessage::Project(projects::Message::SetUp(root.clone())));
        settle(&mut app, setup).await;
        assert_eq!(app.overlay, Some(Overlay::Project));
        assert!(!manifest.exists());
        let confirm = app.update_message(AppMessage::Project(projects::Message::Confirm));
        settle(&mut app, confirm).await;
        assert!(app.overlay.is_none());
        assert!(manifest.is_file());
        assert!(app.workspace.has_project_metadata());
        assert!(
            !app.is_home(),
            "explicitly opening a project enters the editor"
        );
        assert!(
            !app.folder
                .entries()
                .iter()
                .any(|entry| entry.path.starts_with(root.join(".dosty")))
        );
    }

    #[tokio::test]
    async fn opening_a_markdown_file_directly_does_not_enter_its_remembered_project() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(folder.path()).unwrap();
        let path = root.join("Essay.md");
        std::fs::write(&path, "# Quiet\n").unwrap();
        crate::io::project::setup(root.clone(), None).await.unwrap();
        let mut registry = Registry::default();
        assert!(registry.opened(&root));
        workspace_state::save(Some(data.path().to_owned()), registry, vec![])
            .await
            .unwrap();
        let mut app = launched(data.path());
        let launch = app.workspace_launch(false);
        settle(&mut app, launch).await;
        let open = app.update_message(AppMessage::OpenPath(path.clone()));
        settle(&mut app, open).await;
        assert_eq!(
            app.session.path().map(|value| value.as_path()),
            Some(path.as_path())
        );
        assert_eq!(app.session.document.source(), "# Quiet\n");
        assert!(app.folder.root().is_none());
    }

    #[tokio::test]
    async fn a_new_project_opens_a_blank_draft_and_parks_previous_unsaved_writing() {
        use super::super::projects;

        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let parent = std::fs::canonicalize(folder.path()).unwrap();
        let mut app = launched(data.path());
        app.set_document("Keep this text".into(), None, "Draft.md".into());
        app.session.dirty = true;

        drop(app.update_message(AppMessage::Project(projects::Message::New)));
        drop(
            app.update_message(AppMessage::Project(projects::Message::Name(
                "Field Notes".into(),
            ))),
        );
        drop(app.update_message(AppMessage::Project(projects::Message::Browse)));
        drop(
            app.update_message(AppMessage::Project(projects::Message::LocationPicked(
                Some(parent.clone()),
            ))),
        );
        let create = app.update_message(AppMessage::Project(projects::Message::Confirm));
        settle(&mut app, create).await;
        let root = parent.join("Field Notes");
        assert!(root.join(".dosty/project.toml").is_file());
        assert_eq!(app.folder.root(), Some(root.as_path()));
        assert_eq!(app.session.folder, Some(root));
        assert!(app.session.document.source().is_empty());
        assert!(
            app.background
                .dirty()
                .any(|session| session.document.source() == "Keep this text")
        );
    }

    /// Quit and relaunch through the real worker, scan, and load paths.
    #[tokio::test]
    async fn a_relaunch_returns_to_the_same_folder_document_caret_and_layout() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(folder.path()).unwrap();
        std::fs::create_dir_all(root.join("Drafts/Old")).unwrap();
        let essay = root.join("Drafts/Essay.md");
        std::fs::write(&essay, "# Essay\n\nHello 日本 world\n").unwrap();
        std::fs::write(root.join("Notes.md"), "# Notes\n").unwrap();
        std::fs::write(root.join("Drafts/Old/Archive.md"), "old").unwrap();

        let mut app = launched(data.path());
        let launch = app.workspace_launch(true);
        settle(&mut app, launch).await;
        assert_eq!(app.folder.root(), None, "nothing is remembered yet");
        let open = app.open_workspace(folder.path().to_owned());
        settle(&mut app, open).await;
        assert_eq!(app.folder.root(), Some(root.as_path()));
        let open = app.update_message(AppMessage::OpenPath(essay.clone()));
        settle(&mut app, open).await;
        assert_eq!(app.session.path(), Some(&essay));
        app.session.selection = Selection {
            anchor: 9,
            head: 14,
        };
        let toggled = app.update_message(AppMessage::ToggleSidebar(super::super::Sidebar::Outline));
        settle(&mut app, toggled).await;
        let expand = app.update_folder(folder::Message::Tree(
            crate::editor::file_tree::Action::Toggle(root.join("Drafts/Old")),
        ));
        settle(&mut app, expand).await;
        let close = app.update_message(AppMessage::CloseRequested(window::Id::unique()));
        settle(&mut app, close).await;
        assert!(
            !app.workspace.writing,
            "the final state was written before closing"
        );

        let mut app = launched(data.path());
        let launch = app.workspace_launch(true);
        settle(&mut app, launch).await;
        assert_eq!(app.folder.root(), Some(root.as_path()));
        assert_eq!(app.session.path(), Some(&essay));
        assert_eq!(
            app.session.selection,
            Selection {
                anchor: 9,
                head: 14
            }
        );
        assert!(app.chrome.files.open && app.chrome.outline.open);
        assert!(
            app.folder
                .expanded_below(&root)
                .contains(&PathBuf::from("Drafts/Old"))
        );
        assert_eq!(app.workspace.places(), std::slice::from_ref(&root));
        assert_eq!(
            app.workspace.recent_documents(),
            std::slice::from_ref(&essay),
            "Quick Open lists the recent document first"
        );

        // Quick Open preselects the previous document, so Enter switches back.
        let notes = root.join("Notes.md");
        let open = app.update_message(AppMessage::OpenPath(notes.clone()));
        settle(&mut app, open).await;
        assert_eq!(app.session.path(), Some(&notes));
        let quick_open = super::super::quick_open::Message::Open;
        drop(app.update_quick_open(quick_open));
        let accept = app.update_quick_open(super::super::quick_open::Message::Accept);
        settle(&mut app, accept).await;
        assert_eq!(app.session.path(), Some(&essay));

        // A document argument opens that document instead.
        let mut app = launched(data.path());
        let launch = app.workspace_launch(false);
        settle(&mut app, launch).await;
        assert_eq!(app.folder.root(), None);
        assert_eq!(app.workspace.places(), [root]);
    }

    #[test]
    fn place_labels_disambiguate_shared_names() {
        let recent = [
            PathBuf::from("/a/Essays"),
            "/b/Essays".into(),
            "/c/Notes".into(),
        ];
        assert_eq!(place_label(&recent[2], &recent), "Notes");
        assert!(place_label(&recent[0], &recent).starts_with("Essays — "));
        assert_ne!(
            place_label(&recent[0], &recent),
            place_label(&recent[1], &recent)
        );
    }
}
