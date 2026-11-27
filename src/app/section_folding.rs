//! Write mode's session-local heading folds. Markdown and its projection stay complete.

use std::ops::Range;
use std::sync::Arc;

use iced::Task;
use iced::widget::text_editor::{Action, Edit};

use super::{App, EditorMode, Message};
use crate::document::outline;
use crate::document::selection::Selection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub heading: Range<usize>,
    pub body: Range<usize>,
}

impl Target {
    fn hides(&self, selection: Selection, source_len: usize) -> bool {
        let selected = selection.range();
        self.body.contains(&selection.head)
            || self.body.end == source_len && selection.head == source_len
            || selected.start < self.body.end && self.body.start < selected.end
    }
}

#[derive(Default)]
pub(super) struct State {
    pub targets: Vec<Target>,
    pub hidden: Arc<[Target]>,
    revision: Option<u64>,
}

impl App {
    pub(super) fn sync_section_folds(&mut self) {
        if self.section_folds.revision != Some(self.session.document.revision()) {
            let source = self.session.document.source();
            let blocks = self.session.document.blocks();
            self.section_folds.targets = self
                .outline
                .sections
                .iter()
                .filter_map(|section| {
                    let heading = &blocks[section.block].range;
                    // A following parsed block inside the section distinguishes a
                    // real body from a heading followed by whitespace only.
                    let first_body = blocks.get(section.block + 1)?.range.start;
                    if first_body >= section.range.end {
                        return None;
                    }
                    let start = heading.end + source[heading.end..first_body].find('\n')? + 1;
                    Some(Target {
                        heading: section.range.start..heading.end,
                        body: start..section.range.end,
                    })
                })
                .collect();
            self.section_folds.revision = Some(self.session.document.revision());
            self.refresh_section_folds();
        } else if self.mode == EditorMode::Live
            && self.section_folds.hidden.iter().any(|target| {
                target.hides(self.session.selection, self.session.document.source().len())
            })
        {
            self.refresh_section_folds();
        }
    }

    fn refresh_section_folds(&mut self) {
        let targets = &self.section_folds.targets;
        let mut hidden = Vec::with_capacity(self.session.fold_anchors.len());
        let document = self.session.document.collaboration();
        let source_len = self.session.document.source().len();
        self.session.fold_anchors.retain(|anchor| {
            let selection = document.resolve_selection(anchor);
            let Some(start) = selection
                .filter(|selection| !selection.is_caret())
                .map(|s| s.range().start)
            else {
                return false;
            };
            let Ok(index) = targets.binary_search_by_key(&start, |target| target.heading.start)
            else {
                return false;
            };
            let target = &targets[index];
            if self.mode == EditorMode::Live && target.hides(self.session.selection, source_len) {
                return false;
            }
            hidden.push(target.clone());
            true
        });
        hidden.sort_unstable_by_key(|target| target.heading.start);
        // Nested folds stay recorded so they can reappear when their parent opens.
        let mut covered = 0;
        hidden.retain(|target| {
            if target.heading.start < covered {
                return false;
            }
            covered = target.body.end;
            true
        });
        if self.section_folds.hidden.as_ref() != hidden {
            self.section_folds.hidden = hidden.into();
        }
    }

    pub(super) fn current_section_fold(&self) -> Option<usize> {
        let index = outline::current(&self.outline.sections, self.session.selection.head)?;
        let start = self.outline.sections[index].range.start;
        self.section_folds
            .targets
            .binary_search_by_key(&start, |target| target.heading.start)
            .ok()
    }

    pub(super) fn toggle_section_fold(
        &mut self,
        clicked: Option<(usize, Arc<()>)>,
    ) -> Task<Message> {
        if self.mode != EditorMode::Live || self.loading || self.busy || self.overlay.is_some() {
            return Task::none();
        }
        let index = if let Some((start, revision)) = clicked {
            if !Arc::ptr_eq(&revision, &self.projection.source_id)
                || self
                    .section_folds
                    .hidden
                    .iter()
                    .any(|parent| parent.body.contains(&start))
            {
                return Task::none();
            }
            self.section_folds
                .targets
                .binary_search_by_key(&start, |target| target.heading.start)
                .ok()
        } else {
            self.current_section_fold()
        };
        let Some(target) = index.and_then(|index| self.section_folds.targets.get(index)) else {
            return Task::none();
        };
        let start = target.heading.start;
        if self
            .section_folds
            .hidden
            .binary_search_by_key(&start, |target| target.heading.start)
            .is_ok()
        {
            self.remove_section_fold(start);
        } else {
            if target.hides(self.session.selection, self.session.document.source().len()) {
                if !self.session.selection.is_caret() {
                    self.notification = Some("Clear the selection to fold this section.".into());
                    return Task::none();
                }
                self.session.selection = Selection::caret(target.heading.end);
                self.clear_grid();
                self.sync_editor();
            }
            let end = start
                + self.session.document.source()[start..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8);
            self.session
                .fold_anchors
                .push(self.session.document.collaboration().selection(Selection {
                    anchor: start,
                    head: end,
                }));
            self.refresh_section_folds();
        }
        Task::none()
    }

    pub(super) fn reveal_section_at_boundary(&mut self, action: &Action) -> bool {
        if self.mode != EditorMode::Live || !self.session.selection.is_caret() {
            return false;
        }
        let after = match action {
            Action::Edit(Edit::Delete) => false,
            Action::Edit(Edit::Backspace) => true,
            _ => return false,
        };
        let display = self.projection.to_display(self.session.selection.head);
        let Some(target) = self.section_folds.hidden.iter().find(|target| {
            let boundary = if after {
                target.body.end
            } else {
                target.heading.end
            };
            display == self.projection.to_display(boundary)
        }) else {
            return false;
        };
        self.remove_section_fold(target.heading.start);
        true
    }

    fn remove_section_fold(&mut self, start: usize) {
        let document = self.session.document.collaboration();
        self.session.fold_anchors.retain(|anchor| {
            document
                .resolve_selection(anchor)
                .is_some_and(|selection| selection.range().start != start)
        });
        self.refresh_section_folds();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writing(source: &str) -> App {
        let (mut app, _) = App::new();
        app.set_document(source.into(), None, "Fold.md".into());
        app.mode = EditorMode::Live;
        app.sync_editor();
        app
    }

    fn send(app: &mut App, message: Message) {
        drop(app.update_message(message));
    }

    fn click(app: &mut App, heading: usize) {
        send(
            app,
            Message::ToggleSectionFold(Some((heading, Arc::clone(&app.projection.source_id)))),
        );
    }

    #[test]
    fn folding_covers_immediate_bodies_including_crlf_and_setext() {
        for (source, title) in [
            ("# Title\nBody\n# End", "# Title"),
            ("# Title\r\nBody\r\n# End", "# Title"),
            ("Title\r\n=====\r\nBody\r\n# End", "Title\r\n====="),
            ("# Title\nBody", "# Title"),
        ] {
            let mut app = writing(source);
            assert_eq!(app.section_folds.targets.len(), 1);
            let body = source.find("Body").unwrap();
            send(&mut app, Message::SelectSource(Selection::caret(body + 1)));
            let shortcut = super::super::shortcut(
                &iced::keyboard::Key::Character("l".into()),
                iced::keyboard::Modifiers::COMMAND | iced::keyboard::Modifiers::ALT,
            )
            .expect("the fold command has a keyboard shortcut");
            send(&mut app, shortcut);
            assert_eq!(app.session.selection, Selection::caret(title.len()));
            let hidden = &app.section_folds.hidden[0];
            assert_eq!(
                hidden.body,
                body..source.find("# End").unwrap_or(source.len())
            );
            assert_eq!(app.session.document.source(), source);
            assert!(app.projection.text.contains("Body"));
            assert_eq!(app.editor.snapshot(), app.projection.text);
            assert!(!app.session.is_dirty() && !app.session.document.can_undo());
            send(&mut app, Message::ToggleSectionFold(None));
            assert!(app.section_folds.hidden.is_empty());
        }
        let app = writing("# One\n# Next\n  \n");
        assert!(app.section_folds.targets.is_empty());
    }

    #[test]
    fn nested_folds_reappear_and_find_reveals_the_body_at_the_match() {
        let source = "# Parent\n## Child\nKeep café here\n# Next\nTail";
        let child = source.find("## Child").unwrap();
        let text = source.find("café").unwrap();
        let mut app = writing(source);
        click(&mut app, child);
        click(&mut app, 0);
        assert_eq!(app.section_folds.hidden.len(), 1);
        assert_eq!(app.section_folds.hidden[0].heading.start, 0);
        click(&mut app, 0);
        assert_eq!(app.section_folds.hidden[0].heading.start, child);
        click(&mut app, 0);
        send(&mut app, Message::Query("café".into()));
        send(&mut app, Message::FindNext);
        assert_eq!(app.session.selection.range(), text..text + "café".len());
        assert!(app.section_folds.hidden.is_empty());
        assert_eq!(app.mode, EditorMode::Live);
        assert_eq!(app.session.document.source(), source);
        assert!(!app.session.document.can_undo());
    }

    #[test]
    fn selection_and_boundary_deletes_reveal_before_editing_folded_source() {
        let source = "# Title\nBody 🦀\n# Next\nTail";
        let mut app = writing(source);
        let boundary = source.find("# Next").unwrap();
        for (at, command) in [("# Title".len(), Edit::Delete), (boundary, Edit::Backspace)] {
            app.session.selection = Selection::caret(at);
            app.sync_editor();
            click(&mut app, 0);
            assert_eq!(app.section_folds.hidden.len(), 1);
            send(&mut app, Message::Edit(Action::Edit(command)));
            assert!(app.section_folds.hidden.is_empty());
            assert_eq!(app.session.selection, Selection::caret(at));
            assert_eq!(app.session.document.source(), source);
            assert!(!app.session.document.can_undo());
        }
        click(&mut app, 0);
        let reverse = Selection {
            anchor: boundary + 3,
            head: 2,
        };
        send(&mut app, Message::SelectSource(reverse));
        assert!(app.section_folds.hidden.is_empty());
        click(&mut app, 0);
        assert!(app.section_folds.hidden.is_empty());
        assert_eq!(app.session.selection, reverse);
        assert_eq!(app.session.document.source(), source);
    }

    #[test]
    fn background_documents_keep_their_write_folds() {
        let source = "# First\nFold this body\n# Next\nStay here";
        let mut app = writing(source);
        let id = app.session.id();
        send(
            &mut app,
            Message::SelectSource(Selection::caret(source.len())),
        );
        send(&mut app, Message::Edit(Action::Edit(Edit::Insert('!'))));
        click(&mut app, 0);
        assert_eq!(app.section_folds.hidden.len(), 1);
        send(&mut app, Message::New);
        assert_ne!(app.session.id(), id);
        assert!(app.section_folds.hidden.is_empty());
        send(&mut app, Message::Step(super::super::Direction::Back));
        assert_eq!(app.session.id(), id);
        assert_eq!(app.section_folds.hidden.len(), 1);
        assert_eq!(app.section_folds.hidden[0].heading.start, 0);
        assert_eq!(app.session.document.source(), format!("{source}!"));
    }

    #[test]
    fn folds_survive_unrelated_edits_and_switching_back_to_write() {
        let source = "Preamble\n\n# Title\n\nBody\n\n# Next\nTail";
        let heading = source.find('#').unwrap();
        let mut app = writing(source);
        click(&mut app, heading);
        send(&mut app, Message::SetMode(EditorMode::Source));
        assert_eq!(app.editor.snapshot(), source);
        send(&mut app, Message::Edit(Action::Edit(Edit::Insert('X'))));
        send(&mut app, Message::SetMode(EditorMode::Live));
        assert_eq!(app.section_folds.hidden[0].heading.start, heading + 1);
        send(&mut app, Message::Undo);
        assert_eq!(app.section_folds.hidden[0].heading.start, heading);
        app.set_document(source.into(), None, "Another.md".into());
        assert!(app.section_folds.hidden.is_empty());
    }
}
