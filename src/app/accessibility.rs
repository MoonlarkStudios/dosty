//! Opt-in native accessibility for the Markdown Source editor.

use std::sync::{Arc, Mutex};

use accesskit::ActionRequest;
use iced::advanced::widget::{Id, Operation, operation};
use iced::futures::{StreamExt, channel::mpsc};
use iced::{Rectangle, Task};
use iced_winit::accessibility::Config;

use super::accessibility_tree::{Intent, Snapshot};
use super::{App, EDITOR, EditorMode, Message};
use crate::document::selection::Selection;

type Inbox = Arc<Mutex<Option<mpsc::Receiver<ActionRequest>>>>;

/// Keeps native callbacks bounded and delivers them through Iced's event loop.
pub(in crate::app) struct Bootstrap {
    pub config: Config,
    inbox: Inbox,
}

impl Bootstrap {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let sender = Mutex::new(sender);
        let tree = Snapshot::new(
            "",
            Selection::default(),
            0,
            false,
            false,
            Rectangle::default(),
        )
        .tree("Dosty");
        let config = Config::new(tree, move |request| {
            // Native accessibility callbacks must never wait for the UI. A full
            // queue drops the request; the published tree stays authoritative.
            if let Ok(mut sender) = sender.try_lock() {
                let _ = sender.try_send(request);
            }
        });
        Self {
            config,
            inbox: Arc::new(Mutex::new(Some(receiver))),
        }
    }
}

pub(super) struct State {
    config: Config,
    snapshot: Snapshot,
    key: Option<(uuid::Uuid, u64, u64, bool)>,
    epoch: u64,
    layout: EditorLayout,
    composition: Option<(uuid::Uuid, u64)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EditorLayout {
    bounds: Option<Rectangle>,
    focused: bool,
}

impl App {
    pub(in crate::app) fn new_with_accessibility(
        bootstrap: Option<&Bootstrap>,
    ) -> (Self, Task<Message>) {
        let (mut app, task) = Self::new();
        let Some(bootstrap) = bootstrap else {
            return (app, task);
        };
        app.mode = EditorMode::Source;
        app.sync_editor();
        app.accessibility = Some(State {
            config: bootstrap.config.clone(),
            snapshot: Snapshot::new(
                "",
                Selection::default(),
                0,
                false,
                false,
                Rectangle::default(),
            ),
            key: None,
            epoch: 0,
            layout: EditorLayout::default(),
            composition: None,
        });
        let actions = bootstrap
            .inbox
            .lock()
            .ok()
            .and_then(|mut inbox| inbox.take())
            .map(|inbox| Task::run(inbox.map(Message::Accessibility), std::convert::identity))
            .unwrap_or_else(Task::none);
        let publish = app.refresh_accessibility(true);
        (
            app,
            Task::batch([
                task,
                actions,
                publish,
                iced::widget::operation::focus(EDITOR),
            ]),
        )
    }

    pub(super) fn refresh_accessibility(&mut self, measure: bool) -> Task<Message> {
        if self.accessibility.is_none() {
            return Task::none();
        }
        let title = self.title();
        let Some(state) = self.accessibility.as_mut() else {
            return Task::none();
        };
        let owner = (
            self.document.collaboration().id(),
            self.document.collaboration().replica_id(),
        );
        if self.mode != EditorMode::Source
            || state.composition.is_some_and(|pending| pending != owner)
        {
            state.composition = None;
        }
        let enabled = self.mode == EditorMode::Source
            && !self.loading
            && self.overlay.is_none()
            && state.composition.is_none();
        let key = (
            self.document.collaboration().id(),
            self.document.collaboration().replica_id(),
            self.document.revision(),
            enabled,
        );
        let bounds = state.layout.bounds.unwrap_or_default();
        if state.key != Some(key) {
            // Node IDs must not let a queued native action address a new source
            // revision, document session, or re-enabled editor by accident.
            state.epoch = state
                .epoch
                .checked_add(1)
                .expect("accessibility epoch overflow");
            state.snapshot = Snapshot::new(
                self.document.source(),
                self.selection,
                state.epoch,
                enabled,
                state.layout.focused,
                bounds,
            );
            state.key = Some(key);
        } else {
            state
                .snapshot
                .update_state(self.selection, enabled, state.layout.focused, bounds);
        }
        state.config.update(state.snapshot.tree(&title));
        if measure {
            iced::advanced::widget::operate(Probe::default()).map(Message::AccessibilityLayout)
        } else {
            Task::none()
        }
    }

    pub(super) fn accessibility_layout(&mut self, layout: EditorLayout) {
        if let Some(state) = self.accessibility.as_mut() {
            state.layout = layout;
        }
    }

    pub(super) fn accessibility_composition(
        &mut self,
        document: uuid::Uuid,
        replica: u64,
        pending: bool,
    ) {
        let owner = (
            self.document.collaboration().id(),
            self.document.collaboration().replica_id(),
        );
        if (document, replica) == owner
            && let Some(state) = self.accessibility.as_mut()
        {
            state.composition = pending.then_some(owner);
        }
    }

    pub(super) fn accessibility_action(&mut self, request: ActionRequest) -> Task<Message> {
        // Recheck current state as well as the snapshot: native requests arrive
        // asynchronously and must obey the same modal/loading guards as input.
        if self.mode != EditorMode::Source
            || self.loading
            || self.overlay.is_some()
            || self
                .accessibility
                .as_ref()
                .is_some_and(|state| state.composition.is_some())
        {
            return Task::none();
        }
        let Some(intent) = self
            .accessibility
            .as_ref()
            .and_then(|state| state.snapshot.action(&request))
        else {
            return Task::none();
        };
        match intent {
            Intent::Focus => {}
            Intent::Select(selection) => {
                self.selection = selection;
                self.document.break_history_group();
                self.sync_editor();
            }
            Intent::Replace(text) => {
                // Native clients can resend AXValue while reconciling a
                // control. An identical value must leave the directed source
                // selection, typing group and recovery state untouched.
                if text == self.document.source() {
                    return Task::none();
                }
                let before = self.selection;
                self.document.break_history_group();
                self.commit_local(
                    0..self.document.source().len(),
                    &text,
                    before,
                    Selection::caret(text.len()),
                );
                self.document.break_history_group();
                if self.edit_error.is_none() {
                    self.selection = Selection::caret(text.len());
                }
                self.sync_editor();
                self.changed();
            }
        }
        self.editor_focused = true;
        iced::widget::operation::focus(EDITOR)
    }
}

#[derive(Default)]
struct Probe(EditorLayout);

pub(super) fn event_message(
    event: iced::Event,
    _: iced::event::Status,
    _: iced::window::Id,
) -> Option<Message> {
    matches!(
        event,
        iced::Event::Window(
            iced::window::Event::Resized(_)
                | iced::window::Event::Focused
                | iced::window::Event::Unfocused
        )
    )
    .then_some(Message::AccessibilityRefresh)
}

impl Operation<EditorLayout> for Probe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<EditorLayout>)) {
        operate(self);
    }

    fn focusable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        state: &mut dyn operation::Focusable,
    ) {
        if id == Some(&Id::new(EDITOR)) {
            self.0 = EditorLayout {
                bounds: Some(bounds),
                focused: state.is_focused(),
            };
        }
    }

    fn finish(&self) -> operation::Outcome<EditorLayout> {
        operation::Outcome::Some(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Action, ActionData, NodeId, Role, TextPosition, TextSelection};
    use unicode_segmentation::UnicodeSegmentation;

    fn app(source: &str) -> App {
        let bootstrap = Bootstrap::new();
        let (mut app, _task) = App::new_with_accessibility(Some(&bootstrap));
        app.set_document(source.into(), None, "Accessibility.md".into());
        let _task = app.refresh_accessibility(false);
        app
    }

    fn editor(app: &App) -> NodeId {
        app.accessibility
            .as_ref()
            .unwrap()
            .snapshot
            .tree("Test")
            .nodes
            .iter()
            .find(|(_, node)| node.role() == Role::MultilineTextInput)
            .unwrap()
            .0
    }

    fn select(app: &App, anchor: usize, head: usize) -> ActionRequest {
        let target = editor(app);
        let run = NodeId(target.0 + 1);
        ActionRequest {
            target,
            action: Action::SetTextSelection,
            data: Some(ActionData::SetTextSelection(TextSelection {
                anchor: TextPosition {
                    node: run,
                    character_index: anchor,
                },
                focus: TextPosition {
                    node: run,
                    character_index: head,
                },
            })),
        }
    }

    fn source_event(
        app: &mut App,
        tree: &mut iced::advanced::widget::Tree,
        renderer: &iced::Renderer,
        event: iced::Event,
    ) {
        use iced::Size;
        use iced::advanced::{Layout, Shell, clipboard, layout, mouse};

        let size = Size::new(720.0, 800.0);
        let mut messages = Vec::new();
        {
            let mut editor = app.editor_view(true);
            let node = editor.as_widget_mut().layout(
                tree,
                renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            editor.as_widget_mut().update(
                tree,
                &event,
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(size),
            );
        }
        for message in messages {
            let _task = app.update(message);
        }
    }

    #[test]
    fn pending_source_composition_rejects_native_actions_through_clear_and_completion() {
        use iced::advanced::{Layout, input_method, layout, widget};
        use iced::{Event, Font, Renderer, Size};

        for commit in [true, false] {
            let source = "Before selected after";
            let mut app = app(source);
            let stale_select = select(&app, 7, 7);
            let stale_replace = ActionRequest {
                target: editor(&app),
                action: accesskit::Action::SetValue,
                data: Some(ActionData::Value("replacement".into())),
            };
            let renderer =
                Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::MONOSPACE, 14.5.into()));
            let mut tree = widget::Tree::new(app.editor_view(true).as_widget());
            {
                let mut editor = app.editor_view(true);
                let node = editor.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, Size::new(720.0, 800.0)),
                );
                let mut focus = operation::focusable::focus::<()>(Id::new(EDITOR));
                editor.as_widget_mut().operate(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &mut focus,
                );
            }
            source_event(
                &mut app,
                &mut tree,
                &renderer,
                Event::InputMethod(input_method::Event::Opened),
            );
            for preedit in ["にほん", ""] {
                source_event(
                    &mut app,
                    &mut tree,
                    &renderer,
                    Event::InputMethod(input_method::Event::Preedit(preedit.into(), None)),
                );
                assert!(app.accessibility.as_ref().unwrap().composition.is_some());
                assert_eq!(
                    app.accessibility
                        .as_ref()
                        .unwrap()
                        .snapshot
                        .tree("Test")
                        .nodes
                        .len(),
                    1
                );
                for request in [stale_select.clone(), stale_replace.clone()] {
                    let _task = app.update(Message::Accessibility(request));
                    assert_eq!(app.document.source(), source);
                    assert_eq!(app.selection, Selection::caret(0));
                    assert_eq!(app.editor.selection(), app.selection);
                }
            }
            source_event(
                &mut app,
                &mut tree,
                &renderer,
                Event::InputMethod(if commit {
                    input_method::Event::Commit("日本".into())
                } else {
                    input_method::Event::Closed
                }),
            );
            assert!(app.accessibility.as_ref().unwrap().composition.is_none());
            let expected = if commit {
                format!("日本{source}")
            } else {
                source.into()
            };
            assert_eq!(app.document.source(), expected);
            assert_eq!(app.editor.snapshot(), expected);
            let selection = app.selection;
            let _task = app.update(Message::Accessibility(stale_select));
            assert_eq!(app.selection, selection);
            let current = select(&app, 7, 7);
            let _task = app.update(Message::Accessibility(current));
            assert_ne!(app.selection, selection);
            if commit {
                let _task = app.update(Message::Undo);
                assert_eq!(app.document.source(), source);
                assert_eq!(app.selection, Selection::caret(0));
                assert!(!app.document.can_undo());
            }
        }
    }

    #[test]
    fn composition_notifications_cannot_disable_another_session() {
        let mut app = app("Before");
        let document = app.document.collaboration().id();
        let replica = app.document.collaboration().replica_id();
        let _task = app.update(Message::AccessibilityComposition {
            document,
            replica,
            pending: true,
        });
        assert!(app.accessibility.as_ref().unwrap().composition.is_some());
        app.set_document("After".into(), None, "Other.md".into());
        let _task = app.update(Message::Dismiss);
        assert!(app.accessibility.as_ref().unwrap().composition.is_none());
        let _task = app.update(Message::AccessibilityComposition {
            document,
            replica,
            pending: true,
        });
        assert!(app.accessibility.as_ref().unwrap().composition.is_none());
        let request = select(&app, 2, 1);
        let _task = app.update(Message::Accessibility(request));
        assert_eq!(app.selection, Selection { anchor: 2, head: 1 });
    }

    #[test]
    fn native_selection_and_value_use_source_coordinates_and_human_history() {
        let source = "# 日本語 👩🏽‍💻 e\u{301}";
        let mut app = app(source);
        let anchor = source.graphemes(true).count();
        let head = 2;
        let request = select(&app, anchor, head);
        let _task = app.update(Message::Accessibility(request));
        let selection = Selection {
            anchor: source.len(),
            head: 2,
        };
        assert_eq!(app.selection, selection);
        assert_eq!(app.document.source(), source);
        assert_eq!(app.editor.snapshot(), source);
        let _task = app.update(Message::Undo);
        assert_eq!(app.document.source(), source);

        let request = ActionRequest {
            target: editor(&app),
            action: Action::SetValue,
            data: Some(ActionData::Value("Replacement 日本語\n".into())),
        };
        let _task = app.update(Message::Accessibility(request));
        assert_eq!(app.document.source(), "Replacement 日本語\n");
        assert_eq!(app.editor.snapshot(), app.document.source());
        let _task = app.update(Message::Undo);
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, selection);
        let _task = app.update(Message::Redo);
        assert_eq!(app.document.source(), "Replacement 日本語\n");
    }

    #[test]
    fn native_source_movement_progresses_after_accessibility_grapheme_selections() {
        use iced::widget::text_editor::{Action, Motion};

        let source = "a e\u{301} 👩🏽‍💻 🇨🇦 日本語 z";
        let boundaries = source
            .grapheme_indices(true)
            .map(|(offset, _)| offset)
            .chain([source.len()])
            .collect::<Vec<_>>();
        let mut app = app(source);
        for (index, offsets) in boundaries.windows(2).enumerate() {
            let request = select(&app, index, index);
            let _task = app.update(Message::Accessibility(request));
            assert_eq!(app.selection, Selection::caret(offsets[0]));
            assert_eq!(app.editor.selection(), app.selection);
            // These are the real Iced/cosmic-text Source buffer motions. An AX
            // stop inside a combining/ZWJ cluster can make them stall.
            let _task = app.update(Message::Edit(Action::Move(Motion::Right)));
            assert_eq!(app.selection, Selection::caret(offsets[1]));
            let _task = app.update(Message::Edit(Action::Move(Motion::Left)));
            assert_eq!(app.selection, Selection::caret(offsets[0]));
            assert_eq!(app.document.source(), source);
            assert_eq!(app.editor.snapshot(), source);
        }
        assert!(!app.document.can_undo());
    }

    #[test]
    fn same_native_value_preserves_reverse_selection_and_typing_history() {
        use iced::widget::text_editor::{Action, Edit};

        let source = "日本語 unchanged";
        let mut app = app(source);
        let request = select(&app, source.graphemes(true).count(), 0);
        let _task = app.update(Message::Accessibility(request));
        let selection = app.selection;
        let revision = app.document.revision();
        let generation = app.document.collaboration().generation();
        let request = ActionRequest {
            target: editor(&app),
            action: accesskit::Action::SetValue,
            data: Some(ActionData::Value(source.into())),
        };
        let _task = app.update(Message::Accessibility(request));
        assert_eq!(app.selection, selection);
        assert_eq!(app.editor.selection(), selection);
        assert_eq!(app.document.source(), source);
        assert_eq!(app.document.revision(), revision);
        assert_eq!(app.document.collaboration().generation(), generation);
        assert!(!app.document.can_undo());
        assert!(!app.dirty);

        let request = select(
            &app,
            source.graphemes(true).count(),
            source.graphemes(true).count(),
        );
        let _task = app.update(Message::Accessibility(request));
        let _task = app.update(Message::Edit(Action::Edit(Edit::Insert('a'))));
        let request = ActionRequest {
            target: editor(&app),
            action: accesskit::Action::SetValue,
            data: Some(ActionData::Value(app.document.source().into())),
        };
        let _task = app.update(Message::Accessibility(request));
        let _task = app.update(Message::Edit(Action::Edit(Edit::Insert('b'))));
        assert_eq!(app.document.source(), format!("{source}ab"));
        let _task = app.update(Message::Undo);
        assert_eq!(app.document.source(), source);
        assert_eq!(app.selection, Selection::caret(source.len()));
    }

    #[test]
    fn stale_native_requests_and_unavailable_editors_do_not_mutate() {
        let mut app = app("Before 日本語");
        let stale = select(&app, 3, 1);
        app.set_document("After 日本語".into(), None, "Other.md".into());
        let _task = app.update(Message::Dismiss);
        let selection = app.selection;
        let _task = app.update(Message::Accessibility(stale));
        assert_eq!(app.selection, selection);

        for unavailable in 0..3 {
            app.mode = EditorMode::Source;
            app.loading = false;
            app.overlay = None;
            let _task = app.refresh_accessibility(false);
            let request = select(&app, 2, 1);
            match unavailable {
                0 => app.loading = true,
                1 => app.overlay = Some(super::super::Overlay::Settings),
                _ => app.mode = EditorMode::Live,
            }
            let _task = app.update(Message::Accessibility(request));
            assert_eq!(app.document.source(), "After 日本語");
            assert_eq!(app.selection, selection);
            let tree = app.accessibility.as_ref().unwrap().snapshot.tree("Test");
            assert_eq!(tree.nodes.len(), 1);
        }
    }

    #[test]
    fn measured_editor_focus_and_bounds_reach_the_semantic_tree() {
        use iced::advanced::{Layout, layout, widget};
        use iced::{Font, Point, Renderer, Size};

        let mut app = app("Source");
        let renderer =
            Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, 18.0.into()));
        let layout = {
            let mut source = app.editor_view(true);
            let mut tree = widget::Tree::new(source.as_widget());
            let node = source
                .as_widget_mut()
                .layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, Size::new(600.0, 500.0)),
                )
                .move_to(Point::new(300.0, 70.0));
            let mut focus = operation::focusable::focus::<()>(Id::new(EDITOR));
            source
                .as_widget_mut()
                .operate(&mut tree, Layout::new(&node), &renderer, &mut focus);
            let mut probe = Probe::default();
            source.as_widget_mut().operate(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &mut operation::black_box(&mut probe),
            );
            assert!(probe.0.focused);
            assert_eq!(probe.0.bounds, Some(node.bounds()));
            probe.0
        };
        let _task = app.update(Message::AccessibilityLayout(layout));
        let tree = app.accessibility.as_ref().unwrap().snapshot.tree("Test");
        assert_eq!(tree.focus, editor(&app));
        let node = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == editor(&app))
            .unwrap()
            .1;
        assert_eq!(
            node.bounds(),
            Some(accesskit::Rect::new(300.0, 70.0, 900.0, 570.0))
        );
    }
}
