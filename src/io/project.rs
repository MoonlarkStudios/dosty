//! Portable project metadata. Personal workspace state is stored separately.

use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{collaboration_store, file_ops, storage::FileError};

const DIRECTORY: &str = ".dosty";
const MANIFEST: &str = "project.toml";
const VERSION: u32 = 1;
const MAX_BYTES: u64 = 8 * 1024;
const STAGING_PREFIX: &str = ".dosty-project-";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Project {
    version: u32,
    pub name: Option<String>,
}

pub fn folder_name(value: &str) -> Result<String, FileError> {
    if value.contains([':', '*', '?', '"', '<', '>', '|']) || value.chars().any(char::is_control) {
        return Err(FileError::Io(
            "Use a name that works as a folder name.".into(),
        ));
    }
    file_ops::name(value, false)
}

pub fn valid_display_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 240 && !name.chars().any(char::is_control)
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>, FileError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn read(root: &Path) -> Result<Option<Project>, FileError> {
    let directory = root.join(DIRECTORY);
    match metadata(&directory)? {
        None => return Ok(None),
        Some(metadata) if !metadata.is_dir() => {
            return Err(FileError::Io(".dosty must be a real folder.".into()));
        }
        _ => {}
    }
    let path = directory.join(MANIFEST);
    let Some(metadata) = metadata(&path)? else {
        return Ok(None);
    };
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err(FileError::Io("Invalid .dosty/project.toml file.".into()));
    }
    let mut text = String::new();
    File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_BYTES {
        return Err(FileError::Io(".dosty/project.toml is too large.".into()));
    }
    let project: Project = toml::from_str(&text)
        .map_err(|error| FileError::Io(format!("Could not read .dosty/project.toml: {error}")))?;
    if project.version != VERSION {
        return Err(FileError::Io(
            "This project uses an unsupported metadata version.".into(),
        ));
    }
    if project
        .name
        .as_deref()
        .is_some_and(|name| !valid_display_name(name))
    {
        return Err(FileError::Io(
            "Invalid project display name in .dosty/project.toml.".into(),
        ));
    }
    Ok(Some(project))
}

fn root(path: &Path) -> Result<PathBuf, FileError> {
    let root = fs::canonicalize(path)?;
    if !root.is_dir() {
        return Err(FileError::Io("Choose a project folder.".into()));
    }
    Ok(root)
}

pub async fn inspect(path: PathBuf) -> Result<(PathBuf, bool), FileError> {
    tokio::task::spawn_blocking(move || {
        let root = root(&path)?;
        let exists = read(&root)?.is_some();
        Ok((root, exists))
    })
    .await
    .map_err(|error| FileError::Io(error.to_string()))?
}

/// Publish a complete manifest without ever replacing one that already exists.
fn initialize(root: &Path) -> Result<(), FileError> {
    if read(root)?.is_some() {
        return Ok(());
    }
    let directory = root.join(DIRECTORY);
    let created = match fs::create_dir(&directory) {
        Ok(()) => true,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => false,
        Err(error) => return Err(error.into()),
    };
    let result = (|| {
        if !fs::symlink_metadata(&directory)?.is_dir() {
            return Err(FileError::Io(".dosty must be a real folder.".into()));
        }
        let source = toml::to_string_pretty(&Project {
            version: VERSION,
            name: None,
        })
        .map_err(|error| FileError::Io(error.to_string()))?;
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        temporary.write_all(source.as_bytes())?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(directory.join(MANIFEST)) {
            Ok(_) => Ok(()),
            Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {
                read(root)?.ok_or_else(|| {
                    FileError::Io("The project metadata changed during setup.".into())
                })?;
                Ok(())
            }
            Err(error) => Err(error.error.into()),
        }
    })();
    if result.is_err() && created {
        let _ = fs::remove_dir(directory);
    }
    result
}

fn create_project(parent: &Path, name: &str) -> Result<PathBuf, FileError> {
    let name = folder_name(name)?;
    let parent = root(parent)?;
    let destination = parent.join(name);
    let mut builder = tempfile::Builder::new();
    builder.prefix(STAGING_PREFIX);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let staging = builder.tempdir_in(&parent)?;
    // The wrapper owns cleanup. Its child keeps the permissions of a normal
    // new folder and can be moved to the destination on the same filesystem.
    let staged_root = staging.path().join("project");
    fs::create_dir(&staged_root)?;
    initialize(&staged_root)?;
    file_ops::sync_directory(&staged_root.join(DIRECTORY))?;
    file_ops::sync_directory(&staged_root)?;

    let _lock = collaboration_store::directory_write_lock(&parent)?;
    renamore::rename_exclusive(&staged_root, &destination).map_err(|error| match error.kind() {
        ErrorKind::AlreadyExists => FileError::Io("That project name already exists.".into()),
        ErrorKind::Unsupported => FileError::Io(
            "This filesystem cannot publish a project atomically. Choose another location.".into(),
        ),
        _ => error.into(),
    })?;
    // The complete project is visible now. A failed flush cannot undo the
    // rename, so report it in the log and open the project that was created.
    if let Err(error) = file_ops::sync_directory(&parent) {
        tracing::warn!(path = %destination.display(), %error, "Could not sync published project location");
    }
    Ok(destination)
}

pub async fn setup(path: PathBuf, name: Option<String>) -> Result<PathBuf, FileError> {
    tokio::task::spawn_blocking(move || {
        if let Some(name) = name {
            create_project(&path, &name)
        } else {
            let root = root(&path)?;
            initialize(&root)?;
            Ok(root)
        }
    })
    .await
    .map_err(|error| FileError::Io(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_no_staging(parent: &Path) {
        assert!(fs::read_dir(parent).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(STAGING_PREFIX)
        }));
    }

    #[tokio::test]
    async fn setup_preserves_existing_documents_and_never_replaces_a_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let note = root.join("Note.md");
        fs::write(&note, "Markdown\n").unwrap();
        assert_eq!(inspect(root.clone()).await.unwrap(), (root.clone(), false));
        assert!(!root.join(DIRECTORY).exists());
        assert_eq!(setup(root.clone(), None).await.unwrap(), root);
        assert_eq!(fs::read_to_string(&note).unwrap(), "Markdown\n");
        let manifest = root.join(DIRECTORY).join(MANIFEST);
        fs::write(&manifest, "version = 1\nname = 'Writing desk'\n").unwrap();
        setup(root.clone(), None).await.unwrap();
        assert_eq!(
            read(&root).unwrap().unwrap().name.as_deref(),
            Some("Writing desk")
        );
        fs::write(&manifest, "version = 2\n").unwrap();
        assert!(setup(root.clone(), None).await.is_err());
        assert_eq!(fs::read_to_string(manifest).unwrap(), "version = 2\n");
    }

    #[tokio::test]
    async fn new_project_uses_a_new_folder_and_leaves_conflicting_names_alone() {
        let temp = tempfile::tempdir().unwrap();
        let parent = fs::canonicalize(temp.path()).unwrap();
        let root = setup(parent.clone(), Some("Field Guide".into()))
            .await
            .unwrap();
        assert_eq!(root, parent.join("Field Guide"));
        assert!(read(&root).unwrap().is_some());
        assert_no_staging(&parent);
        let other = parent.join("Existing");
        fs::create_dir(&other).unwrap();
        assert!(
            setup(parent.clone(), Some("Existing".into()))
                .await
                .is_err()
        );
        assert!(!other.join(DIRECTORY).exists());
        assert_no_staging(&parent);
        assert!(setup(parent, Some("../Outside".into())).await.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_failure_before_publication_leaves_the_destination_absent() {
        let parent = tempfile::tempdir().unwrap();
        // The lock is acquired after staging and before publication.
        std::os::unix::fs::symlink(
            parent.path().join("unused"),
            parent.path().join(".dosty-folder-lock"),
        )
        .unwrap();
        let error = create_project(parent.path(), "Field Guide").unwrap_err();
        assert!(error.to_string().contains("folder lock"));
        assert!(!parent.path().join("Field Guide").exists());
        assert_no_staging(parent.path());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn project_setup_does_not_write_through_metadata_symlinks() {
        let parent = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), parent.path().join(DIRECTORY)).unwrap();
        assert!(setup(parent.path().to_owned(), None).await.is_err());
        assert!(!outside.path().join(MANIFEST).exists());
    }
}
