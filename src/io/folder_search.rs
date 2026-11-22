//! Bounded content search over a workspace's known Markdown paths.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::storage::{self, FileError, LoadedFile, MAX_FILE_BYTES};

pub(crate) const MAX_FILES: usize = 4_096;
const MAX_SEARCH_BYTES: u64 = 128 * 1024 * 1024;
const MAX_MATCHES: usize = 100;
const EXCERPT_BEFORE: usize = 24;
const EXCERPT_AFTER: usize = 64;

#[derive(Debug, Clone)]
pub struct Target {
    pub path: PathBuf,
    range: Range<usize>,
    hash: blake3::Hash,
}

impl Target {
    pub(crate) fn range_in(&self, source: &str) -> Option<Range<usize>> {
        (self.hash == blake3::hash(source.as_bytes())).then(|| self.range.clone())
    }

    pub(crate) async fn open(self, root: PathBuf) -> Result<(LoadedFile, Range<usize>), FileError> {
        tokio::task::spawn_blocking(move || {
            if !known_file(&root, &self.path) {
                return Err(changed());
            }
            let file = storage::load_blocking(self.path.clone())?;
            let range = self.range_in(&file.source).ok_or_else(changed)?;
            Ok((file, range))
        })
        .await
        .map_err(|error| FileError::Io(error.to_string()))?
    }
}

pub(crate) fn changed() -> FileError {
    FileError::Io("This file changed since the search. Use Find in Folder again.".into())
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub target: Target,
    pub line: usize,
    pub excerpt: String,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub hits: Vec<Hit>,
    pub limited: bool,
    pub skipped: usize,
}

pub(crate) struct Request {
    pub root: PathBuf,
    pub paths: Vec<PathBuf>,
    /// A path with no snapshot was open but outside the memory budget.
    pub open_sources: HashMap<PathBuf, Option<String>>,
    pub query: String,
    pub incomplete_snapshot: bool,
}

pub(crate) async fn search(
    request: Request,
    cancelled: Arc<AtomicBool>,
) -> Result<Report, FileError> {
    tokio::task::spawn_blocking(move || search_blocking(request, &cancelled))
        .await
        .map_err(|error| FileError::Io(error.to_string()))
}

fn search_blocking(request: Request, cancelled: &AtomicBool) -> Report {
    let mut report = Report {
        limited: request.incomplete_snapshot || request.paths.len() > MAX_FILES,
        ..Report::default()
    };
    if request.query.is_empty() {
        return report;
    }

    let mut remaining = MAX_SEARCH_BYTES;
    for path in request.paths.into_iter().take(MAX_FILES) {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        if remaining == 0 {
            report.limited = true;
            break;
        }
        let read_limit = remaining.min(MAX_FILE_BYTES);
        let disk_source;
        let source = if let Some(snapshot) = request.open_sources.get(&path) {
            let Some(source) = snapshot
                .as_ref()
                .filter(|source| source.len() as u64 <= read_limit)
            else {
                report.limited = true;
                continue;
            };
            remaining -= source.len() as u64;
            source
        } else {
            // Workspace paths can be replaced while a search is running. Do
            // not follow an observed alias or read from a nonregular file.
            if !known_file(&request.root, &path) {
                report.skipped += 1;
                continue;
            }
            match read_source(&path, read_limit, &mut remaining) {
                Ok(Some(source)) => {
                    disk_source = source;
                    &disk_source
                }
                Ok(None) if read_limit < MAX_FILE_BYTES => {
                    report.limited = true;
                    break;
                }
                Ok(None) | Err(_) => {
                    report.skipped += 1;
                    continue;
                }
            }
        };
        if add_hits(&mut report.hits, &path, source, &request.query, cancelled) {
            report.limited = true;
            break;
        }
    }
    report
}

fn known_file(root: &Path, path: &Path) -> bool {
    super::folder::is_markdown(path)
        && path.starts_with(root)
        && fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
        && fs::canonicalize(path).is_ok_and(|resolved| resolved == path)
}

/// None signifies the per-file or total byte limit, including file growth
/// between the metadata check and the bounded read.
fn read_source(path: &Path, limit: u64, remaining: &mut u64) -> io::Result<Option<String>> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::other("Expected a regular Markdown file"));
    }
    if metadata.len() > limit {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    let read = file.take(limit + 1).read_to_end(&mut bytes);
    *remaining = remaining.saturating_sub(bytes.len() as u64);
    read?;
    if bytes.len() as u64 > limit {
        return Ok(None);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn add_hits(
    hits: &mut Vec<Hit>,
    path: &Path,
    source: &str,
    query: &str,
    cancelled: &AtomicBool,
) -> bool {
    let mut offset = 0;
    let mut hash = None;
    for (index, line) in source.split_inclusive('\n').enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            return false;
        }
        let text = line.trim_end_matches(['\r', '\n']);
        for (start, matched) in text.match_indices(query) {
            if hits.len() == MAX_MATCHES {
                return true;
            }
            let end = start + matched.len();
            let hash = *hash.get_or_insert_with(|| blake3::hash(source.as_bytes()));
            hits.push(Hit {
                target: Target {
                    path: path.to_owned(),
                    range: offset + start..offset + end,
                    hash,
                },
                line: index + 1,
                excerpt: excerpt(text, start..end),
            });
        }
        offset += line.len();
    }
    false
}

fn excerpt(line: &str, range: Range<usize>) -> String {
    let left = line[..range.start]
        .char_indices()
        .rev()
        .nth(EXCERPT_BEFORE - 1)
        .map_or(0, |(index, _)| index);
    let right = line[range.end..]
        .char_indices()
        .nth(EXCERPT_AFTER)
        .map_or(line.len(), |(offset, _)| range.end + offset);
    let prefix = if left > 0 { "…" } else { "" };
    let suffix = if right < line.len() { "…" } else { "" };
    format!("{prefix}{}{suffix}", &line[left..right])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_exact_utf8_ranges_in_mixed_line_endings_and_clips_long_excerpts() {
        let mut hits = Vec::new();
        let source = format!("wide 🦀\r\n{} café 🦀\n", "é".repeat(200));
        assert!(!add_hits(
            &mut hits,
            Path::new("Notes.md"),
            &source,
            "🦀",
            &AtomicBool::new(false)
        ));
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[1].line, 2);
        assert!(hits[1].excerpt.starts_with('…'));
        assert!(hits[1].excerpt.ends_with(" café 🦀"));
        for hit in hits {
            assert_eq!(&source[hit.target.range_in(&source).unwrap()], "🦀");
            assert!(hit.target.range_in(&format!(" {source}")).is_none());
        }
    }

    #[test]
    fn bounds_results_and_cancels_before_reading() {
        let mut hits = Vec::new();
        let source = "hit ".repeat(MAX_MATCHES + 1);
        assert!(add_hits(
            &mut hits,
            Path::new("Notes.md"),
            &source,
            "hit",
            &AtomicBool::new(false)
        ));
        assert_eq!(hits.len(), MAX_MATCHES);
        assert!(!add_hits(
            &mut Vec::new(),
            Path::new("Notes.md"),
            &source,
            "hit",
            &AtomicBool::new(true)
        ));
    }

    #[tokio::test]
    async fn searches_live_source_and_rejects_changed_disk_matches_on_open() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let active = root.join("Active.md");
        let other = root.join("Other.md");
        fs::write(&active, "saved text").unwrap();
        fs::write(&other, "match on disk").unwrap();
        let report = search(
            Request {
                root: root.clone(),
                paths: vec![active.clone(), other.clone()],
                open_sources: HashMap::from([(active, Some("match in unsaved text".into()))]),
                query: "match".into(),
                incomplete_snapshot: false,
            },
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
        assert_eq!(report.hits.len(), 2);
        assert_eq!(report.hits[0].excerpt, "match in unsaved text");
        let hit = report.hits[1].target.clone();
        let (file, range) = hit.clone().open(root.clone()).await.unwrap();
        assert_eq!(&file.source[range], "match");
        fs::write(&other, "match in a different revision").unwrap();
        assert!(hit.open(root).await.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn search_does_not_follow_replaced_parent_paths() {
        let workspace = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let root = workspace.path().canonicalize().unwrap();
        fs::write(other.path().join("Notes.md"), "secret").unwrap();
        std::os::unix::fs::symlink(other.path(), root.join("Child")).unwrap();
        let report = search_blocking(
            Request {
                paths: vec![root.join("Child/Notes.md")],
                root,
                open_sources: HashMap::new(),
                query: "secret".into(),
                incomplete_snapshot: false,
            },
            &AtomicBool::new(false),
        );
        assert!(report.hits.is_empty());
        assert_eq!(report.skipped, 1);
    }
}
