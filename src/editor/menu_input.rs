//! Deliver native Edit commands through the focused Iced widget's normal bindings.

use iced::advanced::widget::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::keyboard::{self, Key, key};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};

use crate::app::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Request {
    pub sequence: u64,
    pub command: Command,
}

pub(crate) struct MenuInput<'a> {
    child: Element<'a, Message>,
    requests: &'a [Request],
}

#[derive(Default)]
struct State {
    consumed: Option<u64>,
    modifiers: keyboard::Modifiers,
}

impl<'a> MenuInput<'a> {
    pub fn new(child: Element<'a, Message>, requests: &'a [Request]) -> Self {
        Self { child, requests }
    }
}

impl Widget<Message, Theme, Renderer> for MenuInput<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(self.child.as_widget())]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.children[0].diff(self.child.as_widget());
    }
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
        }
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        // Consume each request once, on redraw, after layout and focus operations.
        // This shares selection, IME, table drafts, clipboard guards and modal
        // text-input behavior with real keyboard input instead of editing source.
        if !matches!(
            event,
            Event::Window(iced::window::Event::RedrawRequested(_))
        ) {
            return;
        }
        let consumed = &mut state.consumed;
        let physical_modifiers = state.modifiers;
        let start = self
            .requests
            .iter()
            .position(|request| Some(request.sequence) == *consumed)
            .map_or(0, |index| index + 1);
        for request in &self.requests[start..] {
            *consumed = Some(request.sequence);
            let (character, shift) = match request.command {
                Command::Undo => ("z", false),
                Command::Redo => ("z", true),
                Command::Cut => ("x", false),
                Command::Copy => ("c", false),
                Command::Paste => ("v", false),
                Command::SelectAll => ("a", false),
            };
            let modifiers = keyboard::Modifiers::COMMAND
                | if shift {
                    keyboard::Modifiers::SHIFT
                } else {
                    keyboard::Modifiers::empty()
                };
            let event = Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Character(character.into()),
                modified_key: Key::Character(character.into()),
                physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
                location: keyboard::Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            });
            let mut dispatch = |event: &Event| {
                let mut messages = Vec::new();
                let mut input_shell = Shell::new(&mut messages);
                self.child.as_widget_mut().update(
                    &mut tree.children[0],
                    event,
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    &mut input_shell,
                    viewport,
                );
                let status = input_shell.event_status();
                shell.merge(input_shell, std::convert::identity);
                status
            };
            // Text inputs track modifier state separately and cache paste until
            // key release. Restore the real modifiers after the menu command.
            dispatch(&Event::Keyboard(keyboard::Event::ModifiersChanged(
                modifiers,
            )));
            let status = dispatch(&event);
            dispatch(&Event::Keyboard(keyboard::Event::KeyReleased {
                key: Key::Character(character.into()),
                modified_key: Key::Character(character.into()),
                physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
                location: keyboard::Location::Standard,
                modifiers,
            }));
            dispatch(&Event::Keyboard(keyboard::Event::ModifiersChanged(
                physical_modifiers,
            )));
            if status == iced::event::Status::Ignored {
                match request.command {
                    Command::Undo => shell.publish(Message::Undo),
                    Command::Redo => shell.publish(Message::Redo),
                    _ => {}
                }
            }
        }
        if start < self.requests.len()
            && let Some(sequence) = *consumed
        {
            shell.publish(Message::MenuEditConsumed(sequence));
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<MenuInput<'a>> for Element<'a, Message> {
    fn from(widget: MenuInput<'a>) -> Self {
        Self::new(widget)
    }
}
