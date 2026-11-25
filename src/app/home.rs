//! Welcome at startup and while a document is still open.

use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::widget::{button, column, container, row, scrollable, space, text, tooltip};
use iced::{
    Alignment, Element, Event, Fill, Length, Rectangle, Renderer, Size, Theme, Vector, event,
    window,
};

use super::{App, Message, chrome, folder, projects, quick_open, workspace};
use crate::app::{
    brand,
    icon::{self, Icon},
};
use crate::theme::styles::{self, Button};

const MAX_RECENT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Startup,
    Document,
    Welcome,
}

/// Keep both widget trees so returning to the document retains the editor's
/// viewport, wrap affinity, and focus. Only the visible page participates in
/// layout, input, and widget operations.
pub(super) struct Pages<'a> {
    visible: Element<'a, Message>,
    slot: usize,
}

impl<'a> Pages<'a> {
    pub(super) fn view(visible: Element<'a, Message>, welcome: bool) -> Element<'a, Message> {
        Element::new(Self {
            visible,
            slot: usize::from(welcome),
        })
    }
}

impl Widget<Message, Theme, Renderer> for Pages<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<Pages<'static>>()
    }

    fn children(&self) -> Vec<widget::Tree> {
        let mut children = vec![widget::Tree::empty(), widget::Tree::empty()];
        children[self.slot] = widget::Tree::new(self.visible.as_widget());
        children
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.children[self.slot].diff(self.visible.as_widget());
    }

    fn size(&self) -> Size<Length> {
        self.visible.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.visible
            .as_widget_mut()
            .layout(&mut tree.children[self.slot], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.visible.as_widget_mut().operate(
            &mut tree.children[self.slot],
            layout,
            renderer,
            operation,
        );
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.visible.as_widget_mut().update(
            &mut tree.children[self.slot],
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
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.visible.as_widget().draw(
            &tree.children[self.slot],
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
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.visible.as_widget().mouse_interaction(
            &tree.children[self.slot],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.visible.as_widget_mut().overlay(
            &mut tree.children[self.slot],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Keep app shortcuts available on Welcome. Document shortcuts belong to the editor.
pub(super) fn event_message(
    event: Event,
    status: event::Status,
    id: window::Id,
) -> Option<Message> {
    let keyboard = matches!(event, Event::Keyboard(_));
    let message = super::event_message(event, status, id)?;
    (!keyboard
        || matches!(
            message,
            Message::New
                | Message::Open
                | Message::CloseOverlay
                | Message::ShowSettings
                | Message::QuickOpen(quick_open::Message::Open)
                | Message::Folder(folder::Message::Pick)
                | Message::Chrome(chrome::Message::Menu(chrome::Menu::File))
        ))
    .then_some(message)
}

impl App {
    pub(super) fn is_home(&self) -> bool {
        self.page != Page::Document
    }

    pub(super) fn return_to_document(&mut self) -> bool {
        if self.page != Page::Welcome {
            return false;
        }
        self.page = Page::Document;
        true
    }

    pub(super) fn home_view(&self, size: Size) -> Element<'_, Message> {
        let tokens = self.resolved_theme.tokens;
        let ready = !self.busy && !self.loading;
        let action = |label, kind, message, style| {
            let foreground = if matches!(style, Button::Primary) {
                tokens.on_accent
            } else {
                tokens.muted
            };
            let key = chrome::shortcut_label(&message).unwrap_or_default();
            button(
                row![
                    icon::sized(kind, foreground, 17.0),
                    text(label).size(14),
                    space::horizontal(),
                    text(key).size(12).color(foreground),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([11, 13])
            .width(Fill)
            .on_press_maybe(ready.then_some(message))
            .style(styles::button_style(tokens, style))
        };
        let returning = self.page == Page::Welcome;
        let mut documents = column![text("Documents").size(19)].spacing(7).width(Fill);
        if returning {
            documents = documents.push(action(
                "Return to Document",
                Icon::Document,
                Message::ReturnToDocument,
                Button::Primary,
            ));
        }
        documents = documents
            .push(action(
                "New Document",
                Icon::Plus,
                Message::New,
                if returning {
                    Button::Quiet
                } else {
                    Button::Primary
                },
            ))
            .push(action(
                "Open Markdown File…",
                Icon::Document,
                Message::Open,
                Button::Quiet,
            ))
            .push(action(
                "Quick Open…",
                Icon::Search,
                Message::QuickOpen(quick_open::Message::Open),
                Button::Quiet,
            ));

        let project_button = |label, kind, message, style| {
            button(
                row![icon::sized(kind, tokens.muted, 15.0), text(label).size(13)]
                    .spacing(6)
                    .align_y(Alignment::Center),
            )
            .padding([8, 11])
            .on_press_maybe(ready.then_some(message))
            .style(styles::button_style(tokens, style))
        };
        let project_actions = row![
            project_button(
                "Open Project…",
                Icon::Folder,
                Message::Folder(folder::Message::Pick),
                Button::Selected
            ),
            project_button(
                "New Project…",
                Icon::Plus,
                Message::Project(projects::Message::New),
                Button::Quiet
            ),
        ]
        .spacing(6);
        let mut project_list = column![].spacing(9).width(Fill);
        if self.workspace.places().is_empty() {
            project_list = project_list.push(
                text("Keep related Markdown files together in a folder.")
                    .size(13)
                    .color(tokens.muted),
            );
        }
        project_list = project_list.push(project_actions);
        if !self.workspace.places().is_empty() {
            project_list = project_list.push(text("Recent projects").size(12).color(tokens.muted));
            for (index, project) in self.workspace.recent_projects().take(MAX_RECENT) {
                let path = workspace::project_path(project);
                let label = container(
                    column![
                        text(self.workspace.project_name(project))
                            .size(14)
                            .wrapping(text::Wrapping::None),
                        text(path.clone())
                            .size(11)
                            .color(tokens.muted)
                            .wrapping(text::Wrapping::None),
                    ]
                    .spacing(3),
                )
                .clip(true)
                .width(Fill);
                let open = button(
                    row![
                        icon::sized(Icon::Folder, tokens.muted, 16.0),
                        label,
                        icon::sized(Icon::ChevronRight, tokens.muted, 12.0),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([7, 9])
                .width(Fill)
                .on_press_maybe(
                    ready.then_some(Message::Workspace(workspace::Message::OpenPlace(index))),
                )
                .style(styles::button_style(tokens, Button::Quiet));
                project_list =
                    project_list.push(styles::hint(tokens, open, path, tooltip::Position::Bottom));
            }
        }
        let projects = column![
            text("Projects").size(19),
            container(project_list)
                .padding(16)
                .width(Fill)
                .style(styles::inset(tokens)),
        ]
        .spacing(7)
        .width(Fill);
        let sections: Element<'_, Message> = if size.width >= 700.0 {
            row![
                container(documents).width(iced::Length::FillPortion(5)),
                container(projects).width(iced::Length::FillPortion(6)),
            ]
            .spacing(32)
            .align_y(Alignment::Start)
            .into()
        } else {
            column![documents, projects].spacing(24).into()
        };
        let mut contents = column![brand::view(tokens)].spacing(22);
        if self.page == Page::Startup && (!self.workspace.settled() || self.loading) {
            contents = contents.push(text("Opening Dosty…").size(20).color(tokens.muted));
        } else {
            contents = contents.push(text("Welcome").size(42)).push(sections);
        }
        let page = container(contents.width(Fill))
            .max_width(922)
            .padding(iced::Padding::new(32.0).top(if size.height >= 700.0 { 82.0 } else { 38.0 }))
            .width(Fill);
        column![
            self.home_titlebar(),
            scrollable(container(page).center_x(Fill))
                .height(Fill)
                .style(styles::scrollbar(tokens)),
        ]
        .height(Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Message};
    use crate::io::storage::LoadedFile;

    #[test]
    fn new_document_exits_welcome_even_when_the_draft_is_empty() {
        let (mut app, _) = App::new();
        app.workspace.remember_for_preview(&[]);
        assert!(app.is_home());
        assert_eq!(app.title(), "Dosty");
        drop(app.update(Message::New));
        assert!(!app.is_home());
        assert!(app.session.document.source().is_empty());
        assert!(app.title().starts_with("Untitled.md"));
    }

    #[test]
    fn opening_a_file_exits_welcome_and_new_document_stays_in_the_editor() {
        let (mut app, _) = App::new();
        app.workspace.remember_for_preview(&[]);
        drop(app.update(Message::LoadedStandalone(Ok(LoadedFile {
            path: "/writing/note.md".into(),
            source: "Note".into(),
            collaboration: None,
        }))));
        assert!(!app.is_home());
        drop(app.update(Message::New));
        assert!(!app.is_home());
    }
}
