//! Previewing an image of the library, and inserting it into the document by
//! reference to where it already lives.

use std::path::{Path, PathBuf};

use iced::widget::{column, container, image, row, space, text};
use iced::{ContentFit, Element, Fill, Task};

use super::{App, EDITOR, EditorMode, Message as AppMessage, Overlay, folder};
use crate::document::selection::Selection;
use crate::io::image_store::{self, LoadedImage};
use crate::theme::styles;

#[derive(Debug, Clone)]
pub enum Message {
    Show(PathBuf),
    Loaded {
        request: u64,
        result: Result<LoadedImage, String>,
    },
    Insert(PathBuf),
}

#[derive(Default)]
pub(super) struct State {
    path: Option<PathBuf>,
    /// `None` while the image is being read and decoded on a worker.
    image: Option<Result<LoadedImage, String>>,
    request: u64,
}

impl State {
    /// Drops the decoded image; a late load for it is then ignored.
    pub(super) fn close(&mut self) {
        *self = Self {
            request: self.request.wrapping_add(1),
            ..Self::default()
        };
    }
}

/// The Markdown destination that reaches `target` from a document in `base`.
///
/// `None` when no relative path exists (another drive), or when the path holds
/// a character the angle-bracket destination form cannot carry.
fn relative_destination(base: &Path, target: &Path) -> Option<String> {
    let base: Vec<_> = base.components().collect();
    let target: Vec<_> = target.components().collect();
    let shared = base
        .iter()
        .zip(&target)
        .take_while(|(base, target)| base == target)
        .count();
    if shared == 0 {
        return None;
    }
    let up = std::iter::repeat_n("..".to_owned(), base.len() - shared);
    let down = target[shared..]
        .iter()
        .map(|part| part.as_os_str().to_string_lossy().into_owned());
    let destination = up.chain(down).collect::<Vec<_>>().join("/");
    (!destination.is_empty() && !destination.contains(['<', '>', '\n', '\r']))
        .then_some(destination)
}

impl App {
    pub(super) fn update_image_preview(&mut self, message: Message) -> Task<AppMessage> {
        match message {
            Message::Show(path) => {
                if self.busy || self.loading {
                    return Task::none();
                }
                self.image_preview.close();
                let request = self.image_preview.request;
                self.image_preview.path = Some(path.clone());
                self.overlay = Some(Overlay::Image);
                Task::perform(image_store::load_path(path), move |result| {
                    AppMessage::ImagePreview(Message::Loaded { request, result })
                })
            }
            Message::Loaded { request, result } => {
                if request == self.image_preview.request {
                    self.image_preview.image = Some(result);
                }
                Task::none()
            }
            Message::Insert(path) => {
                if self.overlay == Some(Overlay::Image) {
                    self.overlay = None;
                }
                self.image_preview.close();
                self.insert_image_reference(&path);
                iced::widget::operation::focus(EDITOR)
            }
        }
    }

    /// Inserts a reference to `image` at the selection. The image stays where
    /// it is; nothing is copied.
    pub(super) fn insert_image_reference(&mut self, image: &Path) {
        if self.busy || self.loading {
            return;
        }
        if self.mode == EditorMode::Read {
            self.notification = Some("Switch to Write or Source to insert an image.".into());
            return;
        }
        // A draft is read from the folder its first save will write it into.
        let base = self
            .session
            .path()
            .map(|value| value.as_path())
            .and_then(Path::parent)
            .or(self.session.folder.as_deref());
        let Some(base) = base else {
            self.notification =
                Some("Save this document first, so the image can be found from it.".into());
            return;
        };
        let Some(destination) = relative_destination(base, image) else {
            self.notification = Some(format!(
                "{} cannot be referenced from this document.",
                folder::name(image)
            ));
            return;
        };
        let alt = image
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let alt: String = alt
            .chars()
            .filter(|character| !matches!(character, '[' | ']' | '\\'))
            .collect();
        let markdown = format!("![{alt}](<{destination}>)");
        let selection = self.session.selection;
        let start = selection.anchor.min(selection.head);
        let end = selection.anchor.max(selection.head);
        let after = Selection::caret(start + markdown.len());
        self.session.document.break_history_group();
        self.commit_local(start..end, &markdown, selection, after);
        if self.edit_error.is_none() {
            self.session.selection = after;
        }
        self.changed();
        self.sync_editor();
        self.session.document.break_history_group();
    }

    pub(super) fn image_preview_view(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let Some(path) = &self.image_preview.path else {
            return space().into();
        };
        let picture: Element<'_, AppMessage> = match &self.image_preview.image {
            Some(Ok(loaded)) => image(loaded.handle.clone())
                .content_fit(ContentFit::ScaleDown)
                .width(Fill)
                .height(Fill)
                .into(),
            Some(Err(error)) => text(error).size(13).color(tokens.muted).into(),
            None => text("Opening…").size(13).color(tokens.muted).into(),
        };
        let location = path
            .parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_default();
        container(
            column![
                column![
                    text(folder::name(path)).font(styles::BODY_FONT).size(22),
                    text(location).size(12).color(tokens.muted),
                ]
                .spacing(4),
                container(picture).center(Fill).height(Fill),
                row![
                    space::horizontal(),
                    self.small_button("Close", AppMessage::CloseOverlay, false),
                    self.small_button(
                        "Insert into document",
                        AppMessage::ImagePreview(Message::Insert(path.clone())),
                        false
                    ),
                ]
                .spacing(8),
            ]
            .spacing(16),
        )
        .width(640)
        .height(560)
        .padding(28)
        .style(styles::card(tokens))
        .into()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::{App, EditorMode, Message as AppMessage, Overlay};
    use super::{Message, relative_destination};

    #[test]
    fn destinations_are_relative_to_the_document_and_refuse_what_markdown_cannot_carry() {
        let destination =
            |base: &str, target: &str| relative_destination(Path::new(base), Path::new(target));
        assert_eq!(
            destination("/blog/content/posts", "/blog/content/posts/cover image.png").as_deref(),
            Some("cover image.png")
        );
        assert_eq!(
            destination("/blog/content/posts", "/blog/static/img/海.webp").as_deref(),
            Some("../../static/img/海.webp")
        );
        assert_eq!(
            destination("/book", "/book/figures/1/map.png").as_deref(),
            Some("figures/1/map.png")
        );
        assert_eq!(destination("/book", "/book/a<b>.png"), None);
        assert_eq!(destination("relative", "/book/a.png"), None);
    }

    #[test]
    fn inserting_references_the_image_in_place_as_one_undoable_edit() {
        let (mut app, _task) = App::new();
        let image = Path::new("/book/figures/The [old] map.png");
        app.insert_image_reference(image);
        assert!(app.session.document.source().is_empty());
        assert!(
            app.notification.as_deref().unwrap().contains("Save"),
            "a page that belongs nowhere has nothing to be relative to"
        );

        app.set_document(
            "Before after".into(),
            Some("/book/chapters/one.md".into()),
            "one.md".into(),
        );
        app.session.selection = crate::document::selection::Selection {
            anchor: 12,
            head: 7,
        };
        app.sync_editor();
        app.mode = EditorMode::Read;
        app.insert_image_reference(image);
        assert_eq!(app.session.document.source(), "Before after");
        app.mode = EditorMode::Live;

        drop(app.update(AppMessage::ImagePreview(Message::Show(image.into()))));
        assert_eq!(app.overlay, Some(Overlay::Image));
        drop(app.update(AppMessage::ImagePreview(Message::Insert(image.into()))));
        assert!(app.overlay.is_none());
        let inserted = "![The old map](<../figures/The [old] map.png>)";
        assert_eq!(app.session.document.source(), format!("Before {inserted}"));
        assert_eq!(app.session.selection.head, "Before ".len() + inserted.len());
        assert!(app.session.is_dirty());
        drop(app.update(AppMessage::Undo));
        assert_eq!(app.session.document.source(), "Before after");

        // A draft is relative to the folder its first save will use.
        drop(app.update(AppMessage::New));
        app.insert_image_reference(Path::new("/book/chapters/img/a.png"));
        assert_eq!(app.session.document.source(), "![a](<img/a.png>)");
    }
}
