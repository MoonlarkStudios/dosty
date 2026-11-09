//! Bounded path-only folder discovery, kept off the UI event path.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::storage::FileError;

const MAX_ENTRIES: usize = 32_768;
const MAX_DEPTH: usize = 48;
const MAX_PATH_BYTES: usize = 8 * 1024 * 1024;
const MAX_WARNINGS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub is_directory: bool,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    pub limited: bool,
    pub warnings: Vec<String>,
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
}

/// Hidden and generated directories are deliberately excluded. This does not
/// apply a repository's ignore rules or hide arbitrary directories such as vendor.
fn excluded_directory(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        let name = name.to_string_lossy();
        name.starts_with('.')
            || name.eq_ignore_ascii_case("node_modules")
            || name.eq_ignore_ascii_case("target")
            || name.to_ascii_lowercase().ends_with(".assets")
    })
}

pub async fn scan(root: PathBuf, cancelled: Arc<AtomicBool>) -> Result<Snapshot, FileError> {
    tokio::task::spawn_blocking(move || scan_blocking(&root, &cancelled))
        .await
        .map_err(|error| FileError::Io(error.to_string()))?
}

fn scan_blocking(root: &Path, cancelled: &AtomicBool) -> Result<Snapshot, FileError> {
    let root = fs::canonicalize(root)?;
    if !root.is_dir() {
        return Err(FileError::Io(
            "Choose a folder to browse Markdown files.".into(),
        ));
    }
    let mut snapshot = Snapshot {
        root: root.clone(),
        entries: Vec::new(),
        limited: false,
        warnings: Vec::new(),
    };
    let mut pending = vec![(root.clone(), 0)];
    let mut visited = 0;
    let mut path_bytes = 0;
    while let Some((directory, depth)) = pending.pop() {
        if cancelled.load(Ordering::Relaxed) {
            return Err(FileError::Io("Folder scan cancelled.".into()));
        }
        let contents = match fs::read_dir(&directory) {
            Ok(contents) => contents,
            Err(error) if directory == root => return Err(error.into()),
            Err(error) => {
                if snapshot.warnings.len() < MAX_WARNINGS {
                    snapshot
                        .warnings
                        .push(format!("Cannot read {}: {error}", directory.display()));
                }
                continue;
            }
        };
        for entry in contents {
            if cancelled.load(Ordering::Relaxed) {
                return Err(FileError::Io("Folder scan cancelled.".into()));
            }
            visited += 1;
            if visited > MAX_ENTRIES {
                snapshot.limited = true;
                pending.clear();
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    if snapshot.warnings.len() < MAX_WARNINGS {
                        snapshot.warnings.push(error.to_string());
                    }
                    continue;
                }
            };
            let path = entry.path();
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(error) => {
                    if snapshot.warnings.len() < MAX_WARNINGS {
                        snapshot
                            .warnings
                            .push(format!("Cannot inspect {}: {error}", path.display()));
                    }
                    continue;
                }
            };
            // In particular, never recurse through directory aliases or loops.
            if kind.is_symlink() {
                continue;
            }
            let directory = kind.is_dir();
            if directory {
                if excluded_directory(&path) {
                    continue;
                }
            } else if !kind.is_file()
                || !is_markdown(&path)
                || entry.file_name().to_string_lossy().starts_with('.')
            {
                continue;
            }
            path_bytes += path.as_os_str().len();
            if path_bytes > MAX_PATH_BYTES {
                snapshot.limited = true;
                pending.clear();
                break;
            }
            snapshot.entries.push(Entry {
                path: path.clone(),
                is_directory: directory,
            });
            if directory {
                if depth + 1 >= MAX_DEPTH {
                    snapshot.limited = true;
                } else {
                    pending.push((path, depth + 1));
                }
            }
        }
    }
    snapshot
        .entries
        .sort_unstable_by(|a, b| a.path.cmp(&b.path));
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_empty_folders_and_markdown_without_opening_contents() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("Empty")).unwrap();
        fs::create_dir(directory.path().join("Research")).unwrap();
        fs::write(directory.path().join("Research/Notes.MARKDOWN"), [0xff]).unwrap();
        fs::write(directory.path().join("Essay.md"), "hello").unwrap();
        fs::write(directory.path().join("Plain.txt"), "text").unwrap();
        for name in [".git", "target", "node_modules", "Essay.assets"] {
            fs::create_dir(directory.path().join(name)).unwrap();
            fs::write(directory.path().join(name).join("hidden.md"), "hidden").unwrap();
        }
        let snapshot = scan_blocking(directory.path(), &AtomicBool::new(false)).unwrap();
        let paths = snapshot
            .entries
            .iter()
            .map(|entry| entry.path.strip_prefix(&snapshot.root).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                Path::new("Empty"),
                Path::new("Essay.md"),
                Path::new("Research"),
                Path::new("Research/Notes.MARKDOWN")
            ]
        );
        assert!(snapshot.entries[0].is_directory);
        assert!(!snapshot.limited);
        assert!(snapshot.warnings.is_empty());
    }

    #[test]
    fn cancellation_does_not_publish_a_partial_success() {
        let directory = tempfile::tempdir().unwrap();
        assert!(scan_blocking(directory.path(), &AtomicBool::new(true)).is_err());
    }

    #[test]
    fn very_deep_folders_report_the_discovery_limit() {
        let directory = tempfile::tempdir().unwrap();
        let mut path = directory.path().to_owned();
        for _ in 0..=MAX_DEPTH {
            path.push("nested");
            fs::create_dir(&path).unwrap();
        }
        fs::write(path.join("deep.md"), "deep").unwrap();
        let snapshot = scan_blocking(directory.path(), &AtomicBool::new(false)).unwrap();
        assert!(snapshot.limited);
        assert!(
            !snapshot
                .entries
                .iter()
                .any(|entry| is_markdown(&entry.path))
        );
    }

    #[cfg(unix)]
    #[test]
    fn directory_and_document_symlinks_are_not_followed() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("note.md"), "note").unwrap();
        std::os::unix::fs::symlink(directory.path(), directory.path().join("loop")).unwrap();
        std::os::unix::fs::symlink(
            directory.path().join("note.md"),
            directory.path().join("alias.md"),
        )
        .unwrap();
        let snapshot = scan_blocking(directory.path(), &AtomicBool::new(false)).unwrap();
        assert_eq!(snapshot.entries.len(), 1);
    }
}
