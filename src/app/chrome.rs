//! Native quiet titlebar, independent sidebars, and document commands.

use iced::advanced::widget;
use iced::keyboard::{self, Key, key};
use iced::widget::{
    button, column, container, mouse_area, operation, row, scrollable, space, text, tooltip,
};
use iced::{Alignment, Element, Event, Fill, Padding, Size, Task, window};

use super::{
    App, EDITOR, EditorMode, Format, Message as AppMessage, Overlay, Sidebar, folder, recovery,
    writing,
};
use crate::app::brand;
use crate::app::icon::{self, Icon};
use crate::theme::styles::{self, Button};

// Match the compact native traffic-light row without replacing or repositioning
// the system window buttons.
pub(super) const TITLEBAR_HEIGHT: f32 = if cfg!(target_os = "macos") {
    30.0
} else {
    52.0
};
pub(super) const STATISTICS_HEIGHT: f32 = 28.0;
pub(super) const FILES_WIDTH: f32 = 260.0;
pub(super) const OUTLINE_WIDTH: f32 = 240.0;
const MIN_WRITING_WIDTH: f32 = 480.0;

const MENU_SCROLL: &str = "titlebar-context-menu-scroll";

fn menu_item_id(index: usize) -> widget::Id {
    widget::Id::from(format!("titlebar-context-menu-item-{index}"))
}

/// Keep keyboard choices visible using the same measured bounds as the menu.
/// Moving the selection changes styling, so native focus scrolling cannot do it.
fn reveal_menu_item(index: usize) -> Task<AppMessage> {
    struct Locate {
        target: widget::Id,
        viewport: Option<(f32, f32, f32)>,
        item: Option<iced::Rectangle>,
    }

    impl widget::Operation<f32> for Locate {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation<f32>)) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&widget::Id>,
            bounds: iced::Rectangle,
            content: iced::Rectangle,
            translation: iced::Vector,
            _state: &mut dyn widget::operation::Scrollable,
        ) {
            if id == Some(&widget::Id::from(MENU_SCROLL)) {
                self.viewport = Some((content.y, bounds.height, translation.y));
            }
        }

        fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
            if id == Some(&self.target) {
                self.item = Some(bounds);
            }
        }

        fn finish(&self) -> widget::operation::Outcome<f32> {
            let offset =
                self.viewport
                    .zip(self.item)
                    .and_then(|((origin, height, offset), item)| {
                        let top = item.y - origin;
                        let bottom = top + item.height;
                        if top < offset {
                            Some(top.max(0.0))
                        } else if bottom > offset + height {
                            Some((bottom - height).max(0.0))
                        } else {
                            None
                        }
                    });
            offset.map_or(
                widget::operation::Outcome::None,
                widget::operation::Outcome::Some,
            )
        }
    }

    widget::operate(Locate {
        target: menu_item_id(index),
        viewport: None,
        item: None,
    })
    .then(|offset| {
        operation::scroll_to(
            MENU_SCROLL,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(offset),
            },
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    File,
    Edit,
    Document,
    Format,
    Help,
    Navigation,
    Preferences,
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preference {
    WordCount,
    ReadingTime,
}

#[derive(Debug, Clone)]
pub enum Message {
    Menu(Menu),
    Run(Box<AppMessage>),
    DragWindow,
    MaximizeWindow,
    Preference(Preference, bool),
    Step(i32),
    Hover(usize),
    Accept,
    Expand,
    Back,
}

pub(super) struct State {
    pub menu: Option<Menu>,
    pub files: Panel,
    pub outline: Panel,
    pub sidebars_hidden: bool,
    pub saving: bool,
    pub save_error: Option<String>,
    selected: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Panel {
    pub open: bool,
}

impl State {
    pub(super) fn panel(&self, side: Sidebar) -> Panel {
        match side {
            Sidebar::Files => self.files,
            Sidebar::Outline => self.outline,
        }
    }
    pub(super) fn panel_mut(&mut self, side: Sidebar) -> &mut Panel {
        match side {
            Sidebar::Files => &mut self.files,
            Sidebar::Outline => &mut self.outline,
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            menu: None,
            files: Panel { open: false },
            outline: Panel { open: false },
            sidebars_hidden: false,
            saving: false,
            save_error: None,
            selected: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveState {
    Unsaved,
    Modified,
    Saved,
    Saving,
    Failed,
}

struct Command {
    label: &'static str,
    icon: Icon,
    action: Message,
    enabled: bool,
}

impl Command {
    fn action(label: &'static str, icon: Icon, action: AppMessage, enabled: bool) -> Self {
        Self {
            label,
            icon,
            action: Message::Run(Box::new(action)),
            enabled,
        }
    }
    fn menu(label: &'static str, icon: Icon, menu: Menu) -> Self {
        Self {
            label,
            icon,
            action: Message::Menu(menu),
            enabled: true,
        }
    }
}

pub(super) fn event_message(
    event: Event,
    status: iced::event::Status,
    id: window::Id,
) -> Option<AppMessage> {
    if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = &event
        && (modifiers.is_empty()
            || *modifiers == keyboard::Modifiers::SHIFT
                && matches!(key.as_ref(), Key::Named(key::Named::Tab)))
    {
        let message = match key.as_ref() {
            Key::Named(key::Named::ArrowUp) => Some(Message::Step(-1)),
            Key::Named(key::Named::ArrowDown) => Some(Message::Step(1)),
            Key::Named(key::Named::Tab) => {
                Some(Message::Step(if modifiers.shift() { -1 } else { 1 }))
            }
            Key::Named(key::Named::Enter | key::Named::Space) => Some(Message::Accept),
            Key::Named(key::Named::ArrowRight) => Some(Message::Expand),
            Key::Named(key::Named::ArrowLeft) => Some(Message::Back),
            _ => None,
        };
        if let Some(message) = message {
            return Some(AppMessage::Chrome(message));
        }
    }
    if matches!(event, Event::Window(window::Event::Unfocused)) {
        return Some(AppMessage::CloseOverlay);
    }
    super::event_message(event, status, id)
}

impl App {
    pub(super) fn show_sidebar(&mut self, side: Sidebar) -> Task<AppMessage> {
        if side == Sidebar::Files && self.folder.root().is_none() {
            return Task::none();
        }
        self.chrome.sidebars_hidden = false;
        self.chrome.panel_mut(side).open = true;
        self.preserve_viewport_generation = Some(self.document.collaboration().generation());
        if side == Sidebar::Files {
            self.editor_focused = false;
            operation::focus(crate::editor::file_tree::ID)
        } else {
            operation::focus(EDITOR)
        }
    }

    pub(super) fn toggle_sidebar(&mut self, side: Sidebar) -> Task<AppMessage> {
        if self.overlay.is_some() {
            return Task::none();
        }
        if self.chrome.sidebars_hidden || !self.chrome.panel(side).open {
            return self.show_sidebar(side);
        }
        self.chrome.panel_mut(side).open = false;
        self.preserve_viewport_generation = Some(self.document.collaboration().generation());
        operation::focus(EDITOR)
    }

    pub(super) fn docked_sidebars(&self, width: f32) -> (bool, bool) {
        if self.chrome.sidebars_hidden {
            return (false, false);
        }
        let files = self.folder.root().is_some()
            && self.chrome.files.open
            && width >= FILES_WIDTH + MIN_WRITING_WIDTH;
        let remaining = width - if files { FILES_WIDTH } else { 0.0 };
        let outline = self.chrome.outline.open && remaining >= OUTLINE_WIDTH + MIN_WRITING_WIDTH;
        (files, outline)
    }

    pub(super) fn workspace_view(&self, size: Size) -> Element<'_, AppMessage> {
        let (files_docked, outline_docked) = self.docked_sidebars(size.width);
        let content = column![
            self.search_bar(),
            self.writing_surface(),
            self.statistics_view()
        ]
        .height(Fill);
        let writing = row![
            if files_docked {
                self.sidebar_view(Sidebar::Files)
            } else {
                space().width(0).into()
            },
            content,
            if outline_docked {
                self.sidebar_view(Sidebar::Outline)
            } else {
                space().width(0).into()
            },
        ]
        .height(Fill);
        let mut layers = iced::widget::stack![writing];
        let files_floating = self.folder.root().is_some()
            && self.chrome.files.open
            && !files_docked
            && !self.chrome.sidebars_hidden;
        let outline_floating =
            self.chrome.outline.open && !outline_docked && !self.chrome.sidebars_hidden;
        if files_floating || outline_floating {
            for (side, visible) in [
                (Sidebar::Files, files_floating),
                (Sidebar::Outline, outline_floating),
            ] {
                if !visible {
                    continue;
                }
                let panel = container(self.sidebar_view(side)).height(Fill);
                let panel = container(panel).align_top(Fill);
                let panel = match side {
                    Sidebar::Files => panel.align_left(Fill),
                    Sidebar::Outline => panel.align_right(Fill),
                };
                layers = layers.push(panel);
            }
        }
        column![self.titlebar_view(), layers].height(Fill).into()
    }

    pub(super) fn update_chrome(&mut self, message: Message) -> Task<AppMessage> {
        match message {
            Message::DragWindow => return window::latest().and_then(window::drag),
            Message::MaximizeWindow => return window::latest().and_then(window::toggle_maximize),
            Message::Preference(preference, enabled) => {
                match preference {
                    Preference::WordCount => self.preferences.show_word_count = enabled,
                    Preference::ReadingTime => self.preferences.show_reading_time = enabled,
                }
                return self.persist_preferences();
            }
            Message::Menu(menu) => {
                if self.busy
                    || self
                        .overlay
                        .is_some_and(|overlay| overlay != Overlay::Chrome)
                {
                    return Task::none();
                }
                self.chrome.menu = Some(menu);
                self.chrome.selected = 0;
                self.overlay = Some(Overlay::Chrome);
                return operation::scroll_to(
                    MENU_SCROLL,
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(0.0),
                    },
                );
            }
            Message::Step(direction) => {
                if let Some(menu) = self.chrome.menu {
                    let count = self
                        .chrome_commands(menu)
                        .iter()
                        .filter(|command| command.enabled)
                        .count();
                    if count > 0 {
                        self.chrome.selected = (self.chrome.selected as i64 + i64::from(direction))
                            .rem_euclid(count as i64)
                            as usize;
                    }
                }
                return self.reveal_chrome_choice();
            }
            Message::Hover(index) => self.chrome.selected = index,
            Message::Accept | Message::Expand => {
                if let Some(menu) = self.chrome.menu {
                    let action = self
                        .chrome_commands(menu)
                        .into_iter()
                        .filter(|command| command.enabled)
                        .nth(self.chrome.selected)
                        .map(|command| command.action);
                    if let Some(action) = action
                        && (matches!(message, Message::Accept)
                            || matches!(action, Message::Menu(_)))
                    {
                        return self.update_chrome(action);
                    }
                }
            }
            Message::Back => {
                if matches!(
                    self.chrome.menu,
                    Some(
                        Menu::Help
                            | Menu::Edit
                            | Menu::Format
                            | Menu::Navigation
                            | Menu::Preferences
                            | Menu::Workspace
                    )
                ) {
                    return self.update_chrome(Message::Menu(Menu::File));
                }
                return self.update_message(AppMessage::CloseOverlay);
            }
            Message::Run(message) => {
                self.chrome.menu = None;
                if self.overlay == Some(Overlay::Chrome) {
                    self.overlay = None;
                }
                return self.update_message(*message);
            }
        }
        Task::none()
    }

    fn reveal_chrome_choice(&self) -> Task<AppMessage> {
        let Some(menu) = self.chrome.menu else {
            return Task::none();
        };
        let Some((index, _)) = self
            .chrome_commands(menu)
            .iter()
            .enumerate()
            .filter(|(_, command)| command.enabled)
            .nth(self.chrome.selected)
        else {
            return Task::none();
        };
        reveal_menu_item(index)
    }

    pub(super) fn toggle_hidden_sidebars(&mut self) -> Task<AppMessage> {
        if self.overlay.is_some() || self.loading {
            return Task::none();
        }
        if !self.chrome.files.open && !self.chrome.outline.open {
            return Task::none();
        }
        self.chrome.sidebars_hidden = !self.chrome.sidebars_hidden;
        self.preserve_viewport_generation = Some(self.document.collaboration().generation());
        operation::focus(EDITOR)
    }

    pub(super) fn reveal_navigation(&mut self) {
        self.chrome.sidebars_hidden = false;
        self.chrome.files.open = true;
        self.preserve_viewport_generation = Some(self.document.collaboration().generation());
    }

    fn save_state(&self) -> SaveState {
        if self.chrome.saving {
            SaveState::Saving
        } else if self.chrome.save_error.is_some() {
            SaveState::Failed
        } else if self.path.is_none() {
            SaveState::Unsaved
        } else if self.dirty {
            SaveState::Modified
        } else {
            SaveState::Saved
        }
    }

    fn writing_statistics(&self) -> String {
        let words = self.document.words();
        let mut parts = Vec::with_capacity(2);
        if self.preferences.show_word_count {
            parts.push(format!(
                "{words} {}",
                if words == 1 { "word" } else { "words" }
            ));
        }
        if self.preferences.show_reading_time {
            // A deliberately approximate reading estimate, independent of edit mode.
            parts.push(if words == 0 {
                "0 min read".into()
            } else if words < 200 {
                "<1 min read".into()
            } else {
                format!("~{} min read", words.div_ceil(200))
            });
        }
        parts.join(" · ")
    }

    pub(super) fn statistics_view(&self) -> Element<'_, AppMessage> {
        // This belongs to the writing column, so it stays centered between any
        // docked navigation and the writing area's opposite edge. Keep the same
        // small reading gutter when preferences change to preserve the viewport.
        container(
            text(self.writing_statistics())
                .size(11)
                .color(self.resolved_theme.tokens.muted),
        )
        .center_x(Fill)
        .center_y(STATISTICS_HEIGHT)
        .id("writing-statistics")
        .into()
    }

    fn titlebar_icon<'a>(
        &self,
        kind: Icon,
        label: &'a str,
        message: AppMessage,
        selected: bool,
    ) -> Element<'a, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        tooltip(
            button(icon::view(
                kind,
                if selected {
                    tokens.on_accent_soft
                } else {
                    tokens.muted
                },
            ))
            .on_press(message)
            .padding(6)
            .style(styles::button_style(
                tokens,
                if selected {
                    Button::Selected
                } else {
                    Button::Quiet
                },
            )),
            container(text(label).size(11))
                .padding([6, 9])
                .style(styles::context_menu(tokens)),
            tooltip::Position::Bottom,
        )
        .into()
    }

    pub(super) fn titlebar_view(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        iced::widget::responsive(move |size| {
            let side_width = if size.width < 900.0 { 236.0 } else { 280.0 };
            // Preserve the actual macOS traffic lights, including their native hit targets.
            let native_inset = if cfg!(target_os = "macos") { 76.0 } else { 0.0 };
            let file: Element<'_, AppMessage> = if cfg!(target_os = "macos") {
                space().width(0).into()
            } else {
                button(text("File").size(12))
                    .padding([5, 10])
                    .on_press(AppMessage::Chrome(Message::Menu(Menu::File)))
                    .style(styles::button_style(
                        tokens,
                        if self.chrome.menu == Some(Menu::File) {
                            Button::Selected
                        } else {
                            Button::Quiet
                        },
                    ))
                    .into()
            };
            let files: Element<'_, AppMessage> = if self.folder.root().is_some() {
                container(self.titlebar_icon(
                    Icon::Sidebar,
                    "Workspace / Files · ⌘/Ctrl Shift B",
                    AppMessage::ToggleSidebar(Sidebar::Files),
                    self.chrome.files.open && !self.chrome.sidebars_hidden,
                ))
                .id("titlebar-files")
                .into()
            } else {
                space().width(0).into()
            };
            let left = row![space().width(native_inset), files, file,]
                .spacing(4)
                .align_y(Alignment::Center)
                .width(side_width);
            let name =
                tooltip(
                    button(
                        text(&self.display_name)
                            .size(12)
                            .wrapping(text::Wrapping::None),
                    )
                    .padding([5, 6])
                    .on_press(AppMessage::Chrome(Message::Menu(Menu::Document)))
                    .style(styles::button_style(tokens, Button::Quiet)),
                    container(
                        text(self.path.as_ref().map_or_else(
                            || "Not saved yet".into(),
                            |path| path.display().to_string(),
                        ))
                        .size(11),
                    )
                    .padding(8)
                    .style(styles::context_menu(tokens)),
                    tooltip::Position::Bottom,
                );
            let (status_icon, status_label, status_color) = match self.save_state() {
                SaveState::Unsaved => (
                    Icon::Unsaved,
                    "Not saved yet · Click to save".to_owned(),
                    tokens.accent,
                ),
                SaveState::Modified => (
                    Icon::Modified,
                    "Unsaved changes · Click to save".to_owned(),
                    tokens.accent,
                ),
                SaveState::Saved => (Icon::Check, "All changes saved".to_owned(), tokens.muted),
                SaveState::Saving => (Icon::Save, "Saving…".to_owned(), tokens.muted),
                SaveState::Failed => (
                    Icon::Warning,
                    format!(
                        "Save failed: {}",
                        self.chrome
                            .save_error
                            .as_deref()
                            .unwrap_or("Try saving again")
                    ),
                    tokens.danger,
                ),
            };
            let status = tooltip(
                button(icon::view(status_icon, status_color))
                    .on_press_maybe((!self.busy).then_some(
                        if self.save_state() == SaveState::Saved {
                            AppMessage::Chrome(Message::Menu(Menu::Document))
                        } else {
                            AppMessage::Save
                        },
                    ))
                    .padding(6)
                    .style(styles::button_style(tokens, Button::Quiet)),
                container(text(status_label).size(11))
                    .max_width(360)
                    .padding(8)
                    .style(styles::context_menu(tokens)),
                tooltip::Position::Bottom,
            );
            let title = container(
                row![
                    container(name)
                        .max_width(
                            (size.width
                                - side_width * 2.0
                                - 74.0
                                - if self.mode == EditorMode::Live {
                                    0.0
                                } else {
                                    42.0
                                })
                            .max(48.0)
                        )
                        .clip(true)
                        .id("titlebar-name"),
                    match self.mode {
                        EditorMode::Live => space().width(0).into(),
                        mode => Element::from(
                            text(if mode == EditorMode::Source {
                                "Source"
                            } else {
                                "Read"
                            })
                            .size(10)
                            .color(tokens.muted)
                        ),
                    },
                    container(status).id("titlebar-save-state"),
                ]
                .spacing(2)
                .align_y(Alignment::Center),
            )
            .center_x(Fill);
            let format: Element<'_, AppMessage> = {
                button(text("Format").size(12))
                    .padding([5, 10])
                    .on_press(AppMessage::Chrome(Message::Menu(Menu::Format)))
                    .style(styles::button_style(
                        tokens,
                        if self.chrome.menu == Some(Menu::Format) {
                            Button::Selected
                        } else {
                            Button::Quiet
                        },
                    ))
                    .into()
            };
            let right = row![
                space::horizontal(),
                container(format).id("titlebar-format"),
                container(self.titlebar_icon(
                    Icon::Outline,
                    "Outline · ⌘/Ctrl Alt O",
                    AppMessage::ToggleSidebar(Sidebar::Outline),
                    self.chrome.outline.open && !self.chrome.sidebars_hidden
                ))
                .id("titlebar-outline"),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .width(side_width);
            mouse_area(
                container(
                    row![left, title, right]
                        .spacing(8)
                        .align_y(Alignment::Center),
                )
                .padding(Padding::new(0.0).left(12.0).right(12.0))
                .width(Fill)
                .center_y(TITLEBAR_HEIGHT)
                .style(styles::panel(tokens.surface)),
            )
            .on_press(AppMessage::Chrome(Message::DragWindow))
            .on_double_click(AppMessage::Chrome(Message::MaximizeWindow))
            .into()
        })
        .height(TITLEBAR_HEIGHT)
        .into()
    }

    pub(super) fn context_actions(&self) -> Option<Element<'_, AppMessage>> {
        if self.chrome.sidebars_hidden {
            return None;
        }
        let mut actions = row![].spacing(8).align_y(Alignment::Center);
        if self.folder.can_undo() {
            if let Some(notice) = self.folder.notice() {
                actions = actions.push(text(notice).size(12));
            }
            actions = actions.push(self.small_button(
                "Undo move",
                AppMessage::Folder(folder::Message::UndoMove),
                false,
            ));
        }
        if self.footnotes.can_return() {
            actions = actions.push(self.small_button(
                "Back to reference",
                AppMessage::Footnote(super::footnotes::Message::Back),
                false,
            ));
        }
        if !self.folder.can_undo() && !self.footnotes.can_return() {
            return None;
        }
        Some(
            container(
                container(actions)
                    .padding(8)
                    .style(styles::context_menu(self.resolved_theme.tokens)),
            )
            .align_bottom(Fill)
            .center_x(Fill)
            .padding(Padding::new(16.0).bottom(STATISTICS_HEIGHT + 12.0))
            .into(),
        )
    }

    fn chrome_commands(&self, menu: Menu) -> Vec<Command> {
        let ready = !self.busy;
        match menu {
            Menu::File => {
                let new = self.folder.root().map_or(AppMessage::New, |root| {
                    AppMessage::Folder(folder::Message::NewDocument(root.to_owned()))
                });
                vec![
                    Command::action("New document", Icon::Plus, new, ready),
                    Command::action("Open file…", Icon::Document, AppMessage::Open, ready),
                    Command::action(
                        "Quick Open…",
                        Icon::Search,
                        AppMessage::QuickOpen(super::quick_open::Message::Open),
                        ready && self.folder.root().is_some(),
                    ),
                    Command::action(
                        "Open folder…",
                        Icon::Folder,
                        AppMessage::Folder(folder::Message::Pick),
                        ready,
                    ),
                    Command::action("Save as…", Icon::Save, AppMessage::SaveAs, ready),
                    Command::menu("Document actions…", Icon::Document, Menu::Document),
                    Command::action("Export HTML…", Icon::Export, AppMessage::Export, ready),
                    Command::action(
                        "Find & replace…",
                        Icon::Search,
                        AppMessage::ToggleSearch,
                        true,
                    ),
                    Command::menu("Formatting…", Icon::Heading, Menu::Format),
                    Command::menu("Edit…", Icon::Document, Menu::Edit),
                    Command::menu("View…", Icon::Sidebar, Menu::Navigation),
                    Command::menu("Writing preferences…", Icon::Settings, Menu::Preferences),
                    Command::menu("Help & settings…", Icon::Settings, Menu::Help),
                ]
            }
            Menu::Edit => [
                ("Undo", crate::editor::menu_input::Command::Undo, Icon::Undo),
                ("Redo", crate::editor::menu_input::Command::Redo, Icon::Redo),
                (
                    "Cut",
                    crate::editor::menu_input::Command::Cut,
                    Icon::Document,
                ),
                (
                    "Copy",
                    crate::editor::menu_input::Command::Copy,
                    Icon::Document,
                ),
                (
                    "Paste",
                    crate::editor::menu_input::Command::Paste,
                    Icon::Document,
                ),
                (
                    "Select all",
                    crate::editor::menu_input::Command::SelectAll,
                    Icon::Document,
                ),
            ]
            .into_iter()
            .map(|(label, command, icon)| {
                Command::action(label, icon, AppMessage::MenuEdit(command), !self.loading)
            })
            .collect(),
            Menu::Document => {
                let mut commands = vec![
                    Command::action("Save", Icon::Save, AppMessage::Save, ready),
                    Command::action("Save as…", Icon::Save, AppMessage::SaveAs, ready),
                ];
                if let Some(path) = &self.path {
                    commands.extend([
                        Command::action(
                            "Rename…",
                            Icon::Document,
                            AppMessage::Folder(folder::Message::Rename(path.clone())),
                            ready,
                        ),
                        Command::action(
                            "Move to…",
                            Icon::Folder,
                            AppMessage::Folder(folder::Message::MoveTo(path.clone())),
                            ready,
                        ),
                        Command::action(
                            "Show location",
                            Icon::Folder,
                            AppMessage::Folder(folder::Message::ShowLocation(path.clone())),
                            true,
                        ),
                    ]);
                }
                if self.folder.can_undo() {
                    commands.push(Command::action(
                        "Undo file move",
                        Icon::Undo,
                        AppMessage::Folder(folder::Message::UndoMove),
                        ready,
                    ));
                }
                if self.footnotes.can_return() {
                    commands.push(Command::action(
                        "Back to reference",
                        Icon::Link,
                        AppMessage::Footnote(super::footnotes::Message::Back),
                        true,
                    ));
                }
                commands.extend(
                    [
                        ("Write", Icon::Document, EditorMode::Live),
                        ("Source", Icon::Code, EditorMode::Source),
                        ("Read", Icon::Book, EditorMode::Read),
                    ]
                    .into_iter()
                    .map(|(label, icon, mode)| {
                        Command::action(
                            label,
                            if self.mode == mode { Icon::Check } else { icon },
                            AppMessage::SetMode(mode),
                            !self.loading,
                        )
                    }),
                );
                commands
            }
            Menu::Navigation => vec![
                Command::action(
                    "Files",
                    Icon::Folder,
                    AppMessage::ToggleSidebar(Sidebar::Files),
                    self.folder.root().is_some(),
                ),
                Command::action(
                    "Outline",
                    Icon::Outline,
                    AppMessage::ToggleSidebar(Sidebar::Outline),
                    true,
                ),
                Command::action(
                    "Filter headings…",
                    Icon::Search,
                    AppMessage::Outline(super::outline::Message::Filter),
                    !self.outline.sections.is_empty(),
                ),
                Command::action(
                    if self.chrome.sidebars_hidden {
                        "Show sidebars"
                    } else {
                        "Hide sidebars"
                    },
                    Icon::Sidebar,
                    AppMessage::HideSidebars,
                    self.chrome.files.open || self.chrome.outline.open,
                ),
            ],
            Menu::Workspace => {
                let mut commands = vec![Command::action(
                    if self.folder.root().is_some() {
                        "Switch folder…"
                    } else {
                        "Open folder…"
                    },
                    Icon::Folder,
                    AppMessage::Folder(folder::Message::Pick),
                    ready,
                )];
                if let Some(root) = self.folder.root() {
                    commands.extend([
                        Command::action(
                            "Show location",
                            Icon::Folder,
                            AppMessage::Folder(folder::Message::ShowLocation(root.to_owned())),
                            ready,
                        ),
                        Command::action(
                            if cfg!(target_os = "macos") {
                                "Show in Finder"
                            } else {
                                "Show in file manager"
                            },
                            Icon::Folder,
                            AppMessage::Folder(folder::Message::Reveal(root.to_owned())),
                            ready,
                        ),
                        Command::action(
                            "New document…",
                            Icon::Plus,
                            AppMessage::Folder(folder::Message::NewDocument(root.to_owned())),
                            ready,
                        ),
                        Command::action(
                            "New folder…",
                            Icon::Folder,
                            AppMessage::Folder(folder::Message::NewFolder(root.to_owned())),
                            ready,
                        ),
                        Command::action(
                            "Refresh folder",
                            Icon::Redo,
                            AppMessage::Folder(folder::Message::Refresh),
                            ready,
                        ),
                        Command::action(
                            "Close folder",
                            Icon::Close,
                            AppMessage::Folder(folder::Message::Close),
                            ready,
                        ),
                    ]);
                }
                commands
            }
            Menu::Preferences => {
                let mut commands = crate::theme::WritingFont::ALL
                    .into_iter()
                    .map(|font| {
                        Command::action(
                            match font {
                                crate::theme::WritingFont::SansSerif => "Sans-serif",
                                crate::theme::WritingFont::Serif => "Serif",
                            },
                            if self.preferences.writing_font == font {
                                Icon::Check
                            } else {
                                Icon::Unsaved
                            },
                            AppMessage::WritingFont(font),
                            true,
                        )
                    })
                    .collect::<Vec<_>>();
                commands.extend(
                    [
                        (
                            "Word count",
                            self.preferences.show_word_count,
                            Preference::WordCount,
                        ),
                        (
                            "Reading time",
                            self.preferences.show_reading_time,
                            Preference::ReadingTime,
                        ),
                    ]
                    .into_iter()
                    .map(|(label, enabled, preference)| {
                        Command::action(
                            label,
                            if enabled { Icon::Check } else { Icon::Unsaved },
                            AppMessage::Chrome(Message::Preference(preference, !enabled)),
                            true,
                        )
                    })
                    .collect::<Vec<_>>(),
                );
                commands
            }
            Menu::Format => {
                let mut commands = Vec::new();
                for (label, icon, format) in [
                    ("Heading", Icon::Heading, Format::Heading),
                    ("Bold", Icon::Bold, Format::Bold),
                    ("Italic", Icon::Italic, Format::Italic),
                    ("Strikethrough", Icon::Strikethrough, Format::Strikethrough),
                    ("Inline code", Icon::Code, Format::Code),
                    ("Link…", Icon::Link, Format::Link),
                    ("Quotation…", Icon::Quote, Format::Quote),
                    ("List", Icon::List, Format::List),
                    ("Task", Icon::Task, Format::Task),
                    ("Code block", Icon::Fence, Format::Fence),
                    ("Table", Icon::Table, Format::Table),
                ] {
                    let action = if matches!(format, Format::Quote) {
                        AppMessage::Writing(writing::Message::Details(writing::Kind::Quote))
                    } else {
                        AppMessage::Format(format)
                    };
                    commands.push(Command::action(
                        label,
                        icon,
                        action,
                        !self.loading && self.mode != EditorMode::Read,
                    ));
                }
                commands.push(Command::action(
                    "Image…",
                    Icon::Image,
                    AppMessage::Writing(writing::Message::Details(writing::Kind::Image)),
                    !self.loading && self.mode != EditorMode::Read,
                ));
                commands
            }
            Menu::Help => vec![
                Command::action(
                    "Appearance & shortcuts…",
                    Icon::Settings,
                    AppMessage::ShowSettings,
                    true,
                ),
                Command::action("Welcome", Icon::Book, AppMessage::Welcome, true),
                Command::action("Markdown guide", Icon::Book, AppMessage::Guide, true),
                Command::action(
                    "The quiet hour · sample",
                    Icon::Document,
                    AppMessage::Writing(writing::Message::Sample),
                    ready,
                ),
                Command::action(
                    "Recovery copies…",
                    Icon::Undo,
                    AppMessage::Recovery(recovery::Message::Show),
                    true,
                ),
                Command::action(
                    "Collaboration…",
                    Icon::Document,
                    AppMessage::ShowCollaboration,
                    ready,
                ),
            ],
        }
    }

    pub(super) fn chrome_menu_view(&self, size: Size) -> Element<'_, AppMessage> {
        let menu = self.chrome.menu.unwrap_or(Menu::File);
        let tokens = self.resolved_theme.tokens;
        let mut items = column![].spacing(2);
        if menu == Menu::Help {
            items = items.push(container(brand::view(tokens)).padding([8, 12]));
        }
        let mut enabled_index = 0;
        for (index, command) in self.chrome_commands(menu).into_iter().enumerate() {
            let selected = command.enabled && enabled_index == self.chrome.selected;
            let choice_index = enabled_index;
            enabled_index += usize::from(command.enabled);
            items = items.push(
                container(
                    mouse_area(
                        button(
                            row![
                                icon::view(
                                    command.icon,
                                    if command.enabled {
                                        tokens.muted
                                    } else {
                                        tokens.disabled
                                    }
                                ),
                                text(command.label).size(13)
                            ]
                            .spacing(10)
                            .align_y(Alignment::Center),
                        )
                        .on_press_maybe(
                            command
                                .enabled
                                .then_some(AppMessage::Chrome(command.action)),
                        )
                        .padding([9, 12])
                        .width(Fill)
                        .style(styles::button_style(
                            tokens,
                            if selected {
                                Button::Selected
                            } else {
                                Button::Quiet
                            },
                        )),
                    )
                    .on_enter(AppMessage::Chrome(Message::Hover(
                        if command.enabled {
                            choice_index
                        } else {
                            self.chrome.selected
                        },
                    ))),
                )
                .id(menu_item_id(index))
                .width(Fill),
            );
        }
        let width = 280.0_f32.min((size.width - 24.0).max(120.0));
        let top = if menu == Menu::Workspace {
            TITLEBAR_HEIGHT
                + crate::theme::styles::SIDEBAR_INSET
                + crate::theme::styles::SIDEBAR_ROW_HEIGHT
                + 8.0
        } else {
            TITLEBAR_HEIGHT + 8.0
        };
        let popup = container(
            scrollable(items)
                .id(MENU_SCROLL)
                .height(iced::Shrink)
                .style(styles::scrollbar(tokens)),
        )
        .padding(6)
        .width(width)
        .max_height((size.height - top - 8.0).max(80.0))
        .style(styles::context_menu(tokens));
        let left = match menu {
            Menu::Format => (size.width - width - 16.0).max(8.0),
            Menu::Document => ((size.width - width) / 2.0).max(8.0),
            Menu::Workspace => crate::theme::styles::SIDEBAR_INSET,
            _ => 16.0,
        };
        container(popup)
            .align_top(Fill)
            .align_left(Fill)
            .padding(Padding::new(0.0).left(left).top(top))
            .into()
    }
}

#[cfg(test)]
#[path = "chrome_tests.rs"]
mod tests;
