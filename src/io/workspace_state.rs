//! Remembered folder workspaces, stored in Dosty's per-user data directory.
//!
//! This is a convenience cache rather than document data. Nothing is written
//! inside the writer's folders; unreadable, oversized, or malformed files read
//! as empty; and callers revalidate every path against the filesystem before
//! use. Deleting the directory forgets workspaces and nothing else.
//!
//! Layout below the data directory:
//!
//! ```text
//! workspaces/
//!   workspaces.toml      recent roots and the root to reopen on launch
//!   <root id>/state.toml how one workspace was left
//! ```

use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::storage::{self, FileError, atomic_write};

pub const MAX_RECENT_WORKSPACES: usize = 12;
const MAX_DOCUMENTS: usize = 64;
const MAX_EXPANDED: usize = 1_024;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const REGISTRY: &str = "workspaces.toml";
const STATE: &str = "state.toml";
const ID_LENGTH: usize = 16;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Registry {
    /// The workspace open when Dosty last closed, reopened on launch.
    pub last: Option<PathBuf>,
    /// Canonical workspace roots, most recently opened first.
    pub recent: Vec<PathBuf>,
}

impl Registry {
    pub fn opened(&mut self, root: &Path) {
        self.last = Some(root.to_owned());
        self.recent.retain(|path| path != root);
        self.recent.insert(0, root.to_owned());
        self.recent.truncate(MAX_RECENT_WORKSPACES);
    }

    /// Forget a root that no longer exists or was explicitly removed.
    pub fn forget(&mut self, root: &Path) {
        self.recent.retain(|path| path != root);
        if self.last.as_deref() == Some(root) {
            self.last = None;
        }
    }

    /// Forget every recent workspace except the one currently open.
    pub fn clear_recent(&mut self) {
        self.recent.retain(|path| Some(path) == self.last.as_ref());
    }

    fn sanitize(&mut self) {
        let valid = |path: &PathBuf| path.is_absolute() && path.to_str().is_some();
        self.last = self.last.take().filter(valid);
        self.recent.retain(valid);
        let mut seen = std::collections::HashSet::new();
        self.recent.retain(|path| seen.insert(path.clone()));
        self.recent.truncate(MAX_RECENT_WORKSPACES);
    }
}

/// How one workspace was left. Paths are relative to `root`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    /// The canonical root this state describes. A mismatch, such as a copied
    /// data directory, discards the state instead of applying another folder's.
    pub root: PathBuf,
    pub files_hidden: bool,
    pub outline_open: bool,
    /// Expanded folders.
    pub expanded: Vec<PathBuf>,
    /// Documents, most recently active first. The first is reopened.
    pub documents: Vec<DocumentState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentState {
    pub path: PathBuf,
    /// UTF-8 source offsets of the last selection. The caller validates them
    /// against the loaded source, which may have changed outside Dosty.
    #[serde(default)]
    pub anchor: usize,
    #[serde(default)]
    pub head: usize,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            ..Self::default()
        }
    }

    pub fn document(&self, relative: &Path) -> Option<&DocumentState> {
        self.documents
            .iter()
            .find(|document| document.path == relative)
    }

    /// Record `relative` as the most recently active document.
    pub fn visited(&mut self, relative: &Path, anchor: usize, head: usize) {
        if !portable_relative(relative) {
            return;
        }
        self.documents.retain(|document| document.path != relative);
        self.documents.insert(
            0,
            DocumentState {
                path: relative.to_owned(),
                anchor,
                head,
            },
        );
        self.documents.truncate(MAX_DOCUMENTS);
    }

    /// Follow a checked move or rename of `from` (a file or folder) to `to`.
    pub fn moved(&mut self, from: &Path, to: &Path) {
        let follow = |path: &mut PathBuf| {
            if let Ok(rest) = path.strip_prefix(from) {
                *path = if rest.as_os_str().is_empty() {
                    to.to_owned()
                } else {
                    to.join(rest)
                };
            }
        };
        self.expanded.iter_mut().for_each(follow);
        self.documents
            .iter_mut()
            .for_each(|document| follow(&mut document.path));
        self.sanitize();
    }

    fn sanitize(&mut self) {
        self.expanded.retain(|path| portable_relative(path));
        let mut seen = std::collections::HashSet::new();
        self.expanded.retain(|path| seen.insert(path.clone()));
        self.expanded.truncate(MAX_EXPANDED);
        self.documents
            .retain(|document| portable_relative(&document.path));
        let mut seen = std::collections::HashSet::new();
        self.documents
            .retain(|document| seen.insert(document.path.clone()));
        self.documents.truncate(MAX_DOCUMENTS);
    }
}

/// Stored paths must stay below their root and survive TOML (UTF-8) encoding.
fn portable_relative(path: &Path) -> bool {
    path.to_str().is_some()
        && path.components().next().is_some()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

pub fn directory() -> Option<PathBuf> {
    storage::preferences_path()?
        .parent()
        .map(|parent| parent.join("workspaces"))
}

fn id(root: &Path) -> String {
    blake3::hash(root.as_os_str().as_encoded_bytes()).to_hex()[..ID_LENGTH].to_owned()
}

fn is_id(name: &str) -> bool {
    name.len() == ID_LENGTH && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn read_bounded(path: &Path) -> Option<String> {
    if path.is_symlink() {
        return None;
    }
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() as u64 <= MAX_FILE_BYTES).then_some(text)
}

fn private_directory(path: &Path) -> Result<(), FileError> {
    if path.is_symlink() {
        return Err(FileError::Io(
            "Workspace storage must not be a symbolic link.".into(),
        ));
    }
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_toml(path: &Path, value: &impl Serialize) -> Result<(), FileError> {
    if path.is_symlink() {
        return Err(FileError::Io(
            "Workspace storage must not be a symbolic link.".into(),
        ));
    }
    let text = toml::to_string_pretty(value).map_err(|e| FileError::Io(e.to_string()))?;
    atomic_write(path, text.as_bytes())
}

fn load_registry_in(directory: &Path) -> Registry {
    let mut registry = read_bounded(&directory.join(REGISTRY))
        .and_then(|text| toml::from_str::<Registry>(&text).ok())
        .unwrap_or_default();
    registry.sanitize();
    registry
}

fn load_workspace_in(directory: &Path, root: &Path) -> Workspace {
    let workspace = read_bounded(&directory.join(id(root)).join(STATE))
        .and_then(|text| toml::from_str::<Workspace>(&text).ok())
        .filter(|workspace| workspace.root == root);
    let mut workspace = workspace.unwrap_or_else(|| Workspace::new(root.to_owned()));
    workspace.sanitize();
    workspace
}

/// Write `workspaces` (oldest first, so a later snapshot of the same root wins)
/// and then the registry.
fn save_in(
    directory: &Path,
    registry: &Registry,
    workspaces: &[Workspace],
) -> Result<(), FileError> {
    private_directory(directory)?;
    let mut registry = registry.clone();
    registry.sanitize();
    for workspace in workspaces
        .iter()
        .filter(|workspace| workspace.root.is_absolute() && workspace.root.to_str().is_some())
    {
        let mut workspace = workspace.clone();
        workspace.sanitize();
        let folder = directory.join(id(&workspace.root));
        private_directory(&folder)?;
        write_toml(&folder.join(STATE), &workspace)?;
    }
    write_toml(&directory.join(REGISTRY), &registry)?;
    prune(directory, &registry);
    Ok(())
}

/// Remove state for workspaces that are no longer remembered, so storage stays
/// bounded and clearing recent workspaces also forgets how they were left.
/// Only directories named like a root id are considered.
fn prune(directory: &Path, registry: &Registry) {
    let keep: std::collections::HashSet<_> = registry
        .recent
        .iter()
        .chain(&registry.last)
        .map(|root| id(root))
        .collect();
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let is_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_directory && is_id(name) && !keep.contains(name) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Read the registry on a worker. Without a data directory, nothing is
/// remembered.
pub async fn load_registry(directory: Option<PathBuf>) -> Registry {
    tokio::task::spawn_blocking(move || {
        directory.map_or_else(Registry::default, |directory| load_registry_in(&directory))
    })
    .await
    .unwrap_or_default()
}

/// Resolve `path` to a canonical folder and read how it was left.
pub async fn open(directory: Option<PathBuf>, path: PathBuf) -> Result<Workspace, FileError> {
    tokio::task::spawn_blocking(move || {
        let root = fs::canonicalize(&path)?;
        if !root.is_dir() {
            return Err(FileError::Io(
                "Choose a folder to open as a workspace.".into(),
            ));
        }
        Ok(directory.map_or_else(
            || Workspace::new(root.clone()),
            |directory| load_workspace_in(&directory, &root),
        ))
    })
    .await
    .map_err(|error| FileError::Io(error.to_string()))?
}

pub async fn save(
    directory: Option<PathBuf>,
    registry: Registry,
    workspaces: Vec<Workspace>,
) -> Result<(), FileError> {
    tokio::task::spawn_blocking(move || {
        let Some(directory) = directory else {
            return Ok(());
        };
        save_in(&directory, &registry, &workspaces)
    })
    .await
    .map_err(|error| FileError::Io(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(directory: &tempfile::TempDir, name: &str) -> PathBuf {
        let path = directory.path().join(name);
        fs::create_dir_all(&path).unwrap();
        fs::canonicalize(path).unwrap()
    }

    #[test]
    fn workspace_state_round_trips_and_recent_order_is_most_recent_first() {
        let data = tempfile::tempdir().unwrap();
        let folders = tempfile::tempdir().unwrap();
        let (essays, notes) = (root(&folders, "Essays"), root(&folders, "Notes"));
        let mut registry = Registry::default();
        registry.opened(&essays);
        registry.opened(&notes);
        registry.opened(&essays);
        let mut workspace = Workspace::new(essays.clone());
        workspace.outline_open = true;
        workspace.expanded.push("Drafts".into());
        workspace.visited(Path::new("Drafts/One.md"), 3, 9);
        workspace.visited(Path::new("Two.md"), 0, 0);
        workspace.visited(Path::new("Drafts/One.md"), 4, 4);
        save_in(data.path(), &registry, std::slice::from_ref(&workspace)).unwrap();

        let loaded = load_registry_in(data.path());
        assert_eq!(loaded.last.as_ref(), Some(&essays));
        assert_eq!(loaded.recent, [essays.clone(), notes.clone()]);
        let restored = load_workspace_in(data.path(), &essays);
        assert_eq!(restored, workspace);
        assert_eq!(restored.documents[0].path, Path::new("Drafts/One.md"));
        assert_eq!(restored.documents.len(), 2);
        assert_eq!(
            load_workspace_in(data.path(), &notes),
            Workspace::new(notes)
        );
    }

    #[test]
    fn malformed_foreign_or_escaping_state_is_ignored() {
        let data = tempfile::tempdir().unwrap();
        let folders = tempfile::tempdir().unwrap();
        let essays = root(&folders, "Essays");
        let other = root(&folders, "Other");
        assert_eq!(load_registry_in(data.path()), Registry::default());

        fs::write(data.path().join(REGISTRY), "recent = 7").unwrap();
        assert_eq!(load_registry_in(data.path()), Registry::default());
        fs::write(
            data.path().join(REGISTRY),
            "last = \"relative\"\nrecent = [\"relative\", \"/a\", \"/a\"]",
        )
        .unwrap();
        let registry = load_registry_in(data.path());
        assert_eq!(registry.last, None);
        assert_eq!(registry.recent.len(), usize::from(cfg!(unix)));

        // State copied from another root is never applied.
        let folder = data.path().join(id(&essays));
        fs::create_dir_all(&folder).unwrap();
        let foreign = Workspace {
            outline_open: true,
            ..Workspace::new(other)
        };
        fs::write(folder.join(STATE), toml::to_string(&foreign).unwrap()).unwrap();
        assert_eq!(
            load_workspace_in(data.path(), &essays),
            Workspace::new(essays.clone())
        );

        let escaping = format!(
            "root = {:?}\nexpanded = [\"../outside\", \"/absolute\", \"Kept\"]\n\
             [[documents]]\npath = \"../secret.md\"\n[[documents]]\npath = \"Kept.md\"\n",
            essays.to_str().unwrap()
        );
        fs::write(folder.join(STATE), escaping).unwrap();
        let workspace = load_workspace_in(data.path(), &essays);
        assert_eq!(workspace.expanded, [PathBuf::from("Kept")]);
        assert_eq!(workspace.documents.len(), 1);
        assert_eq!(workspace.documents[0].path, Path::new("Kept.md"));

        fs::write(folder.join(STATE), "x".repeat(MAX_FILE_BYTES as usize + 1)).unwrap();
        assert_eq!(
            load_workspace_in(data.path(), &essays),
            Workspace::new(essays)
        );
    }

    #[test]
    fn moves_follow_documents_and_folders_below_the_moved_path() {
        let mut workspace = Workspace::new("/w".into());
        workspace.expanded = vec!["Drafts".into(), "Drafts/Old".into(), "Notes".into()];
        workspace.visited(Path::new("Drafts/Old/a.md"), 1, 1);
        workspace.visited(Path::new("Drafts.md"), 0, 0);
        workspace.moved(Path::new("Drafts"), Path::new("Archive/Drafts"));
        assert_eq!(
            workspace.expanded,
            [
                PathBuf::from("Archive/Drafts"),
                "Archive/Drafts/Old".into(),
                "Notes".into()
            ]
        );
        let paths: Vec<_> = workspace.documents.iter().map(|d| d.path.clone()).collect();
        assert_eq!(
            paths,
            [PathBuf::from("Drafts.md"), "Archive/Drafts/Old/a.md".into()]
        );
    }

    #[test]
    fn storage_stays_bounded_and_clearing_recent_forgets_state() {
        let data = tempfile::tempdir().unwrap();
        let folders = tempfile::tempdir().unwrap();
        let mut registry = Registry::default();
        let roots: Vec<_> = (0..MAX_RECENT_WORKSPACES + 3)
            .map(|index| root(&folders, &format!("W{index}")))
            .collect();
        for root in &roots {
            registry.opened(root);
            save_in(data.path(), &registry, &[Workspace::new(root.clone())]).unwrap();
        }
        assert_eq!(registry.recent.len(), MAX_RECENT_WORKSPACES);
        let stored = || {
            fs::read_dir(data.path())
                .unwrap()
                .filter(|entry| entry.as_ref().unwrap().file_type().unwrap().is_dir())
                .count()
        };
        assert_eq!(stored(), MAX_RECENT_WORKSPACES);

        let unrelated = data.path().join("not-an-id");
        fs::create_dir(&unrelated).unwrap();
        registry.clear_recent();
        assert_eq!(registry.recent, [roots.last().unwrap().clone()]);
        save_in(data.path(), &registry, &[]).unwrap();
        assert_eq!(
            stored(),
            2,
            "the open workspace and unrelated folders remain"
        );
        assert!(unrelated.exists());
    }

    #[cfg(unix)]
    #[test]
    fn storage_is_private() {
        use std::os::unix::fs::PermissionsExt as _;
        let data = tempfile::tempdir().unwrap();
        let folders = tempfile::tempdir().unwrap();
        let essays = root(&folders, "Essays");
        let directory = data.path().join("workspaces");
        let mut registry = Registry::default();
        registry.opened(&essays);
        save_in(&directory, &registry, &[Workspace::new(essays.clone())]).unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&directory), 0o700);
        assert_eq!(mode(&directory.join(id(&essays))), 0o700);
        assert_eq!(mode(&directory.join(REGISTRY)), 0o600);
        assert_eq!(mode(&directory.join(id(&essays)).join(STATE)), 0o600);
    }
}
