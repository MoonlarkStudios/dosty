//! Keep list-filter navigation local to its focused text input.

use crate::app::Message;
use iced::advanced::widget::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::keyboard::{self, Key, key};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};

pub(crate) struct FilterInput<'a> {
    child: Element<'a, Message>,
    navigation: fn(key::Named) -> Option<Message>,
}

impl<'a> FilterInput<'a> {
    pub fn new(
        input: iced::widget::TextInput<'a, Message>,
        navigation: fn(key::Named) -> Option<Message>,
    ) -> Self {
        Self {
            child: input.into(),
            navigation,
        }
    }
}

impl Widget<Message, Theme, Renderer> for FilterInput<'_> {
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
        let focused =
            tree.children[0]
                .state
                .downcast_ref::<iced::widget::text_input::State<
                    <Renderer as iced::advanced::text::Renderer>::Paragraph,
                >>()
                .is_focused();
        if focused
            && let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event
            && modifiers.is_empty()
        {
            let action = match key.as_ref() {
                Key::Named(key) => (self.navigation)(key),
                _ => None,
            };
            if let Some(action) = action {
                shell.publish(action);
                shell.capture_event();
                return;
            }
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

impl<'a> From<FilterInput<'a>> for Element<'a, Message> {
    fn from(widget: FilterInput<'a>) -> Self {
        Self::new(widget)
    }
}
