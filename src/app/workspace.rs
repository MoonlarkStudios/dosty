//! Remembered folder workspaces.
//!
//! A folder opened in Dosty becomes a workspace: Dosty reopens the last one on
//! launch, restores which document was active and where its caret was, which
//! folders were expanded, and which sidebars were open, and offers recent
//! folders. State is written to the per-user data directory on workers; see
//! [`crate::io::workspace_state`].

use std::path::{Path, PathBuf};

use iced::{Task, window};

use super::{App, Message as AppMessage, Pending};
use crate::document::selection::Selection;
use crate::io::storage::FileError;
use crate::io::workspace_state::{self, Registry, Workspace};

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
    OpenRecent(usize),
    ClearRecent,
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
    request: u64,
    writing: bool,
    pending: bool,
    close_after_save: Option<window::Id>,
}

struct Opening {
    workspace: Workspace,
    /// The workspace whose documents may be replaced by this one's.
    previous_root: Option<PathBuf>,
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
            request: 0,
            writing: false,
            pending: false,
            close_after_save: None,
        }
    }
}

impl State {
    pub(super) fn recent(&self) -> &[PathBuf] {
        &self.registry.recent
    }

    #[cfg(test)]
    pub(super) fn remember_for_preview(&mut self, roots: &[&str]) {
        for root in roots.iter().rev() {
            self.registry.opened(Path::new(root));
        }
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

/// A short menu label: the folder name, qualified by its parent only when two
/// recent folders share a name.
pub(super) fn recent_label(root: &Path, recent: &[PathBuf]) -> String {
    let name = |path: &Path| {
        path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into(),
        )
    };
    let label = name(root);
    if recent
        .iter()
        .filter(|other| other.as_path() != root)
        .all(|other| name(other) != label)
    {
        return label;
    }
    let parent = root.parent().unwrap_or(root);
    let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_owned());
    let parent = match home
        .as_deref()
        .and_then(|home| parent.strip_prefix(home).ok())
    {
        Some(relative) if relative.as_os_str().is_empty() => "~".to_owned(),
        Some(relative) => format!("~{}{}", std::path::MAIN_SEPARATOR, relative.display()),
        None => parent.display().to_string(),
    };
    format!("{label} — {parent}")
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
                // A folder opened before the registry arrived stays the latest.
                if let Some(workspace) = &self.workspace.current {
                    registry.opened(&workspace.root);
                }
                self.workspace.registry = registry;
                self.workspace.loaded = true;
                let reopen = self.workspace.registry.last.clone().filter(|_| {
                    restore
                        && self.folder.root().is_none()
                        && self.workspace.opening.is_none()
                        && self.path.is_none()
                        && !self.dirty
                });
                let save = if std::mem::take(&mut self.workspace.pending) {
                    self.persist_workspace()
                } else {
                    Task::none()
                };
                let open = reopen.map_or_else(Task::none, |root| self.open_workspace(root));
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
                match result {
                    Ok(workspace) => {
                        self.folder.prepare_reveal(!workspace.files_hidden);
                        let root = workspace.root.clone();
                        self.workspace.opening = Some(Opening {
                            workspace,
                            previous_root: self.folder.root().map(Path::to_owned),
                        });
                        self.scan_folder(root)
                    }
                    Err(error) => {
                        tracing::info!("workspace {} unavailable: {error}", requested.display());
                        let remembered = self.workspace.registry.recent.contains(&requested);
                        self.notification = Some(if remembered {
                            format!(
                                "“{}” could not be reopened. It may have been moved, renamed, or disconnected.",
                                recent_label(&requested, &[])
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
            Message::OpenRecent(index) => {
                let Some(root) = self.workspace.registry.recent.get(index).cloned() else {
                    return Task::none();
                };
                if self.busy {
                    return Task::none();
                }
                if self.folder.root() == Some(root.as_path()) {
                    self.reveal_navigation();
                    return Task::none();
                }
                self.open_workspace(root)
            }
            Message::ClearRecent => {
                self.workspace.registry.clear_recent();
                self.persist_workspace()
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

    /// Adopt a successfully scanned root as the current workspace, applying
    /// its remembered state when it was opened through [`Self::open_workspace`].
    pub(super) fn workspace_scanned(&mut self, root: &Path) -> Task<AppMessage> {
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
            return Task::none();
        }
        self.capture_workspace();
        if let Some(outgoing) = self.workspace.current.take() {
            self.workspace.outgoing.push(outgoing);
        }
        let (workspace, previous_root) = opening.map_or_else(
            || (Workspace::new(root.to_owned()), None),
            |opening| (opening.workspace, opening.previous_root),
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
        let replaceable = !self.dirty
            && !self.busy
            && match &self.path {
                None => self.document.source().is_empty(),
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
            .filter(|path| self.path.as_ref() != Some(path));
        self.workspace.registry.opened(root);
        self.workspace.current = Some(workspace);
        let open = reopen.map_or_else(Task::none, |path| self.request(Pending::Open(Some(path))));
        Task::batch([self.persist_workspace(), open])
    }

    pub(super) fn workspace_closed(&mut self) -> Task<AppMessage> {
        self.workspace.opening = None;
        self.capture_workspace();
        if let Some(outgoing) = self.workspace.current.take() {
            self.workspace.outgoing.push(outgoing);
        }
        self.workspace.registry.last = None;
        self.persist_workspace()
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
        let Some((workspace, path)) = self.workspace.current.as_ref().zip(self.path.as_ref())
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
        .clamp(self.document.source());
        if selection == Selection::default() {
            return false;
        }
        self.selection = self.projection.normalize_caret(selection);
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
            .path
            .as_ref()
            .and_then(|path| path.strip_prefix(&workspace.root).ok())
        {
            workspace.visited(relative, self.selection.anchor, self.selection.head);
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
        assert_eq!(app.workspace.registry.recent, [PathBuf::from("/writing")]);
        assert_eq!(
            app.workspace.registry.last.as_deref(),
            Some(Path::new("/writing"))
        );
    }

    #[test]
    fn unsaved_writing_is_never_displaced_by_a_restored_document() {
        let (mut app, _) = App::new();
        app.set_document("Unsaved thought".into(), None, "Untitled.md".into());
        app.dirty = true;
        let mut remembered = Workspace::new("/writing".into());
        remembered.visited(Path::new("Notes.md"), 0, 0);
        opened(&mut app, remembered);
        scanned(&mut app, "/writing", &["Notes.md"], &[]);
        assert!(!app.loading && app.overlay.is_none());
        assert_eq!(app.document.source(), "Unsaved thought");
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
        assert_eq!(app.selection.anchor, 9);
        assert!(app.document.source().is_char_boundary(app.selection.head));
        assert!(app.selection.head <= app.document.source().len());

        app.selection = Selection::caret(3);
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
    fn moves_closing_and_unavailable_folders_update_the_registry() {
        let (mut app, _) = App::new();
        app.workspace.registry.opened(Path::new("/gone"));
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

        drop(app.update_folder(folder::Message::Close));
        assert!(app.workspace.current.is_none());
        assert_eq!(app.workspace.registry.last, None);
        assert_eq!(app.workspace.outgoing.len(), 1);

        drop(app.update_workspace(Message::Opened {
            request: app.workspace.request,
            requested: "/gone".into(),
            result: Err(FileError::Io("missing".into())),
        }));
        assert_eq!(app.workspace.registry.recent, [PathBuf::from("/writing")]);
        assert!(
            app.notification
                .as_deref()
                .is_some_and(|notice| notice.contains("“gone” could not be reopened"))
        );
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
        assert_eq!(app.path.as_ref(), Some(&essay));
        app.selection = Selection {
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
        assert_eq!(app.path.as_ref(), Some(&essay));
        assert_eq!(
            app.selection,
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
        assert_eq!(app.workspace.recent(), std::slice::from_ref(&root));
        assert_eq!(
            app.workspace.recent_documents(),
            std::slice::from_ref(&essay),
            "Quick Open lists the recent document first"
        );

        // Quick Open preselects the previous document, so Enter switches back.
        let notes = root.join("Notes.md");
        let open = app.update_message(AppMessage::OpenPath(notes.clone()));
        settle(&mut app, open).await;
        assert_eq!(app.path.as_ref(), Some(&notes));
        let quick_open = super::super::quick_open::Message::Open;
        drop(app.update_quick_open(quick_open));
        let accept = app.update_quick_open(super::super::quick_open::Message::Accept);
        settle(&mut app, accept).await;
        assert_eq!(app.path.as_ref(), Some(&essay));

        // A document argument opens that document instead.
        let mut app = launched(data.path());
        let launch = app.workspace_launch(false);
        settle(&mut app, launch).await;
        assert_eq!(app.folder.root(), None);
        assert_eq!(app.workspace.recent(), [root]);
    }

    #[test]
    fn recent_labels_disambiguate_shared_names() {
        let recent = [
            PathBuf::from("/a/Essays"),
            "/b/Essays".into(),
            "/c/Notes".into(),
        ];
        assert_eq!(recent_label(&recent[2], &recent), "Notes");
        assert!(recent_label(&recent[0], &recent).starts_with("Essays — "));
        assert_ne!(
            recent_label(&recent[0], &recent),
            recent_label(&recent[1], &recent)
        );
    }
}
