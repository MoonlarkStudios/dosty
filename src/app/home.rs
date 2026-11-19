//! The home surface: what a launch shows when there is nothing to resume.
//!
//! It sits on the pristine first page rather than replacing it. The editor
//! keeps focus, so typing starts a document and the surface is gone.

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Element, Fill};

use super::trail::Direction;
use super::{App, EditorMode, Message, folder, quick_open, workspace};
use crate::editor::file_tree;
use crate::theme::styles::{self, Button};

const MAX_RECENT: usize = 5;
/// Rows extend this far past the writing column on each side, so their labels
/// line up with the page's text while their hover surface has room around it.
const INSET: f32 = 10.0;

impl App {
    /// Whether the page is still the untouched one the window opened with, and
    /// nothing is on its way to replace it.
    pub(super) fn is_home(&self) -> bool {
        self.mode == EditorMode::Live
            && self.session.path.is_none()
            && !self.session.dirty
            && self.session.document.source().is_empty()
            && !self.trail.can_step(Direction::Back)
            && !self.trail.can_step(Direction::Forward)
            && !self.busy
            && !self.loading
            && self.workspace.settled()
    }

    pub(super) fn home_view(&self) -> Element<'_, Message> {
        let tokens = self.resolved_theme.tokens;
        let heading = |label: &'static str| {
            container(text(label).size(11).color(tokens.muted)).padding([0.0, INSET])
        };
        let choice = |title: String, location: String, message: Message| {
            button(
                row![
                    text(title).size(14).wrapping(text::Wrapping::None),
                    space::horizontal(),
                    text(location)
                        .size(12)
                        .color(tokens.muted)
                        .wrapping(text::Wrapping::None),
                ]
                .spacing(16)
                .align_y(Alignment::Center),
            )
            .padding([7.0, INSET])
            .width(Fill)
            .on_press(message)
            .style(styles::button_style(tokens, Button::Quiet))
        };
        // The empty page already invites writing; this only offers the way back.
        let mut content = column![].spacing(4).width(Fill);

        let recent: Vec<_> = self
            .workspace
            .recent_documents()
            .into_iter()
            .filter(|path| self.folder.has_file(path))
            .take(MAX_RECENT)
            .collect();
        if !recent.is_empty() {
            content = content.push(heading("Recent"));
        }
        for path in recent {
            let location = path.parent().map(folder::name).unwrap_or_default();
            content = content.push(choice(
                quick_open::title(&path),
                location,
                Message::Folder(folder::Message::Tree(file_tree::Action::Open(path))),
            ));
        }

        let places = self.workspace.places();
        if places.is_empty() {
            content = content.push(
                container(
                    text("Add a place to keep your writing within reach.")
                        .size(14)
                        .color(tokens.muted),
                )
                .padding([0.0, INSET]),
            );
        } else {
            content = content.push(space().height(18)).push(heading("Places"));
        }
        for (index, place) in places.iter().enumerate() {
            let open = self.folder.root() == Some(place.as_path());
            content = content.push(choice(
                workspace::place_label(place, places),
                if open { "Open".into() } else { String::new() },
                Message::Workspace(workspace::Message::OpenPlace(index)),
            ));
        }

        content = content.push(space().height(18)).push(
            row![
                self.small_button("Add place…", Message::Folder(folder::Message::Pick), false),
                self.small_button("Open file…", Message::Open, false),
                self.small_button(
                    "Quick Open",
                    Message::QuickOpen(quick_open::Message::Open),
                    false
                ),
            ]
            .spacing(8),
        );
        // Leave the first lines of the page, and its caret, clear.
        container(
            column![space().height(112), content]
                .max_width(self.resolved_theme.typography.content_width + 2.0 * INSET),
        )
        .padding(iced::Padding::new(32.0).top(56.0).bottom(16.0))
        .center_x(Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use iced::widget::text_editor::{Action, Edit};

    use super::super::{App, EditorMode, Message};
    use crate::io::storage::LoadedFile;

    fn launched() -> App {
        let (mut app, _task) = App::new();
        assert!(!app.is_home(), "nothing shows until the library is read");
        app.workspace.remember_for_preview(&[]);
        app
    }

    #[test]
    fn home_is_the_untouched_launch_page_and_ends_with_the_first_keystroke() {
        let mut app = launched();
        assert!(app.is_home());
        drop(app.update(Message::New));
        assert!(
            app.is_home(),
            "a new page over a pristine one is still home"
        );
        for mode in [EditorMode::Source, EditorMode::Read] {
            app.mode = mode;
            assert!(!app.is_home());
        }
        app.mode = EditorMode::Live;
        app.loading = true;
        assert!(!app.is_home(), "a document is on its way");
        app.loading = false;

        drop(app.update(Message::Edit(Action::Edit(Edit::Insert('a')))));
        assert!(!app.is_home());
        drop(app.update(Message::Undo));
        assert!(
            !app.is_home(),
            "writing that was undone is still a document"
        );
    }

    #[test]
    fn a_blank_page_after_visiting_a_document_is_not_home() {
        let mut app = launched();
        drop(app.update(Message::Loaded(Ok(LoadedFile {
            path: "/writing/note.md".into(),
            source: "Note".into(),
            collaboration: None,
        }))));
        assert!(!app.is_home());
        drop(app.update(Message::New));
        assert!(!app.is_home(), "the writer asked for a page, not for home");
    }
}
