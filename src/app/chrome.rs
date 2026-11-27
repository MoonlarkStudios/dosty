//! Native quiet titlebar, independent sidebars, and document commands.

use std::borrow::Cow;

use iced::advanced::widget;
use iced::keyboard::{self, Key, key};
use iced::widget::{
    button, column, container, mouse_area, operation, row, scrollable, space, text, tooltip,
};
use iced::{Alignment, Element, Event, Fill, Padding, Size, Task, window};

use super::trail::Direction;
use super::{
    App, EDITOR, EditorMode, Format, Message as AppMessage, Overlay, Sidebar, folder, projects,
    recovery, writing,
};
use crate::app::brand;
use crate::app::icon::{self, Icon};
use crate::editor::menu_input;
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
pub(super) const INITIAL_WINDOW_WIDTH: f32 = 1180.0;

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
    Mode,
    Format,
    Help,
    Navigation,
    Preferences,
    Projects,
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
    WindowOpened(window::Id, f32),
    WindowWidth(f32),
    WindowScale(f32),
    Preference(Preference, bool),
    Statistics,
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
    pub(super) window_width: f32,
    pub(super) window_scale: f32,
    last_sidebar: Sidebar,
    pub saving: bool,
    pub save_error: Option<String>,
    selected: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Panel {
    pub open: bool,
}

impl State {
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
            window_width: INITIAL_WINDOW_WIDTH,
            window_scale: 1.0,
            last_sidebar: Sidebar::Files,
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
    label: Cow<'static, str>,
    group: Option<&'static str>,
    icon: Icon,
    action: Message,
    enabled: bool,
}

impl Command {
    fn action(
        label: impl Into<Cow<'static, str>>,
        icon: Icon,
        action: AppMessage,
        enabled: bool,
    ) -> Self {
        Self {
            label: label.into(),
            group: None,
            icon,
            action: Message::Run(Box::new(action)),
            enabled,
        }
    }
    fn menu(label: &'static str, icon: Icon, menu: Menu) -> Self {
        Self {
            label: label.into(),
            group: None,
            icon,
            action: Message::Menu(menu),
            enabled: true,
        }
    }

    fn shortcut(&self) -> Option<&'static str> {
        match &self.action {
            Message::Run(action) => shortcut_label(action),
            _ => None,
        }
    }
}

pub(super) const fn platform_keys(mac: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else {
        other
    }
}

pub(super) fn shortcut_label(action: &AppMessage) -> Option<&'static str> {
    let (mac, other) = match action {
        AppMessage::New => ("⌘N", "Ctrl+N"),
        AppMessage::Open => ("⌘O", "Ctrl+O"),
        AppMessage::Format(Format::Bold) => ("⌘B", "Ctrl+B"),
        AppMessage::Format(Format::Italic) => ("⌘I", "Ctrl+I"),
        AppMessage::Format(Format::Strikethrough) => ("⇧⌘X", "Ctrl+Shift+X"),
        AppMessage::Format(Format::Link) => ("⌘K", "Ctrl+K"),
        AppMessage::Save => ("⌘S", "Ctrl+S"),
        AppMessage::SaveAs => ("⇧⌘S", "Ctrl+Shift+S"),
        AppMessage::QuickOpen(super::quick_open::Message::Open) => ("⌘P", "Ctrl+P"),
        AppMessage::Hosted(super::hosted_workspace::Message::Show) => ("⇧⌘P", "Ctrl+Shift+P"),
        AppMessage::Step(Direction::Back) => ("⌘[", "Ctrl+["),
        AppMessage::Step(Direction::Forward) => ("⌘]", "Ctrl+]"),
        AppMessage::ShowSettings => ("⌘,", "Ctrl+,"),
        AppMessage::ToggleSidebar(Sidebar::Files) => ("⇧⌘B", "Ctrl+Shift+B"),
        AppMessage::ToggleSidebar(Sidebar::Outline) => ("⌥⌘O", "Ctrl+Alt+O"),
        AppMessage::FolderSearch(super::folder_search::Message::Open) => ("⌥⌘F", "Ctrl+Alt+F"),
        AppMessage::Outline(super::outline::Message::Filter) => ("⇧⌘J", "Ctrl+Shift+J"),
        AppMessage::ReturnToDocument => ("Esc", "Esc"),
        _ => return None,
    };
    Some(platform_keys(mac, other))
}

pub(super) fn control_hint<'a>(label: &'a str, action: &AppMessage) -> Cow<'a, str> {
    shortcut_label(action).map_or(Cow::Borrowed(label), |keys| {
        Cow::Owned(format!("{label} · {keys}"))
    })
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

fn word_label(words: usize) -> String {
    format!("{words} {}", if words == 1 { "word" } else { "words" })
}

fn reading_time(words: usize) -> String {
    if words == 0 {
        "0 min read".into()
    } else if words < 200 {
        "<1 min read".into()
    } else {
        format!("~{} min read", words.div_ceil(200))
    }
}

impl App {
    pub(super) fn show_sidebar(&mut self, side: Sidebar) -> Task<AppMessage> {
        self.chrome.sidebars_hidden = false;
        self.chrome.panel_mut(side).open = true;
        self.chrome.last_sidebar = side;
        self.preserve_viewport_generation =
            Some(self.session.document.collaboration().generation());
        // Without an open place the sidebar has no tree to take the keyboard.
        if side == Sidebar::Files && self.folder.root().is_some() {
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
        let (files, outline) = self.docked_sidebars(self.chrome.window_width);
        let visible = match side {
            Sidebar::Files => files,
            Sidebar::Outline => outline,
        };
        if !visible {
            return self.show_sidebar(side);
        }
        self.chrome.panel_mut(side).open = false;
        self.preserve_viewport_generation =
            Some(self.session.document.collaboration().generation());
        operation::focus(EDITOR)
    }

    pub(super) fn docked_sidebars(&self, width: f32) -> (bool, bool) {
        if self.chrome.sidebars_hidden {
            return (false, false);
        }
        let files = self.chrome.files.open && width >= FILES_WIDTH + MIN_WRITING_WIDTH;
        let outline = self.chrome.outline.open && width >= OUTLINE_WIDTH + MIN_WRITING_WIDTH;
        if files && outline && width < FILES_WIDTH + OUTLINE_WIDTH + MIN_WRITING_WIDTH {
            return match self.chrome.last_sidebar {
                Sidebar::Files => (true, false),
                Sidebar::Outline => (false, true),
            };
        }
        (files, outline)
    }

    pub(super) fn workspace_view(&self, size: Size) -> Element<'_, AppMessage> {
        if self.is_home() {
            return self.home_view(size);
        }
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
        column![self.titlebar_view(), writing].height(Fill).into()
    }

    pub(super) fn update_chrome(&mut self, message: Message) -> Task<AppMessage> {
        match message {
            Message::DragWindow => return window::latest().and_then(window::drag),
            Message::MaximizeWindow => return window::latest().and_then(window::toggle_maximize),
            Message::WindowOpened(id, width) => {
                return Task::batch([
                    self.update_chrome(Message::WindowWidth(width)),
                    window::scale_factor(id)
                        .map(|scale| AppMessage::Chrome(Message::WindowScale(scale))),
                ]);
            }
            Message::WindowScale(scale) => self.chrome.window_scale = scale,
            Message::WindowWidth(width) => {
                let (was_files, was_outline) = self.docked_sidebars(self.chrome.window_width);
                self.chrome.window_width = width;
                let (files, outline) = self.docked_sidebars(width);
                if (files, outline) != (was_files, was_outline) {
                    self.preserve_viewport_generation =
                        Some(self.session.document.collaboration().generation());
                    if (was_files && !files || was_outline && !outline)
                        && !self.editor_focused
                        && self.overlay.is_none()
                        && !self.search_open
                        && self.mode != EditorMode::Read
                    {
                        return operation::focus(EDITOR);
                    }
                }
            }
            Message::Preference(preference, enabled) => {
                match preference {
                    Preference::WordCount => self.preferences.show_word_count = enabled,
                    Preference::ReadingTime => self.preferences.show_reading_time = enabled,
                }
                return self.persist_preferences();
            }
            Message::Statistics => {
                if self.overlay == Some(Overlay::Statistics) {
                    self.overlay = None;
                } else if self.overlay.is_none() && !self.busy && !self.loading {
                    self.overlay = Some(Overlay::Statistics);
                }
                return Task::none();
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
                            | Menu::Projects
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
        self.preserve_viewport_generation =
            Some(self.session.document.collaboration().generation());
        operation::focus(EDITOR)
    }

    pub(super) fn reveal_navigation(&mut self) {
        self.chrome.sidebars_hidden = false;
        self.chrome.files.open = true;
        self.preserve_viewport_generation =
            Some(self.session.document.collaboration().generation());
    }

    /// Whether an Edit command can act on the widget that will receive it.
    ///
    /// Commands are delivered to the focused widget. Dialog fields, Find, and
    /// table drafts own their text history and clipboard, so only the document
    /// consults its undo stacks. The command menu itself closes before delivery
    /// and therefore does not count as a receiving field.
    pub(super) fn edit_command_enabled(&self, command: menu_input::Command) -> bool {
        let field_focused = !matches!(self.overlay, None | Some(Overlay::Chrome))
            || self.search_open
            || self.grid.draft.is_some();
        !self.loading
            && (field_focused
                || !self.is_home()
                    && match command {
                        menu_input::Command::Undo => self.session.document.can_undo(),
                        menu_input::Command::Redo => self.session.document.can_redo(),
                        _ => self.mode != EditorMode::Read,
                    })
    }

    fn save_state(&self) -> SaveState {
        if self.chrome.saving {
            SaveState::Saving
        } else if self.chrome.save_error.is_some() {
            SaveState::Failed
        } else if self.session.is_unbound() {
            SaveState::Unsaved
        } else if self.session.is_dirty() {
            SaveState::Modified
        } else {
            SaveState::Saved
        }
    }

    fn writing_statistics(&self) -> String {
        let words = self.session.document.words();
        let mut parts = Vec::with_capacity(2);
        if self.preferences.show_word_count {
            parts.push(
                if self.mode != EditorMode::Read && !self.session.selection.is_caret() {
                    format!(
                        "{} selected",
                        word_label(self.session.document.selected_words(self.session.selection))
                    )
                } else {
                    word_label(words)
                },
            );
        }
        if self.preferences.show_reading_time {
            parts.push(reading_time(words));
        }
        parts.join(" · ")
    }

    pub(super) fn statistics_view(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let label = self.writing_statistics();
        let control: Element<'_, AppMessage> = if label.is_empty() {
            space().into()
        } else {
            tooltip(
                button(text(label).size(11))
                    .on_press_maybe(
                        (!self.loading && !self.busy)
                            .then_some(AppMessage::Chrome(Message::Statistics)),
                    )
                    .padding([3, 8])
                    .style(styles::button_style(tokens, Button::Quiet)),
                container(text("Document statistics").size(11))
                    .padding(5)
                    .style(styles::context_menu(tokens)),
                tooltip::Position::Top,
            )
            .into()
        };
        // A fixed gutter preserves the writing viewport as the label changes.
        container(control)
            .center_x(Fill)
            .center_y(STATISTICS_HEIGHT)
            .id("writing-statistics")
            .into()
    }

    pub(super) fn statistics_popover(&self, size: Size) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let words = self.session.document.words();
        let characters = self.session.document.source().chars().count();
        let close = self.titlebar_icon(
            Icon::Close,
            "Close statistics",
            AppMessage::CloseOverlay,
            true,
            false,
            None,
        );
        let metric = |label: &'static str, value: String| {
            row![
                text(label).size(12).color(tokens.muted),
                space::horizontal(),
                text(value).size(13).color(tokens.text),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
        };
        let details = column![
            row![
                text("Document statistics").size(14),
                space().width(Fill),
                close
            ]
            .align_y(Alignment::Center),
            metric("Words", words.to_string()),
            metric("Markdown characters", characters.to_string()),
            metric(
                "Estimated reading time",
                reading_time(words).trim_end_matches(" read").to_owned(),
            ),
        ]
        .spacing(8);
        let popup = container(details)
            .padding(12)
            .width(264.0_f32.min((size.width - 16.0).max(1.0)))
            .style(styles::context_menu(tokens));
        let (files, outline) = self.docked_sidebars(size.width);
        row![
            space().width(if files { FILES_WIDTH } else { 0.0 }),
            container(popup)
                .center_x(Fill)
                .align_bottom(Fill)
                .padding(Padding::new(0.0).bottom(STATISTICS_HEIGHT + 6.0)),
            space().width(if outline { OUTLINE_WIDTH } else { 0.0 }),
        ]
        .height(Fill)
        .into()
    }

    fn titlebar_icon<'a>(
        &self,
        kind: Icon,
        label: &'a str,
        message: AppMessage,
        enabled: bool,
        selected: bool,
        caption: Option<&'static str>,
    ) -> Element<'a, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let color = if !enabled {
            tokens.disabled
        } else if selected {
            tokens.on_accent_soft
        } else {
            tokens.muted
        };
        let mut content = row![icon::view(kind, color)]
            .spacing(6)
            .align_y(Alignment::Center);
        if let Some(caption) = caption {
            content = content.push(text(caption).size(13).wrapping(text::Wrapping::None));
        }
        let hint = control_hint(label, &message);
        styles::hint(
            tokens,
            button(content)
                .on_press_maybe(enabled.then_some(message))
                .padding([5, 6])
                .style(styles::button_style(
                    tokens,
                    if selected {
                        Button::Selected
                    } else {
                        Button::Quiet
                    },
                )),
            hint,
            tooltip::Position::Bottom,
        )
        .into()
    }

    pub(super) fn titlebar_view(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        iced::widget::responsive(move |size| {
            let (files_visible, outline_visible) = self.docked_sidebars(size.width);
            let wide = size.width >= 960.0;
            let side_width = if wide { 324.0 } else { 244.0 };
            let ready = !self.busy && !self.loading;
            // Leave the native macOS window buttons and their hit targets clear.
            let native_inset = if cfg!(target_os = "macos") { 76.0 } else { 0.0 };
            let mut left = row![space().width(native_inset)];
            if !cfg!(target_os = "macos") {
                left = left.push(self.small_button(
                    "File",
                    AppMessage::Chrome(Message::Menu(Menu::File)),
                    self.chrome.menu == Some(Menu::File),
                ));
            }
            let files = container(self.titlebar_icon(
                Icon::Sidebar,
                if files_visible {
                    "Hide Files"
                } else {
                    "Show Files"
                },
                AppMessage::ToggleSidebar(Sidebar::Files),
                ready,
                files_visible,
                None,
            ))
            .id("titlebar-files");
            let quick_open = container(self.titlebar_icon(
                Icon::Search,
                "Quick Open",
                AppMessage::QuickOpen(super::quick_open::Message::Open),
                ready,
                self.overlay == Some(Overlay::QuickOpen),
                wide.then_some("Quick Open"),
            ))
            .id("titlebar-quick-open");
            let left = left
                .push(files)
                .push(self.titlebar_icon(
                    Icon::Home,
                    "Welcome",
                    AppMessage::ShowWelcome,
                    ready,
                    false,
                    None,
                ))
                .push(self.titlebar_icon(
                    Icon::Back,
                    "Previous document",
                    AppMessage::Step(Direction::Back),
                    ready && self.trail.can_step(Direction::Back),
                    false,
                    None,
                ))
                .push(self.titlebar_icon(
                    Icon::Forward,
                    "Next document",
                    AppMessage::Step(Direction::Forward),
                    ready && self.trail.can_step(Direction::Forward),
                    false,
                    None,
                ))
                .push(quick_open)
                .spacing(3)
                .align_y(Alignment::Center)
                .width(side_width);
            let name = styles::hint(
                tokens,
                button(
                    row![
                        text(&self.session.display_name)
                            .font(iced::Font {
                                weight: iced::font::Weight::Bold,
                                ..styles::BODY_FONT
                            })
                            .size(13)
                            .wrapping(text::Wrapping::None),
                        icon::sized(Icon::ChevronDown, tokens.muted, 10.0),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .padding([5, 6])
                .on_press_maybe(ready.then_some(if self.session.hosted().is_some() {
                    AppMessage::Hosted(super::hosted_workspace::Message::Show)
                } else {
                    AppMessage::Chrome(Message::Menu(Menu::Document))
                }))
                .style(styles::button_style(tokens, Button::Quiet)),
                self.session.path().map_or_else(
                    || "Document actions".into(),
                    |path| format!("Document actions · {}", path.display()),
                ),
                tooltip::Position::Bottom,
            );
            let (status_icon, status_label, caption, status_color) = match self.save_state() {
                SaveState::Unsaved => (
                    Icon::Unsaved,
                    "Not saved yet · Click to save".to_owned(),
                    Some("New draft"),
                    tokens.accent,
                ),
                SaveState::Modified => (
                    Icon::Modified,
                    "Unsaved changes · Click to save".to_owned(),
                    Some("Unsaved"),
                    tokens.accent,
                ),
                SaveState::Saved => (
                    Icon::Check,
                    "All changes saved".to_owned(),
                    None,
                    tokens.muted,
                ),
                SaveState::Saving => (
                    Icon::Save,
                    "Saving…".to_owned(),
                    Some("Saving…"),
                    tokens.muted,
                ),
                SaveState::Failed => (
                    Icon::Warning,
                    format!(
                        "{} · Click to retry",
                        self.chrome
                            .save_error
                            .as_deref()
                            .unwrap_or("Try saving again")
                    ),
                    Some("Save failed"),
                    tokens.danger,
                ),
            };
            let mut status_content = row![icon::view(status_icon, status_color)]
                .spacing(4)
                .align_y(Alignment::Center);
            if let Some(caption) = caption {
                status_content = status_content.push(text(caption).size(11).color(status_color));
            }
            let status = styles::hint(
                tokens,
                button(status_content)
                    .on_press_maybe((!self.busy).then_some(
                        if self.save_state() == SaveState::Saved {
                            AppMessage::Chrome(Message::Menu(Menu::Document))
                        } else {
                            AppMessage::Save
                        },
                    ))
                    .padding(6)
                    .style(styles::button_style(tokens, Button::Quiet)),
                status_label,
                tooltip::Position::Bottom,
            );
            let title = container(
                row![
                    container(name)
                        .max_width(
                            (size.width
                                - side_width * 2.0
                                - if caption.is_some() { 146.0 } else { 74.0 })
                            .max(48.0)
                        )
                        .clip(true)
                        .id("titlebar-name"),
                    container(status).id("titlebar-save-state"),
                ]
                .spacing(2)
                .align_y(Alignment::Center),
            )
            .center_x(Fill);
            let (mode_icon, mode_label) = match self.mode {
                EditorMode::Live => (Icon::Edit, "Write"),
                EditorMode::Source => (Icon::Code, "Source"),
                EditorMode::Read => (Icon::Book, "Read"),
            };
            let mode_open = self.chrome.menu == Some(Menu::Mode);
            let mode = styles::hint(
                tokens,
                button(
                    row![
                        icon::sized(mode_icon, tokens.accent, 14.0),
                        text(mode_label).size(13),
                        icon::sized(Icon::ChevronDown, tokens.muted, 10.0),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .padding([5, 8])
                .on_press_maybe(ready.then_some(AppMessage::Chrome(Message::Menu(Menu::Mode))))
                .style(styles::button_style(
                    tokens,
                    if mode_open {
                        Button::Selected
                    } else {
                        Button::Quiet
                    },
                )),
                "Change editing mode",
                tooltip::Position::Bottom,
            );
            let right = row![
                space::horizontal(),
                container(mode).id("titlebar-mode"),
                container(self.titlebar_icon(
                    Icon::Bold,
                    if self.mode == EditorMode::Read {
                        "Switch to Write or Source to format"
                    } else {
                        "Format document"
                    },
                    AppMessage::Chrome(Message::Menu(Menu::Format)),
                    ready && self.mode != EditorMode::Read,
                    self.chrome.menu == Some(Menu::Format),
                    Some("Format"),
                ))
                .id("titlebar-format"),
                container(self.titlebar_icon(
                    Icon::Outline,
                    if outline_visible {
                        "Hide Outline"
                    } else {
                        "Show Outline"
                    },
                    AppMessage::ToggleSidebar(Sidebar::Outline),
                    ready,
                    outline_visible,
                    None,
                ))
                .id("titlebar-outline"),
                self.titlebar_icon(
                    Icon::Settings,
                    "Settings",
                    AppMessage::ShowSettings,
                    !self.busy,
                    self.overlay == Some(Overlay::Settings),
                    None,
                ),
            ]
            .spacing(4)
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

    pub(super) fn home_titlebar(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let content: Element<'_, AppMessage> = if cfg!(target_os = "macos") {
            space().into()
        } else {
            self.small_button("File", AppMessage::Chrome(Message::Menu(Menu::File)), false)
        };
        mouse_area(
            container(
                row![
                    content,
                    space::horizontal(),
                    self.titlebar_icon(
                        Icon::Folder,
                        "Hosted workspace",
                        AppMessage::Hosted(super::hosted_workspace::Message::Show),
                        !self.busy,
                        self.overlay == Some(Overlay::Hosted),
                        Some("Hosted workspace"),
                    ),
                    self.titlebar_icon(
                        Icon::Settings,
                        "Settings",
                        AppMessage::ShowSettings,
                        !self.busy,
                        self.overlay == Some(Overlay::Settings),
                        None,
                    )
                ]
                .align_y(Alignment::Center),
            )
            .padding([0, 12])
            .center_y(TITLEBAR_HEIGHT)
            .width(Fill)
            .style(styles::panel(tokens.surface)),
        )
        .on_press(AppMessage::Chrome(Message::DragWindow))
        .on_double_click(AppMessage::Chrome(Message::MaximizeWindow))
        .into()
    }

    pub(super) fn context_actions(&self) -> Option<Element<'_, AppMessage>> {
        if self.is_home() || self.chrome.sidebars_hidden {
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
                let mut commands = vec![
                    Command::action("New document", Icon::Plus, AppMessage::New, ready),
                    Command::action(
                        "Open Markdown File…",
                        Icon::Document,
                        AppMessage::Open,
                        ready,
                    ),
                    Command::action(
                        "Quick Open…",
                        Icon::Search,
                        AppMessage::QuickOpen(super::quick_open::Message::Open),
                        ready,
                    ),
                    Command::action(
                        "Back",
                        Icon::Back,
                        AppMessage::Step(Direction::Back),
                        ready && self.trail.can_step(Direction::Back),
                    ),
                    Command::action(
                        "Forward",
                        Icon::Forward,
                        AppMessage::Step(Direction::Forward),
                        ready && self.trail.can_step(Direction::Forward),
                    ),
                    Command::action(
                        "Open Project…",
                        Icon::Folder,
                        AppMessage::Folder(folder::Message::Pick),
                        ready,
                    ),
                    Command::action(
                        "New Project…",
                        Icon::Plus,
                        AppMessage::Project(projects::Message::New),
                        ready,
                    ),
                    Command::menu("Projects…", Icon::Folder, Menu::Projects),
                ];
                if !self.is_home() {
                    commands.extend([
                        Command::action("Welcome", Icon::Home, AppMessage::ShowWelcome, ready),
                        Command::action("Save as…", Icon::Save, AppMessage::SaveAs, ready),
                        Command::menu("Document actions…", Icon::Document, Menu::Document),
                        Command::action("Export HTML…", Icon::Export, AppMessage::Export, ready),
                        Command::action(
                            "Find & replace…",
                            Icon::Search,
                            AppMessage::ToggleSearch,
                            true,
                        ),
                        Command::action(
                            "Find in folder…",
                            Icon::Search,
                            AppMessage::FolderSearch(super::folder_search::Message::Open),
                            ready && self.folder.root().is_some(),
                        ),
                        Command::menu("Formatting…", Icon::Heading, Menu::Format),
                        Command::menu("Edit…", Icon::Document, Menu::Edit),
                        Command::menu("View…", Icon::Sidebar, Menu::Navigation),
                        Command::menu("Writing preferences…", Icon::Settings, Menu::Preferences),
                    ]);
                }
                commands.push(Command::menu(
                    "Help & settings…",
                    Icon::Settings,
                    Menu::Help,
                ));
                commands
            }
            Menu::Edit => [
                ("Undo", menu_input::Command::Undo, Icon::Undo),
                ("Redo", menu_input::Command::Redo, Icon::Redo),
                ("Cut", menu_input::Command::Cut, Icon::Document),
                ("Copy", menu_input::Command::Copy, Icon::Document),
                ("Paste", menu_input::Command::Paste, Icon::Document),
                ("Select all", menu_input::Command::SelectAll, Icon::Document),
            ]
            .into_iter()
            .map(|(label, command, icon)| {
                Command::action(
                    label,
                    icon,
                    AppMessage::MenuEdit(command),
                    self.edit_command_enabled(command),
                )
            })
            .collect(),
            Menu::Document => {
                let mut commands = vec![
                    Command::action("Save", Icon::Save, AppMessage::Save, ready),
                    Command::action("Save as…", Icon::Save, AppMessage::SaveAs, ready),
                ];
                if self.session.is_unbound() {
                    commands.push(Command::action(
                        "Name…",
                        Icon::Document,
                        AppMessage::Folder(folder::Message::NameDraft),
                        ready,
                    ));
                }
                if let Some(path) = self.session.path() {
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
                if self.backlinks_available() {
                    commands.push(Command::action(
                        "Linked from…",
                        Icon::Link,
                        AppMessage::Links(super::links::Message::ShowBacklinks),
                        ready,
                    ));
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
                commands
            }
            Menu::Mode => [
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
                    ready,
                )
            })
            .collect(),
            Menu::Navigation => vec![
                Command::menu("Editing mode…", Icon::Document, Menu::Mode),
                Command::action(
                    "Files",
                    Icon::Folder,
                    AppMessage::ToggleSidebar(Sidebar::Files),
                    true,
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
                    "Toggle section fold",
                    Icon::Heading,
                    AppMessage::ToggleSectionFold(None),
                    ready && self.mode == EditorMode::Live && self.current_section_fold().is_some(),
                ),
                Command::action(
                    "Document statistics…",
                    Icon::Document,
                    AppMessage::Chrome(Message::Statistics),
                    ready && !self.loading,
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
            Menu::Projects => {
                let mut commands: Vec<_> = self
                    .workspace
                    .recent_projects()
                    .map(|(index, root)| {
                        Command::action(
                            self.workspace.project_label(root),
                            if self.folder.root() == Some(root) {
                                Icon::Check
                            } else {
                                Icon::Folder
                            },
                            AppMessage::Workspace(super::workspace::Message::OpenPlace(index)),
                            ready,
                        )
                    })
                    .collect();
                commands.extend([
                    Command::action(
                        "Hosted Workspace…",
                        Icon::Folder,
                        AppMessage::Hosted(super::hosted_workspace::Message::Show),
                        ready,
                    ),
                    Command::action(
                        "Open Project…",
                        Icon::Folder,
                        AppMessage::Folder(folder::Message::Pick),
                        ready,
                    ),
                    Command::action(
                        "New Project…",
                        Icon::Plus,
                        AppMessage::Project(projects::Message::New),
                        ready,
                    ),
                ]);
                commands
            }
            Menu::Workspace => {
                let mut commands = vec![Command::menu("Projects…", Icon::Folder, Menu::Projects)];
                if let Some(root) = self.folder.root() {
                    if !self.workspace.has_project_metadata() {
                        commands.push(Command::action(
                            "Set Up Project…",
                            Icon::Folder,
                            AppMessage::Project(projects::Message::SetUp(root.to_owned())),
                            ready,
                        ));
                    }
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
                            "Remove from Projects",
                            Icon::Close,
                            AppMessage::Folder(folder::Message::Remove),
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
                            font.label(),
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
                    let mut command = Command::action(
                        label,
                        icon,
                        action,
                        !self.loading && self.mode != EditorMode::Read,
                    );
                    command.group = match format {
                        Format::Heading => Some("Text"),
                        Format::Quote => Some("Blocks"),
                        _ => None,
                    };
                    commands.push(command);
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
                Command::action("Settings…", Icon::Settings, AppMessage::ShowSettings, true),
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
            if let Some(group) = command.group {
                items = items.push(
                    container(text(group).size(11).color(tokens.muted))
                        .padding(iced::Padding::new(10.0).top(6.0).bottom(2.0)),
                );
            }
            let shortcut = command.shortcut();
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
                                text(command.label).size(13),
                                space::horizontal(),
                                text(shortcut.unwrap_or_default()).size(11).color(
                                    if command.enabled {
                                        if selected {
                                            tokens.on_accent_soft
                                        } else {
                                            tokens.muted
                                        }
                                    } else {
                                        tokens.disabled
                                    },
                                ),
                            ]
                            .spacing(10)
                            .align_y(Alignment::Center),
                        )
                        .on_press_maybe(
                            command
                                .enabled
                                .then_some(AppMessage::Chrome(command.action)),
                        )
                        .padding([if menu == Menu::Format { 6 } else { 9 }, 12])
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
        let preferred_width: f32 = match menu {
            Menu::Mode => 144.0,
            Menu::Projects => 360.0,
            _ => 280.0,
        };
        let width = preferred_width.min((size.width - 24.0).max(120.0));
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
            Menu::Format => (size.width - width - 80.0).max(8.0),
            Menu::Mode => (size.width - width - 150.0).max(8.0),
            Menu::Document => ((size.width - width) / 2.0).max(8.0),
            Menu::Projects => ((size.width - width) / 2.0).max(8.0),
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
