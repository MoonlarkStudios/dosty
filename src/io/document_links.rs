//! Relative Markdown destinations and on-demand reads of workspace documents.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use pulldown_cmark::{Event, Parser, Tag};

use super::{assets, storage};
use crate::document::block_parser::MARKDOWN_OPTIONS;
use crate::document::writing::{HeadingIds, parse_heading};

const URL_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
const MAX_BACKLINK_FILES: usize = 2_048;
const MAX_BACKLINK_BYTES: u64 = 32 * 1024 * 1024;
const MAX_BACKLINKS: usize = 100;
const MAX_LINK_EVENTS: usize = 65_536;
pub(crate) const MAX_HEADINGS: usize = 2_048;

#[derive(Debug, Clone)]
pub struct Heading {
    pub title: String,
    pub fragment: String,
}

#[derive(Debug, Clone)]
pub struct Backlink {
    pub path: PathBuf,
    pub snippet: String,
    pub offset: usize,
    pub digest: blake3::Hash,
}

#[derive(Debug, Clone, Default)]
pub struct Backlinks {
    pub entries: Vec<Backlink>,
    pub partial: bool,
}

pub(crate) fn fragment(destination: &str) -> Option<String> {
    percent_decode_str(destination)
        .decode_utf8()
        .ok()
        .map(|decoded| decoded.into_owned())
}

pub(crate) fn with_heading(mut destination: String, fragment: &str) -> String {
    destination.push('#');
    destination.extend(utf8_percent_encode(fragment, URL_SEGMENT));
    destination
}

/// Resolve only the path portion. A file name containing a literal hash is
/// represented by %23, while an unescaped hash starts the heading fragment.
pub(crate) fn destination(value: &str, base: Option<&Path>) -> Option<(PathBuf, Option<String>)> {
    let value = value.trim();
    let (path, heading) = match value.split_once('#') {
        Some((path, heading)) if !heading.is_empty() => (path, Some(fragment(heading)?)),
        Some((path, _)) => (path, None),
        None => (value, None),
    };
    if path.is_empty() {
        return None;
    }
    let file = assets::local_path(path, base).ok()??;
    let extension = file.extension()?.to_str()?;
    ["md", "markdown", "txt"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        .then_some((file, heading))
}

/// Both paths come from the same folder workspace. Emit standard relative
/// paths, escaping individual names while keeping Markdown's slash separators.
pub(crate) fn relative(root: &Path, base: &Path, target: &Path) -> Option<String> {
    let parent = base.strip_prefix(root).ok()?;
    let target = target.strip_prefix(root).ok()?;
    let mut from = parent.components().peekable();
    let mut to = target.components().peekable();
    while from.peek().is_some() && from.peek() == to.peek() {
        from.next();
        to.next();
    }
    let mut segments = Vec::new();
    for component in from {
        if !matches!(component, Component::Normal(_)) {
            return None;
        }
        segments.push("..".to_owned());
    }
    for component in to {
        let Component::Normal(name) = component else {
            return None;
        };
        segments.push(utf8_percent_encode(name.to_str()?, URL_SEGMENT).to_string());
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

pub(crate) async fn resolve_path(path: PathBuf) -> Result<PathBuf, storage::FileError> {
    tokio::task::spawn_blocking(move || fs::canonicalize(path).map_err(Into::into))
        .await
        .map_err(|error| storage::FileError::Io(error.to_string()))?
}

pub(crate) async fn headings(
    root: PathBuf,
    path: PathBuf,
) -> Result<Vec<Heading>, storage::FileError> {
    tokio::task::spawn_blocking(move || {
        let source = read_workspace_file(&root, &path, storage::MAX_FILE_BYTES)?;
        if source.len() as u64 > storage::MAX_FILE_BYTES {
            return Err(storage::FileError::TooLarge);
        }
        let source = std::str::from_utf8(&source)
            .map_err(|error| storage::FileError::Io(error.to_string()))?;
        Ok(heading_choices(source))
    })
    .await
    .map_err(|error| storage::FileError::Io(error.to_string()))?
}

fn heading_choices(source: &str) -> Vec<Heading> {
    let mut depth = 0_usize;
    let mut ids = HeadingIds::default();
    let mut headings = Vec::new();
    for (event, range) in Parser::new_ext(source, MARKDOWN_OPTIONS).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0
                    && matches!(tag, Tag::Heading { .. })
                    && let Some((_, title, custom)) = parse_heading(&source[range])
                {
                    let fragment = ids.assign(&title, custom.as_deref());
                    if !fragment.is_empty() {
                        headings.push(Heading { title, fragment });
                        if headings.len() == MAX_HEADINGS {
                            break;
                        }
                    }
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    headings
}

pub(crate) async fn backlinks(
    root: PathBuf,
    paths: Vec<PathBuf>,
    target: PathBuf,
    cancelled: Arc<AtomicBool>,
) -> Result<Backlinks, storage::FileError> {
    tokio::task::spawn_blocking(move || scan_backlinks(&root, paths, &target, &cancelled))
        .await
        .map_err(|error| storage::FileError::Io(error.to_string()))?
}

fn scan_backlinks(
    root: &Path,
    paths: Vec<PathBuf>,
    target: &Path,
    cancelled: &AtomicBool,
) -> Result<Backlinks, storage::FileError> {
    let target = fs::canonicalize(target)?;
    if !target.starts_with(root) {
        return Err(storage::FileError::Io(
            "This document is outside the current workspace.".into(),
        ));
    }
    let mut result = Backlinks::default();
    let mut bytes = MAX_BACKLINK_BYTES;
    let mut links = MAX_LINK_EVENTS;
    let candidates = paths.into_iter().filter(|path| path != &target);
    for (index, path) in candidates.enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        if index == MAX_BACKLINK_FILES || bytes == 0 || result.entries.len() == MAX_BACKLINKS {
            result.partial = true;
            break;
        }
        let limit = storage::MAX_FILE_BYTES.min(bytes);
        let source = match read_workspace_file(root, &path, limit) {
            Ok(source) => source,
            Err(_) => {
                result.partial = true;
                continue;
            }
        };
        bytes = bytes.saturating_sub(source.len() as u64);
        if source.len() as u64 > limit {
            result.partial = true;
            continue;
        }
        let Ok(source) = std::str::from_utf8(&source) else {
            result.partial = true;
            continue;
        };
        for (event, range) in Parser::new_ext(source, MARKDOWN_OPTIONS).into_offset_iter() {
            let Event::Start(Tag::Link { dest_url, .. }) = event else {
                continue;
            };
            if links == 0 {
                result.partial = true;
                return Ok(result);
            }
            links -= 1;
            let Some((linked, _)) = destination(&dest_url, path.parent()) else {
                continue;
            };
            // The spelling can differ on a case-insensitive filesystem.
            // Confirm equality against the real path before admitting the link.
            if assets::lexically_normalized(&linked)
                .as_os_str()
                .eq_ignore_ascii_case(target.as_os_str())
                && fs::canonicalize(linked).is_ok_and(|path| path == target)
            {
                let line_start = source[..range.start].rfind('\n').map_or(0, |at| at + 1);
                let line = source[line_start..].lines().next().unwrap_or_default();
                let snippet_start = if source[line_start..range.start].chars().take(72).count() < 72
                {
                    line
                } else {
                    &line[range.start - line_start..]
                };
                result.entries.push(Backlink {
                    path,
                    snippet: snippet_start.trim().chars().take(150).collect(),
                    offset: range.start,
                    digest: blake3::hash(source.as_bytes()),
                });
                break;
            }
        }
    }
    Ok(result)
}

/// Reject files replaced since folder discovery, including symlinks and
/// non-regular entries, and leave the workspace's Markdown untouched.
fn read_workspace_file(
    root: &Path,
    path: &Path,
    limit: u64,
) -> Result<Vec<u8>, storage::FileError> {
    if !path.starts_with(root)
        || !fs::symlink_metadata(path)?.is_file()
        || fs::canonicalize(path)? != path
    {
        return Err(storage::FileError::Io(
            "This workspace file is no longer available.".into(),
        ));
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(storage::FileError::TooLarge);
    }
    let mut source = Vec::new();
    file.take(limit + 1).read_to_end(&mut source)?;
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_heading_fragments_escape_separately() {
        let root = Path::new("Workspace");
        let source = root.join("Drafts/Intro.md");
        let target = root.join("Reference/資料 #1 & 50%.MD");
        let encoded = relative(root, source.parent().unwrap(), &target).unwrap();
        let link = with_heading(encoded, "記録-and-log");
        assert_eq!(
            link,
            "../Reference/%E8%B3%87%E6%96%99%20%231%20%26%2050%25.MD#%E8%A8%98%E9%8C%B2-and-log"
        );
        let (path, heading) = destination(&link, source.parent()).unwrap();
        assert_eq!(assets::lexically_normalized(&path), target);
        assert_eq!(heading.as_deref(), Some("記録-and-log"));
        assert!(destination("https://example.com/draft.md#intro", source.parent()).is_none());
        assert!(destination("mail:local.md#intro", source.parent()).is_none());
    }

    #[test]
    fn headings_use_distinct_fragments_for_duplicates_and_custom_ids() {
        let options =
            heading_choices("# Intro\n\n> # Quoted\n\n# Other {#intro-1}\n\n## **Intro**\n");
        let ids: Vec<_> = options
            .iter()
            .map(|heading| heading.fragment.as_str())
            .collect();
        assert_eq!(ids, ["intro", "intro-1", "intro-2"]);
    }

    #[test]
    fn backlinks_respect_the_filesystems_filename_case_rules() {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        let draft = root.join("Draft.md");
        let alias = root.join("draft.md");
        let source = root.join("Notes.md");
        fs::write(&draft, "# Draft").unwrap();
        fs::write(&source, "[draft](draft.md)").unwrap();

        let result = scan_backlinks(&root, vec![source], &draft, &AtomicBool::new(false)).unwrap();
        let same_file = fs::canonicalize(alias).is_ok_and(|path| path == draft);
        assert_eq!(result.entries.len(), usize::from(same_file));
    }

    #[test]
    fn backlinks_find_real_reference_and_inline_links_with_source_positions() {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        let draft = root.join("Draft.md");
        let source = root.join("Refs/Notes.md");
        let hiddens = root.join("False.md");
        fs::create_dir(root.join("Refs")).unwrap();
        fs::write(&draft, "# Draft").unwrap();
        let note = "# Notes\n\nDiscussion 🦀 [draft][ref].\n\n[ref]: ../Draft.md#draft\n";
        fs::write(&source, note).unwrap();
        fs::write(&hiddens, "`[draft](Draft.md)` and ![draft](Draft.md)").unwrap();

        let result = scan_backlinks(
            &root,
            vec![hiddens, draft.clone(), source.clone()],
            &draft,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(!result.partial);
        assert_eq!(result.entries.len(), 1);
        let backlink = &result.entries[0];
        assert_eq!(backlink.path, source);
        assert_eq!(backlink.offset, note.find("[draft]").unwrap());
        assert_eq!(backlink.snippet, "Discussion 🦀 [draft][ref].");
        assert_eq!(backlink.digest, blake3::hash(note.as_bytes()));
    }
}
