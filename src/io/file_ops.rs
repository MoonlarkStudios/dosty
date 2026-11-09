//! Checked document organization. All entry points run on blocking workers.
//!
//! Moving a saved file never publishes the editor's unsaved source. Relative
//! assets for both source snapshots and undo are copied before path publication.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use pulldown_cmark::{Event, Parser, Tag};

use super::assets::{self, History};
use super::collaboration_store::{self, MoveSnapshot};
use super::folder::is_markdown;
use super::storage::{FileError, MAX_FILE_BYTES};
use crate::document::block_parser::MARKDOWN_OPTIONS;
use crate::document::collaboration::MAX_STATE_BYTES;

const MAX_MOVE_ENTRIES: usize = 32_768;
const MAX_MOVE_DIRECTORIES: usize = 128;
const MAX_MOVE_DOCUMENTS: usize = 128;
const MAX_MOVE_SOURCE_BYTES: usize = 128 * 1024 * 1024;
const MAX_NAME_BYTES: usize = 240;

#[derive(Debug, Clone)]
pub struct ActiveDocument {
    pub path: PathBuf,
    pub saved_source: String,
    pub saved_version: Option<String>,
    pub source: String,
    pub history: History,
}

#[derive(Debug, Clone)]
pub struct MoveResult {
    pub from: PathBuf,
    pub to: PathBuf,
    pub active_path: Option<PathBuf>,
    pub warning: Option<String>,
}

fn error(message: impl Into<String>) -> FileError {
    FileError::Io(message.into())
}

fn name(value: &str, document: bool) -> Result<String, FileError> {
    let value = value.trim();
    if value.is_empty()
        || value.starts_with('.')
        || value.ends_with('.')
        || value.ends_with(' ')
        || value.contains(['/', '\\', '\0'])
        || !matches!(
            Path::new(value).components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err(error("Use a visible filename without path separators."));
    }
    let mut value = value.to_owned();
    if document && !is_markdown(Path::new(&value)) {
        value.push_str(".md");
    }
    if value.len() > MAX_NAME_BYTES {
        return Err(error("This name is too long. Use at most 240 UTF-8 bytes."));
    }
    if !document
        && (value.eq_ignore_ascii_case("target")
            || value.eq_ignore_ascii_case("node_modules")
            || value.to_ascii_lowercase().ends_with(".assets"))
    {
        return Err(error(
            "Choose a folder name outside Dosty's generated-folder exclusions.",
        ));
    }
    Ok(value)
}

fn real_directory(path: &Path) -> Result<PathBuf, FileError> {
    let path = fs::canonicalize(if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    })?;
    if !path.is_dir() {
        return Err(error("The destination must be a folder."));
    }
    Ok(path)
}

fn absent(path: &Path) -> Result<(), FileError> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
        Ok(_) => Err(error("That name already exists. Nothing was replaced.")),
    }
}

fn destination_clear(path: &Path, document: bool) -> Result<(), FileError> {
    absent(path)?;
    if document {
        for sibling in collaboration_store::move_metadata_paths(path)? {
            absent(&sibling)?;
        }
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), FileError> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub async fn create_document(parent: PathBuf, value: String) -> Result<PathBuf, FileError> {
    tokio::task::spawn_blocking(move || create_document_blocking(&parent, &value))
        .await
        .map_err(|e| error(e.to_string()))?
}

fn create_document_blocking(parent: &Path, value: &str) -> Result<PathBuf, FileError> {
    let parent = real_directory(parent)?;
    let destination = parent.join(name(value, true)?);
    let _lock = collaboration_store::directory_write_lock(&parent)?;
    destination_clear(&destination, true)?;
    let temporary = tempfile::NamedTempFile::new_in(&parent)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(&destination)
        .map_err(|e| error(e.error.to_string()))?;
    sync_directory(&parent)?;
    Ok(destination)
}

pub async fn create_folder(parent: PathBuf, value: String) -> Result<PathBuf, FileError> {
    tokio::task::spawn_blocking(move || create_folder_blocking(&parent, &value))
        .await
        .map_err(|e| error(e.to_string()))?
}

fn create_folder_blocking(parent: &Path, value: &str) -> Result<PathBuf, FileError> {
    let parent = real_directory(parent)?;
    let destination = parent.join(name(value, false)?);
    let _lock = collaboration_store::directory_write_lock(&parent)?;
    fs::create_dir(&destination)?;
    sync_directory(&parent)?;
    Ok(destination)
}

#[derive(Debug, PartialEq, Eq)]
struct Manifest {
    directories: BTreeSet<PathBuf>,
    documents: BTreeSet<PathBuf>,
    entries: BTreeSet<(PathBuf, bool)>,
}

fn manifest(root: &Path, active: Option<&Path>) -> Result<Manifest, FileError> {
    let mut manifest = Manifest {
        directories: BTreeSet::from([root.to_owned()]),
        documents: BTreeSet::new(),
        entries: BTreeSet::new(),
    };
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            if entry.file_name() == ".dosty-folder-lock" {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
                return Err(error(
                    "Move a folder containing only real files and folders; symbolic links are not relocated.",
                ));
            }
            let path = entry.path();
            manifest.entries.insert((path.clone(), kind.is_dir()));
            if manifest.entries.len() > MAX_MOVE_ENTRIES {
                return Err(error(
                    "This folder is too large to organize in Dosty. Move a smaller folder.",
                ));
            }
            if kind.is_dir() {
                manifest.directories.insert(path.clone());
                if manifest.directories.len() > MAX_MOVE_DIRECTORIES {
                    return Err(error(
                        "Move a folder with at most 128 subdirectories at a time.",
                    ));
                }
                pending.push(path);
            } else if is_markdown(&path) || active == Some(path.as_path()) {
                manifest.documents.insert(path);
                if manifest.documents.len() > MAX_MOVE_DOCUMENTS {
                    return Err(error("Move at most 128 Markdown documents at a time."));
                }
            }
        }
    }
    Ok(manifest)
}

fn locks(directories: BTreeSet<PathBuf>) -> Result<Vec<(PathBuf, File)>, FileError> {
    // Canonical absolute lexical order is shared by all organization operations.
    // Saves take their one directory lock before the document checkpoint lock.
    directories
        .into_iter()
        .map(|directory| {
            let lock = collaboration_store::directory_write_lock(&directory)?;
            Ok((directory, lock))
        })
        .collect()
}

pub async fn move_item(
    from: PathBuf,
    to: PathBuf,
    active: Option<ActiveDocument>,
) -> Result<MoveResult, FileError> {
    tokio::task::spawn_blocking(move || move_item_blocking(&from, &to, active.as_ref()))
        .await
        .map_err(|e| error(e.to_string()))?
}

fn move_item_blocking(
    from: &Path,
    to: &Path,
    active: Option<&ActiveDocument>,
) -> Result<MoveResult, FileError> {
    let kind = fs::symlink_metadata(from)?.file_type();
    if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
        return Err(error("Only real files and folders can be organized."));
    }
    let from = fs::canonicalize(from)?;
    let filename = to
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| error("Choose a valid destination name."))?;
    let filename = name(filename, false)?;
    if kind.is_file() && is_markdown(&from) && !is_markdown(Path::new(&filename)) {
        return Err(error(
            "Keep the .md or .markdown extension when renaming a document.",
        ));
    }
    let to_parent = real_directory(to.parent().unwrap_or(Path::new(".")))?;
    let to = to_parent.join(filename);
    if names_same_entry(&from, &to)? {
        return rename_through_staging(&from, &to, active);
    }
    // An unavailable active file must not prevent organizing an unrelated item.
    // Resolve a missing leaf through its parent so an ancestor move still gets
    // the saved-baseline conflict rather than silently remapping a deleted file.
    let active_path = active.and_then(|active| {
        fs::canonicalize(&active.path).ok().or_else(|| {
            Some(
                fs::canonicalize(active.path.parent()?)
                    .ok()?
                    .join(active.path.file_name()?),
            )
        })
    });
    let remapped = active_path.as_ref().and_then(|path| {
        path.strip_prefix(&from).ok().map(|relative| {
            if relative.as_os_str().is_empty() {
                to.clone()
            } else {
                to.join(relative)
            }
        })
    });
    if remapped.is_some() && active_path.as_ref().is_some_and(|path| !path.exists()) {
        return Err(FileError::Conflict);
    }
    if from == to {
        return Ok(MoveResult {
            from,
            to,
            active_path: remapped,
            warning: None,
        });
    }
    if kind.is_dir() && to.starts_with(&from) {
        return Err(error("A folder cannot be moved into itself."));
    }
    let from_parent = from
        .parent()
        .ok_or_else(|| error("The filesystem root cannot be moved."))?;
    let mut warning = None;
    if kind.is_dir() {
        let before = manifest(&from, active_path.as_deref())?;
        let mut directories = before.directories.clone();
        directories.insert(from_parent.to_owned());
        directories.insert(to_parent.clone());
        let held = locks(directories)?;
        destination_clear(&to, false)?;
        if manifest(&from, active_path.as_deref())? != before {
            return Err(error(
                "This folder changed while preparing the move. Refresh and try again.",
            ));
        }
        let mut snapshots = Vec::new();
        let mut source_bytes = 0;
        for path in &before.documents {
            let snapshot = collaboration_store::snapshot_for_move(path)?;
            source_bytes += snapshot.source.len();
            if source_bytes > MAX_MOVE_SOURCE_BYTES {
                return Err(error(
                    "This folder contains over 128 MiB of Markdown. Move a smaller folder.",
                ));
            }
            if active_path.as_deref() == Some(path.as_path()) {
                check_active(
                    active.expect("active path came from this snapshot"),
                    &snapshot,
                )?;
            }
            let relative = path
                .strip_prefix(&from)
                .expect("manifest stays under its root");
            validate_folder_references(&snapshot.source, path, &to.join(relative), &from, &to)?;
            if active_path.as_deref() == Some(path.as_path()) {
                let active = active.expect("checked active snapshot");
                validate_folder_references(&active.source, path, &to.join(relative), &from, &to)?;
                if !active.history.complete {
                    return Err(error(
                        "Save and reopen this document before moving its folder; local reference history is incomplete.",
                    ));
                }
                for destination in active
                    .history
                    .references
                    .iter()
                    .chain(&active.history.links)
                {
                    validate_folder_destination(
                        destination,
                        path,
                        &to.join(relative),
                        &from,
                        &to,
                        true,
                    )?;
                }
            }
            snapshots.push(snapshot);
        }
        // The rename moves relative assets and checkpoint siblings together.
        // All cooperating writes are excluded until the new namespace is ready.
        // As with ordinary saves, advisory locks cannot constrain external tools.
        absent(&to)?;
        // Windows cannot rename a directory while handles inside it are open.
        // Release the moved tree's locks and checkpoint guards, keeping both
        // parents locked. A cooperating writer that opens a handle inside the
        // tree in this interval makes the rename fail with nothing moved; one
        // that starts after it fails at the vanished path.
        #[cfg(windows)]
        let held: Vec<_> = {
            snapshots.clear();
            held.into_iter()
                .filter(|(directory, _)| !directory.starts_with(&from))
                .collect()
        };
        fs::rename(&from, &to).map_err(|e| {
            error(format!(
                "Cannot move this folder (moves must stay on one filesystem): {e}"
            ))
        })?;
        if let Err(error) = sync_directory(from_parent).and_then(|()| sync_directory(&to_parent)) {
            warning = Some(format!(
                "The folder was moved, but the directory change could not be synced to disk: {error}"
            ));
        }
        drop(snapshots);
        drop(held);
    } else {
        let _locks = locks(BTreeSet::from([from_parent.to_owned(), to_parent.clone()]))?;
        destination_clear(&to, true)?;
        let snapshot = collaboration_store::snapshot_for_move(&from)?;
        let active_document = if active_path.as_deref() == Some(from.as_path()) {
            active
        } else {
            None
        };
        if let Some(active) = active_document {
            check_active(active, &snapshot)?;
        }
        validate_file_links(&snapshot.source, &from, &to)?;
        if let Some(active) = active_document {
            validate_file_links(&active.source, &from, &to)?;
            if from.parent() != to.parent() {
                if !active.history.complete {
                    return Err(error(
                        "Save and reopen this document before moving it to another folder; local reference history is incomplete.",
                    ));
                }
                for destination in &active.history.links {
                    validate_file_destination(destination)?;
                }
            }
            assets::relocate_sources(
                &[&snapshot.source, &active.source],
                Some(&from),
                &to,
                &active.history,
            )?;
        } else {
            assets::relocate(&snapshot.source, Some(&from), &to, &History::default())?;
        }
        publish_move(&from, &to, &snapshot)?;
    }
    Ok(MoveResult {
        from,
        to,
        active_path: remapped,
        warning,
    })
}

/// Whether `to` is another spelling of `from` on a case- or
/// normalization-insensitive filesystem, such as default APFS or NTFS.
///
/// Such a rename names the existing entry itself, so the no-clobber checks would
/// otherwise reject it as an occupied destination. The requested spelling must
/// not exist as its own directory entry: two hard links or two distinct files are
/// genuine conflicts.
fn names_same_entry(from: &Path, to: &Path) -> Result<bool, FileError> {
    let (Some(parent), Some(requested)) = (to.parent(), to.file_name()) else {
        return Ok(false);
    };
    if from.parent() != Some(parent) || from.file_name() == Some(requested) {
        return Ok(false);
    }
    let target = match fs::symlink_metadata(to) {
        Ok(target) => target,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    for entry in fs::read_dir(parent)? {
        if entry?.file_name() == requested {
            return Ok(false);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let source = fs::symlink_metadata(from)?;
        Ok(source.dev() == target.dev() && source.ino() == target.ino())
    }
    // Stable Rust exposes no Windows file identity. Without an exact entry, a
    // requested name that resolves and differs only by case reaches `from`.
    #[cfg(not(unix))]
    {
        let _ = target;
        let fold = |path: &Path| path.file_name().map(|n| n.to_string_lossy().to_lowercase());
        Ok(fold(from) == fold(to))
    }
}

/// Rename an entry to another spelling of its own name through an unused
/// visible sibling, using the ordinary checked move for each step.
fn rename_through_staging(
    from: &Path,
    to: &Path,
    active: Option<&ActiveDocument>,
) -> Result<MoveResult, FileError> {
    let parent = from.parent().expect("a named entry has a parent");
    let stem = from
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| error("Choose a valid destination name."))?;
    let tag = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let staging = parent.join(match from.extension().and_then(|e| e.to_str()) {
        Some(extension) if from.is_file() => format!("{stem} (renaming {tag}).{extension}"),
        _ => format!("{stem} (renaming {tag})"),
    });
    let first = move_item_blocking(from, &staging, active)?;
    let staged = active.map(|active| ActiveDocument {
        path: first
            .active_path
            .clone()
            .unwrap_or_else(|| active.path.clone()),
        ..active.clone()
    });
    match move_item_blocking(&staging, to, staged.as_ref()) {
        Ok(second) => Ok(MoveResult {
            from: first.from,
            to: second.to,
            active_path: second.active_path,
            warning: second.warning.or(first.warning),
        }),
        Err(failure) => {
            // Restore the original spelling. If that also fails, report where
            // the entry now is rather than leaving the writer to search for it.
            if let Err(restore) = move_item_blocking(&staging, from, staged.as_ref()) {
                return Err(error(format!(
                    "{failure} The item remains at \"{}\": {restore}",
                    staging.display()
                )));
            }
            Err(failure)
        }
    }
}

fn check_active(active: &ActiveDocument, snapshot: &MoveSnapshot) -> Result<(), FileError> {
    if active.saved_source != snapshot.source {
        return Err(FileError::Conflict);
    }
    if active.saved_version != snapshot.version {
        return Err(FileError::MetadataConflict);
    }
    Ok(())
}

fn hash_file(path: &Path, limit: u64) -> Result<blake3::Hash, FileError> {
    let mut file = File::open(path)?.take(limit + 1);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0; 16_384];
    let mut read = 0;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > limit {
            return Err(FileError::TooLarge);
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize())
}

fn publish_move(from: &Path, to: &Path, snapshot: &MoveSnapshot) -> Result<(), FileError> {
    let targets = collaboration_store::move_metadata_paths(to)?;
    let mut family = vec![(from.to_owned(), to.to_owned())];
    if let Some(checkpoint) = &snapshot.checkpoint {
        family.push((checkpoint.clone(), targets[0].clone()));
    }
    if let Some(lock) = &snapshot.lock {
        family.push((lock.clone(), targets[2].clone()));
    }
    let mut published = Vec::new();
    let result = (|| {
        for (old, new) in &family {
            // Hard-link publication is atomic and never replaces an existing
            // path. Cross-volume moves fail without unlinking the source.
            fs::hard_link(old, new)?;
            published.push(new.clone());
        }
        let source_hash = blake3::hash(snapshot.source.as_bytes());
        if hash_file(from, MAX_FILE_BYTES)? != source_hash
            || hash_file(to, MAX_FILE_BYTES)? != source_hash
        {
            return Err(FileError::Conflict);
        }
        if let Some(expected) = snapshot.checkpoint_hash {
            let limit = MAX_STATE_BYTES as u64 + 64;
            if hash_file(
                snapshot
                    .checkpoint
                    .as_ref()
                    .expect("checkpoint hash has a path"),
                limit,
            )? != expected
                || hash_file(&targets[0], limit)? != expected
            {
                return Err(FileError::MetadataConflict);
            }
        }
        sync_directory(to.parent().expect("destination has a parent"))?;
        for (old, _) in &family {
            fs::remove_file(old)?;
        }
        sync_directory(from.parent().expect("source has a parent"))?;
        Ok(())
    })();
    if let Err(error) = result {
        // Restore any unlinked original using the complete destination first.
        // If a non-cooperating writer occupied an original name, keep both sets
        // rather than deleting either writer's bytes.
        let mut restored = true;
        for (old, new) in &family {
            if !old.try_exists()? && published.contains(new) && fs::hard_link(new, old).is_err() {
                restored = false;
            }
        }
        // An occupied source name is not proof that our original was restored:
        // a non-cooperating writer may have created different bytes there.
        restored &=
            hash_file(from, MAX_FILE_BYTES).ok() == Some(blake3::hash(snapshot.source.as_bytes()));
        if let (Some(path), Some(expected)) = (&snapshot.checkpoint, snapshot.checkpoint_hash) {
            restored &= hash_file(path, MAX_STATE_BYTES as u64 + 64).ok() == Some(expected);
        }
        if restored {
            for new in published.into_iter().rev() {
                let _ = fs::remove_file(new);
            }
        }
        return Err(error);
    }
    Ok(())
}

fn relative_destination(destination: &str) -> bool {
    if destination.is_empty() || destination.starts_with('#') || destination.contains(':') {
        return false;
    }
    let path = destination.split('#').next().unwrap_or(destination);
    percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .map_or(true, |decoded| !Path::new(decoded.as_ref()).is_absolute())
}

fn validate_file_destination(destination: &str) -> Result<(), FileError> {
    if relative_destination(destination) {
        return Err(error(
            "This document or its undo history has a relative link. Update the link and save and reopen the document before moving it to another folder.",
        ));
    }
    Ok(())
}

fn validate_file_links(source: &str, old: &Path, new: &Path) -> Result<(), FileError> {
    if old.parent() == new.parent() {
        return Ok(());
    }
    for event in Parser::new_ext(source, MARKDOWN_OPTIONS) {
        if let Event::Start(Tag::Link { dest_url, .. }) = event {
            validate_file_destination(&dest_url)?;
        }
    }
    Ok(())
}

fn validate_folder_references(
    source: &str,
    old: &Path,
    new: &Path,
    moved: &Path,
    destination_root: &Path,
) -> Result<(), FileError> {
    for event in Parser::new_ext(source, MARKDOWN_OPTIONS) {
        if let Event::Start(Tag::Image { dest_url, .. } | Tag::Link { dest_url, .. }) = event {
            validate_folder_destination(&dest_url, old, new, moved, destination_root, false)?;
        }
    }
    Ok(())
}

/// Resolve the future namespace against the tree that is still at its old
/// location. Check every component before `..`: a path that re-enters the old
/// folder and then leaves it again still breaks when that folder disappears.
fn future_reference(path: &Path, moved: &Path, destination_root: &Path) -> Option<PathBuf> {
    let mut current = PathBuf::new();
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        match component {
            Component::CurDir => continue,
            // A Windows prefix such as `\\?\C:` names a volume device rather
            // than a directory, so neither it nor the root is probed.
            Component::Prefix(_) | Component::RootDir => {
                current.push(component.as_os_str());
                continue;
            }
            Component::ParentDir => {
                current.pop();
            }
            component => current.push(component.as_os_str()),
        }
        let probe = if let Ok(relative) = current.strip_prefix(destination_root) {
            moved.join(relative)
        } else if current.starts_with(moved) {
            return None;
        } else {
            current.clone()
        };
        let metadata = fs::symlink_metadata(probe).ok()?;
        if metadata.file_type().is_symlink() || (components.peek().is_some() && !metadata.is_dir())
        {
            return None;
        }
    }
    if let Ok(relative) = current.strip_prefix(destination_root) {
        let actual = fs::canonicalize(moved.join(relative)).ok()?;
        actual
            .strip_prefix(moved)
            .ok()
            .map(|relative| destination_root.join(relative))
    } else {
        fs::canonicalize(current).ok()
    }
}

fn validate_folder_destination(
    destination: &str,
    old: &Path,
    new: &Path,
    moved: &Path,
    destination_root: &Path,
    historical: bool,
) -> Result<(), FileError> {
    if destination.is_empty() || destination.starts_with('#') {
        return Ok(());
    }
    if destination.contains(':') && !destination.starts_with("file://") {
        return Ok(());
    }
    let path_part = destination.split('#').next().unwrap_or(destination);
    let Some(old_path) = assets::local_path(path_part, old.parent())? else {
        return Ok(());
    };
    let actual = match fs::canonicalize(&old_path) {
        Ok(actual) => actual,
        Err(e) if historical || e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    // Decode before classifying: %2Fpath is just as absolute as /path.
    let relative = relative_destination(destination);
    let internal = actual.strip_prefix(moved).ok();
    if !relative {
        // An absolute path through the old folder also breaks when its final
        // target is outside it, e.g. /Drafts/../shared/image.png.
        if old_path.ancestors().any(|path| {
            fs::canonicalize(path)
                .ok()
                .is_some_and(|path| path.starts_with(moved))
        }) {
            return Err(error(
                "This folder contains an absolute reference to a file inside it. Use a relative reference before moving the folder.",
            ));
        }
        return Ok(());
    }
    for ancestor in old_path.ancestors() {
        if fs::symlink_metadata(ancestor)?.file_type().is_symlink() {
            return Err(error(
                "This reference traverses a symbolic link. Use a direct relative reference before moving the folder.",
            ));
        }
    }
    let expected = internal.map_or_else(|| actual.clone(), |path| destination_root.join(path));
    let Some(new_path) = assets::local_path(path_part, new.parent())? else {
        return Ok(());
    };
    let resolved = future_reference(&new_path, moved, destination_root);
    if resolved.as_ref() != Some(&expected) {
        return Err(error(
            "This folder has a relative reference that would change after moving it. Move that resource into the folder or use an absolute reference first.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    #[test]
    fn creation_never_overwrites_and_validates_visible_names() {
        let directory = tempfile::tempdir().unwrap();
        let path = create_document_blocking(directory.path(), "Essay").unwrap();
        assert_eq!(path.file_name().unwrap(), "Essay.md");
        fs::write(&path, "keep me").unwrap();
        assert!(create_document_blocking(directory.path(), "Essay.md").is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "keep me");
        for value in ["", ".", "..", "../outside", "a/b", "a\\b", ".hidden"] {
            assert!(create_folder_blocking(directory.path(), value).is_err());
        }
    }

    #[test]
    fn rename_preserves_checkpoint_identity_and_saved_baseline() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("Essay.md");
        let to = directory.path().join("Renamed.md");
        let mut document = Document::new("Saved 日本 🦀\r\n".into());
        let state = document.collaboration().state();
        collaboration_store::save(&from, document.source(), &state, None, None).unwrap();
        document.replace(0..5, "Unsaved").unwrap();
        let active = ActiveDocument {
            path: from.clone(),
            saved_source: "Saved 日本 🦀\r\n".into(),
            saved_version: Some(collaboration_store::version(&state)),
            source: document.source().into(),
            history: History::default(),
        };
        let result = move_item_blocking(&from, &to, Some(&active)).unwrap();
        assert_eq!(
            result.active_path.as_ref(),
            Some(&fs::canonicalize(&to).unwrap())
        );
        assert!(!from.exists());
        let (source, restored) = collaboration_store::load(&to).unwrap();
        assert_eq!(source, active.saved_source);
        let restored = restored.unwrap();
        assert_eq!(restored.document, state.document);
        assert_eq!(restored.update, state.update);
        assert_eq!(document.source(), active.source);
        assert!(create_document_blocking(directory.path(), "Essay.md").is_ok());
    }

    #[test]
    fn staged_rename_preserves_checkpoint_and_active_document() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("essay.md");
        let document = Document::new("Saved\n".into());
        let state = document.collaboration().state();
        collaboration_store::save(&from, document.source(), &state, None, None).unwrap();
        let active = ActiveDocument {
            path: from.clone(),
            saved_source: "Saved\n".into(),
            saved_version: Some(collaboration_store::version(&state)),
            source: "Saved and unsaved\n".into(),
            history: History::default(),
        };
        let from = fs::canonicalize(&from).unwrap();
        let to = fs::canonicalize(directory.path()).unwrap().join("Essay.md");
        let result = rename_through_staging(&from, &to, Some(&active)).unwrap();
        assert_eq!(result.from, from);
        assert_eq!(result.to, to);
        assert_eq!(result.active_path.as_ref(), Some(&to));
        let names: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| is_markdown(Path::new(name)))
            .collect();
        assert_eq!(names, ["Essay.md"]);
        let (source, restored) = collaboration_store::load(&to).unwrap();
        assert_eq!(source, "Saved\n");
        assert_eq!(restored.unwrap().document, state.document);
    }

    #[test]
    fn capitalization_only_rename_succeeds_on_every_filesystem() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("essay.md");
        fs::write(&from, "keep me").unwrap();
        let result = move_item_blocking(&from, &directory.path().join("Essay.md"), None).unwrap();
        assert_eq!(result.to.file_name().unwrap(), "Essay.md");
        let names: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| is_markdown(Path::new(name)))
            .collect();
        assert_eq!(names, ["Essay.md"]);
        assert_eq!(fs::read_to_string(&result.to).unwrap(), "keep me");
    }

    #[test]
    fn failed_staged_rename_restores_the_original_name() {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        let from = root.join("essay.md");
        let occupied = root.join("Other.md");
        fs::write(&from, "mine").unwrap();
        fs::write(&occupied, "theirs").unwrap();
        assert!(rename_through_staging(&from, &occupied, None).is_err());
        assert_eq!(fs::read_to_string(&from).unwrap(), "mine");
        assert_eq!(fs::read_to_string(&occupied).unwrap(), "theirs");
        let markdown = fs::read_dir(&root)
            .unwrap()
            .filter(|entry| is_markdown(&entry.as_ref().unwrap().path()))
            .count();
        assert_eq!(markdown, 2, "no staging name remains");
    }

    #[test]
    fn distinct_entries_are_never_treated_as_one_spelling() {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        let from = root.join("a.md");
        fs::write(&from, "a").unwrap();
        assert!(!names_same_entry(&from, &root.join("missing.md")).unwrap());
        assert!(!names_same_entry(&from, &from).unwrap());
        let linked = root.join("b.md");
        fs::hard_link(&from, &linked).unwrap();
        assert!(!names_same_entry(&from, &linked).unwrap());
        assert!(move_item_blocking(&from, &linked, None).is_err());
        assert_eq!(fs::read_to_string(&from).unwrap(), "a");
    }

    #[test]
    fn moving_plain_markdown_does_not_seed_or_require_a_checkpoint() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("plain.md");
        let to = directory.path().join("moved.md");
        fs::write(&from, "plain").unwrap();
        move_item_blocking(&from, &to, None).unwrap();
        let (source, state) = collaboration_store::load(&to).unwrap();
        assert_eq!(source, "plain");
        assert!(state.is_none());
    }

    #[test]
    fn conflicts_and_folder_cycles_preserve_both_locations() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("one.md");
        let to = directory.path().join("two.md");
        fs::write(&from, "one").unwrap();
        fs::write(&to, "two").unwrap();
        assert!(move_item_blocking(&from, &to, None).is_err());
        assert_eq!(fs::read_to_string(&from).unwrap(), "one");
        assert_eq!(fs::read_to_string(&to).unwrap(), "two");
        fs::create_dir(directory.path().join("Folder")).unwrap();
        fs::create_dir(directory.path().join("Folder/Child")).unwrap();
        assert!(
            move_item_blocking(
                &directory.path().join("Folder"),
                &directory.path().join("Folder/Child/Folder"),
                None
            )
            .is_err()
        );
    }

    #[test]
    fn stale_active_baseline_cannot_be_relocated() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("note.md");
        let to = directory.path().join("renamed.md");
        fs::write(&from, "external").unwrap();
        let active = ActiveDocument {
            path: from.clone(),
            saved_source: "old".into(),
            saved_version: None,
            source: "unsaved".into(),
            history: History::default(),
        };
        assert!(matches!(
            move_item_blocking(&from, &to, Some(&active)),
            Err(FileError::Conflict)
        ));
        assert!(!to.exists());
        assert_eq!(fs::read_to_string(&from).unwrap(), "external");
    }

    #[test]
    fn folder_move_keeps_internal_links_assets_and_checkpoint_names() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("Drafts");
        let to = directory.path().join("Finished");
        fs::create_dir(&from).unwrap();
        fs::create_dir(from.join("Empty")).unwrap();
        fs::create_dir(from.join("media")).unwrap();
        fs::write(from.join("media/figure.png"), "preserved literal bytes").unwrap();
        fs::write(from.join("other.md"), "other").unwrap();
        let document = Document::new("![Figure](media/figure.png)\n\n[Other](other.md)".into());
        let state = document.collaboration().state();
        collaboration_store::save(&from.join("note.md"), document.source(), &state, None, None)
            .unwrap();
        move_item_blocking(&from, &to, None).unwrap();
        assert!(to.join("Empty").is_dir());
        assert_eq!(
            fs::read_to_string(to.join("media/figure.png")).unwrap(),
            "preserved literal bytes"
        );
        assert_eq!(
            collaboration_store::load(&to.join("note.md"))
                .unwrap()
                .1
                .unwrap()
                .document,
            state.document
        );
    }

    #[test]
    fn file_move_keeps_saved_current_and_undo_assets_without_saving_edits() {
        use std::io::Cursor;

        fn png(value: u8) -> Vec<u8> {
            let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([value, 12, 44, 255]));
            let mut output = Cursor::new(Vec::new());
            image
                .write_to(&mut output, image::ImageFormat::Png)
                .unwrap();
            output.into_inner()
        }

        let directory = tempfile::tempdir().unwrap();
        let old = directory.path().join("Drafts");
        let new = directory.path().join("Finished");
        fs::create_dir(&old).unwrap();
        fs::create_dir(&new).unwrap();
        let from = old.join("Essay.md");
        let to = new.join("Essay renamed.md");
        let first = assets::import(&from, assets::Input::Png(png(10))).unwrap();
        let second = assets::import(&from, assets::Input::Png(png(20))).unwrap();
        let third = assets::import(&from, assets::Input::Png(png(30))).unwrap();
        let saved_source = format!("![Saved](<{first}>)\r\n");
        let document = Document::new(saved_source.clone());
        let state = document.collaboration().state();
        collaboration_store::save(&from, &saved_source, &state, None, None).unwrap();
        let active = ActiveDocument {
            path: from.clone(),
            saved_source: saved_source.clone(),
            saved_version: Some(collaboration_store::version(&state)),
            source: format!("![Unsaved](<{second}>)"),
            history: History {
                references: vec![third.clone()],
                ..History::default()
            },
        };
        move_item_blocking(&from, &to, Some(&active)).unwrap();
        assert_eq!(fs::read_to_string(&to).unwrap(), saved_source);
        for (url, bytes) in [(first, png(10)), (second, png(20)), (third, png(30))] {
            let path = assets::local_path(&url, Some(&new)).unwrap().unwrap();
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        assert!(old.join("Essay.assets").is_dir());
        assert_eq!(
            collaboration_store::load(&to).unwrap().1.unwrap().update,
            state.update
        );
    }

    #[test]
    fn references_that_would_change_meaning_block_relocation() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("note.md");
        let target = directory.path().join("Other");
        fs::create_dir(&target).unwrap();
        fs::write(&from, "[Related](related.md)").unwrap();
        assert!(move_item_blocking(&from, &target.join("note.md"), None).is_err());
        assert!(from.exists());
        assert!(!target.join("note.md").exists());
    }

    #[test]
    fn removed_historical_hyperlinks_block_cross_directory_file_moves() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("note.md");
        let target = directory.path().join("Other");
        fs::create_dir(&target).unwrap();
        fs::write(&from, "plain").unwrap();
        let mut history = crate::io::image_store::ImageStore::default();
        let linked = crate::document::projection::Projection::new("plain [Other](other.md)");
        history.record_links(&linked.rich.links);
        history.record_links(&[]);
        let active = ActiveDocument {
            path: from.clone(),
            saved_source: "plain".into(),
            saved_version: None,
            source: "plain".into(),
            history: history.history(),
        };
        assert!(move_item_blocking(&from, &target.join("note.md"), Some(&active)).is_err());
        assert_eq!(fs::read_to_string(&from).unwrap(), "plain");
        assert!(!target.join("note.md").exists());
        // Same-directory rename leaves every retained relative base intact.
        move_item_blocking(&from, &directory.path().join("renamed.md"), Some(&active)).unwrap();
    }

    #[test]
    fn folder_rename_rejects_escape_reentry_and_encoded_absolute_references() {
        for image in [false, true] {
            for encoded_absolute in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let from = directory.path().join("Drafts");
                let to = directory.path().join("Finished");
                fs::create_dir(&from).unwrap();
                let asset = if image { "figure.png" } else { "other.md" };
                fs::write(from.join(asset), "preserved resource").unwrap();
                let destination = if encoded_absolute {
                    percent_encoding::utf8_percent_encode(
                        &fs::canonicalize(from.join(asset))
                            .unwrap()
                            .to_string_lossy(),
                        percent_encoding::NON_ALPHANUMERIC,
                    )
                    .to_string()
                } else {
                    format!("../Drafts/{asset}")
                };
                let prefix = if image { "!" } else { "" };
                let source = format!("{prefix}[Resource]({destination})");
                fs::write(from.join("note.md"), &source).unwrap();
                assert!(move_item_blocking(&from, &to, None).is_err(), "{source}");
                assert!(!to.exists());
                assert_eq!(fs::read_to_string(from.join("note.md")).unwrap(), source);
            }
        }
    }

    #[test]
    fn folder_rename_preserves_external_relative_references_in_future_namespace() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("Drafts");
        let to = directory.path().join("Finished");
        fs::create_dir(&from).unwrap();
        fs::create_dir(from.join("nested")).unwrap();
        fs::create_dir(directory.path().join("shared")).unwrap();
        fs::write(directory.path().join("shared/file.png"), "unchanged image").unwrap();
        let source = "![Image](../shared/file.png)\n\n[Link](../shared/file.png)";
        fs::write(from.join("note.md"), source).unwrap();
        // Normalizing a path that leaves and re-enters the moved root is safe
        // when it resolves through the *future* name rather than the old name.
        fs::write(
            from.join("nested/note.md"),
            "[Inside](../nested/../note.md)",
        )
        .unwrap();
        move_item_blocking(&from, &to, None).unwrap();
        assert_eq!(fs::read_to_string(to.join("note.md")).unwrap(), source);
        assert_eq!(
            fs::read_to_string(directory.path().join("shared/file.png")).unwrap(),
            "unchanged image"
        );
    }

    #[test]
    fn folder_rename_rejects_paths_that_traverse_the_vanished_old_name() {
        // Windows drive paths contain `:` and are addressable only as file URLs,
        // whose dot segments URL parsing removes before any path is resolved.
        let cases: &[bool] = if cfg!(windows) {
            &[false]
        } else {
            &[false, true]
        };
        for &absolute in cases {
            let directory = tempfile::tempdir().unwrap();
            let from = directory.path().join("Drafts");
            let to = directory.path().join("Finished");
            fs::create_dir(&from).unwrap();
            fs::create_dir(directory.path().join("shared")).unwrap();
            fs::write(directory.path().join("shared/file.png"), "external").unwrap();
            let destination = if absolute {
                format!(
                    "{}/../shared/file.png",
                    fs::canonicalize(&from).unwrap().display()
                )
            } else {
                "../Drafts/../shared/file.png".into()
            };
            fs::write(from.join("note.md"), format!("![Image](<{destination}>)")).unwrap();
            assert!(move_item_blocking(&from, &to, None).is_err());
            assert!(!to.exists());
        }
    }

    #[test]
    fn folder_rename_checks_removed_historical_hyperlinks_against_future_namespace() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("Drafts");
        let to = directory.path().join("Finished");
        fs::create_dir(&from).unwrap();
        fs::write(from.join("other.md"), "other").unwrap();
        fs::write(from.join("note.md"), "plain").unwrap();
        let active = ActiveDocument {
            path: from.join("note.md"),
            saved_source: "plain".into(),
            saved_version: None,
            source: "plain".into(),
            history: History {
                links: vec!["../Drafts/other.md".into()],
                ..History::default()
            },
        };
        assert!(move_item_blocking(&from, &to, Some(&active)).is_err());
        assert!(from.join("note.md").exists());
        assert!(!to.exists());
    }

    #[test]
    fn missing_active_file_does_not_block_an_unrelated_move() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("one.md");
        let to = directory.path().join("two.md");
        fs::write(&from, "one").unwrap();
        let active = ActiveDocument {
            path: directory.path().join("missing.md"),
            saved_source: "old".into(),
            saved_version: None,
            source: "unsaved".into(),
            history: History::default(),
        };
        let result = move_item_blocking(&from, &to, Some(&active)).unwrap();
        assert!(result.active_path.is_none());
    }
}
