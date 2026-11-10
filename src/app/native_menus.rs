//! Main-thread macOS menus. Commands return through the application's guarded paths.

use std::path::PathBuf;
use std::sync::Mutex;

use iced::futures::{StreamExt, channel::mpsc};
use iced::{Task, window};
use muda::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

use super::{
    App, EditorMode, Format, Message, Overlay, Sidebar, chrome, folder, recovery, workspace,
    writing,
};
use crate::editor::menu_input;
use crate::theme::WritingFont;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    New,
    Open,
    QuickOpen,
    OpenFolder,
    OpenRecent(usize),
    ClearRecent,
    Save,
    SaveAs,
    Rename,
    Move,
    Location,
    Export,
    Close,
    Edit(menu_input::Command),
    Find,
    Format(Format),
    Image,
    Files,
    Outline,
    FilterOutline,
    HideSidebars,
    Mode(EditorMode),
    Preference(chrome::Preference),
    WritingFont(WritingFont),
    Settings,
    Welcome,
    Guide,
    Sample,
    Recovery,
    UndoMove,
    BackToReference,
}

enum Item {
    Action(MenuItem),
    Check(CheckMenuItem),
}
impl Item {
    fn native(&self) -> &dyn IsMenuItem {
        match self {
            Self::Action(item) => item,
            Self::Check(item) => item,
        }
    }
    fn enabled(&self, enabled: bool) {
        match self {
            Self::Action(item) => item.set_enabled(enabled),
            Self::Check(item) => item.set_enabled(enabled),
        }
    }
}
struct Entry {
    command: Command,
    item: Item,
    enabled: bool,
}

pub(super) struct State {
    // Retain the root and all command handles for the entire application lifetime.
    root: Menu,
    window: Submenu,
    help: Submenu,
    entries: Vec<Entry>,
    hidden: bool,
    /// File → Open Recent is rebuilt from fixed command items whenever the
    /// remembered folders change, so menu identifiers stay stable.
    recent: Submenu,
    recent_items: Vec<MenuItem>,
    recent_clear: MenuItem,
    recent_empty: MenuItem,
    recent_separator: PredefinedMenuItem,
    recent_shown: Option<Vec<PathBuf>>,
}

impl State {
    fn add(
        &mut self,
        menu: &Submenu,
        label: &str,
        command: Command,
        shortcut: Option<&str>,
        checked: bool,
    ) -> muda::Result<()> {
        let id = format!("dosty.{command:?}");
        let accelerator = shortcut.map(|text| {
            text.parse()
                .expect("native menu shortcuts are static valid key sequences")
        });
        let item = if checked {
            let item = CheckMenuItem::with_id(id, label, true, false, None);
            item.set_key_accelerator(accelerator)?;
            Item::Check(item)
        } else {
            let item = MenuItem::with_id(id, label, true, None);
            item.set_key_accelerator(accelerator)?;
            Item::Action(item)
        };
        menu.append(item.native())?;
        self.entries.push(Entry {
            command,
            item,
            enabled: true,
        });
        Ok(())
    }

    fn new() -> muda::Result<(Self, Task<Message>)> {
        let root = Menu::new();
        let command_item = |command: Command, label: &str| {
            MenuItem::with_id(format!("dosty.{command:?}"), label, true, None)
        };
        let mut state = Self {
            root,
            window: Submenu::new("Window", true),
            help: Submenu::new("Help", true),
            entries: Vec::new(),
            hidden: false,
            recent: Submenu::new("Open Recent", true),
            recent_items: (0..crate::io::workspace_state::MAX_RECENT_WORKSPACES)
                .map(|index| command_item(Command::OpenRecent(index), ""))
                .collect(),
            recent_clear: command_item(Command::ClearRecent, "Clear Menu"),
            recent_empty: MenuItem::new("No Recent Folders", false, None),
            recent_separator: PredefinedMenuItem::separator(),
            recent_shown: None,
        };
        let recent_entries: Vec<_> = state
            .recent_items
            .iter()
            .enumerate()
            .map(|(index, item)| (Command::OpenRecent(index), item.clone()))
            .chain([(Command::ClearRecent, state.recent_clear.clone())])
            .collect();
        state
            .entries
            .extend(recent_entries.into_iter().map(|(command, item)| Entry {
                command,
                item: Item::Action(item),
                enabled: true,
            }));
        let application = Submenu::new("Dosty", true);
        application.append(&PredefinedMenuItem::about(
            Some("About Dosty"),
            Some(muda::AboutMetadata {
                name: Some("Dosty".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..Default::default()
            }),
        ))?;
        application.append(&PredefinedMenuItem::separator())?;
        state.add(
            &application,
            "Settings…",
            Command::Settings,
            Some("Meta+,"),
            false,
        )?;
        application.append(&PredefinedMenuItem::services(None))?;
        application.append(&PredefinedMenuItem::separator())?;
        application.append_items(&[
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
        ])?;
        application.append(&PredefinedMenuItem::separator())?;
        // A custom Quit action preserves Save/Discard/Cancel for this document.
        state.add(
            &application,
            "Quit Dosty",
            Command::Close,
            Some("Meta+q"),
            false,
        )?;
        state.root.append(&application)?;

        let file = Submenu::new("File", true);
        for (label, command, shortcut) in [
            ("New Document", Command::New, Some("Meta+n")),
            ("Open…", Command::Open, Some("Meta+o")),
            ("Quick Open…", Command::QuickOpen, Some("Meta+p")),
            ("Open Folder…", Command::OpenFolder, Some("Meta+Shift+o")),
        ] {
            state.add(&file, label, command, shortcut, false)?;
        }
        file.append(&state.recent)?;
        for (label, command, shortcut) in [
            ("Save", Command::Save, Some("Meta+s")),
            ("Save As…", Command::SaveAs, Some("Meta+Shift+s")),
            ("Export HTML…", Command::Export, None),
        ] {
            state.add(&file, label, command, shortcut, false)?;
        }
        let document = Submenu::new("Document", true);
        for (label, command) in [
            ("Rename…", Command::Rename),
            ("Move To…", Command::Move),
            ("Show Location", Command::Location),
        ] {
            state.add(&document, label, command, None, false)?;
        }
        document.append(&PredefinedMenuItem::separator())?;
        for (label, mode) in [
            ("Write", EditorMode::Live),
            ("Source", EditorMode::Source),
            ("Read", EditorMode::Read),
        ] {
            state.add(&document, label, Command::Mode(mode), None, true)?;
        }
        state.add(&document, "Undo File Move", Command::UndoMove, None, false)?;
        state.add(
            &document,
            "Back to Reference",
            Command::BackToReference,
            None,
            false,
        )?;
        file.append(&document)?;
        file.append(&PredefinedMenuItem::separator())?;
        state.add(&file, "Close Window", Command::Close, Some("Meta+w"), false)?;
        state.root.append(&file)?;

        let edit = Submenu::new("Edit", true);
        for (label, command, shortcut) in [
            ("Undo", menu_input::Command::Undo, "Meta+z"),
            ("Redo", menu_input::Command::Redo, "Meta+Shift+z"),
            ("Cut", menu_input::Command::Cut, "Meta+x"),
            ("Copy", menu_input::Command::Copy, "Meta+c"),
            ("Paste", menu_input::Command::Paste, "Meta+v"),
            ("Select All", menu_input::Command::SelectAll, "Meta+a"),
        ] {
            state.add(&edit, label, Command::Edit(command), Some(shortcut), false)?;
        }
        state.add(
            &edit,
            "Find and Replace…",
            Command::Find,
            Some("Meta+f"),
            false,
        )?;
        state.root.append(&edit)?;

        let format = Submenu::new("Format", true);
        for (label, command, shortcut) in [
            ("Heading", Format::Heading, None),
            ("Bold", Format::Bold, Some("Meta+b")),
            ("Italic", Format::Italic, Some("Meta+i")),
            ("Strikethrough", Format::Strikethrough, Some("Meta+Shift+x")),
            ("Inline Code", Format::Code, None),
            ("Link…", Format::Link, Some("Meta+k")),
            ("Quotation…", Format::Quote, None),
            ("List", Format::List, None),
            ("Task List", Format::Task, None),
            ("Code Block", Format::Fence, None),
            ("Table", Format::Table, None),
        ] {
            state.add(&format, label, Command::Format(command), shortcut, false)?;
        }
        state.add(&format, "Image…", Command::Image, None, false)?;
        state.root.append(&format)?;

        let view = Submenu::new("View", true);
        for (label, command, shortcut, check) in [
            ("Files", Command::Files, Some("Meta+Shift+b"), true),
            ("Outline", Command::Outline, Some("Meta+Alt+o"), true),
            (
                "Filter Outline…",
                Command::FilterOutline,
                Some("Meta+Shift+j"),
                false,
            ),
            (
                "Hide Sidebars",
                Command::HideSidebars,
                Some("Meta+Shift+f"),
                false,
            ),
        ] {
            state.add(&view, label, command, shortcut, check)?;
        }
        view.append(&PredefinedMenuItem::separator())?;
        for (label, preference) in [
            ("Word Count", chrome::Preference::WordCount),
            ("Reading Time", chrome::Preference::ReadingTime),
        ] {
            state.add(&view, label, Command::Preference(preference), None, true)?;
        }
        let typeface = Submenu::new("Typeface", true);
        for font in WritingFont::ALL {
            state.add(
                &typeface,
                &font.to_string(),
                Command::WritingFont(font),
                None,
                true,
            )?;
        }
        view.append(&typeface)?;
        state.root.append(&view)?;
        let window = state.window.clone();
        window.append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::zoom(None),
            &PredefinedMenuItem::fullscreen(None),
        ])?;
        state.root.append(&window)?;
        let help = state.help.clone();
        for (label, command) in [
            ("Welcome", Command::Welcome),
            ("Markdown Guide", Command::Guide),
            ("The Quiet Hour", Command::Sample),
            ("Recovery Copies…", Command::Recovery),
        ] {
            state.add(&help, label, command, None, false)?;
        }
        state.root.append(&help)?;
        let commands: Vec<_> = state
            .entries
            .iter()
            .map(|entry| (entry.item.native().id().clone(), entry.command))
            .collect();
        let (sender, receiver) = mpsc::channel(32);
        let sender = Mutex::new(sender);
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some((_, command)) = commands.iter().find(|(id, _)| id == &event.id)
                && let Ok(mut sender) = sender.try_lock()
            {
                let _ = sender.try_send(*command);
            }
        }));
        Ok((
            state,
            Task::run(receiver.map(Message::NativeMenu), std::convert::identity),
        ))
    }

    fn sync_recent(&mut self, recent: &[PathBuf]) {
        if self.recent_shown.as_deref() == Some(recent) {
            return;
        }
        while self.recent.remove_at(0).is_some() {}
        let result = if recent.is_empty() {
            self.recent.append(&self.recent_empty)
        } else {
            self.recent_items
                .iter()
                .zip(recent)
                .try_for_each(|(item, root)| {
                    item.set_text(workspace::recent_label(root, recent));
                    self.recent.append(item)
                })
                .and_then(|()| {
                    self.recent.append_items(&[
                        &self.recent_separator as &dyn IsMenuItem,
                        &self.recent_clear,
                    ])
                })
        };
        if let Err(error) = result {
            tracing::warn!("could not update Open Recent: {error}");
        }
        self.recent_shown = Some(recent.to_vec());
    }

    fn sync(&mut self, app: &App) {
        self.sync_recent(app.workspace.recent());
        for entry in &mut self.entries {
            let (enabled, checked) = availability(app, entry.command);
            if enabled != entry.enabled {
                entry.item.enabled(enabled);
                entry.enabled = enabled;
            }
            if let Item::Check(item) = &entry.item
                && item.is_checked() != checked
            {
                item.set_checked(checked);
            }
            if entry.command == Command::HideSidebars
                && self.hidden != app.chrome.sidebars_hidden
                && let Item::Action(item) = &entry.item
            {
                item.set_text(if app.chrome.sidebars_hidden {
                    "Show Sidebars"
                } else {
                    "Hide Sidebars"
                });
            }
        }
        self.hidden = app.chrome.sidebars_hidden;
    }
}

fn availability(app: &App, command: Command) -> (bool, bool) {
    let ready = !app.busy && !app.loading && matches!(app.overlay, None | Some(Overlay::Chrome));
    match command {
        Command::QuickOpen => (ready && app.folder.root().is_some(), false),
        Command::OpenRecent(index) => (ready && index < app.workspace.recent().len(), false),
        Command::ClearRecent => (!app.workspace.recent().is_empty(), false),
        Command::Edit(action) => (app.edit_command_enabled(action), false),
        Command::Mode(mode) => (ready, app.mode == mode),
        Command::Format(_) | Command::Image => (ready && app.mode != EditorMode::Read, false),
        Command::Rename | Command::Move | Command::Location => (ready && app.path.is_some(), false),
        Command::UndoMove => (ready && app.folder.can_undo(), false),
        Command::BackToReference => (ready && app.footnotes.can_return(), false),
        Command::Files => (
            ready && app.folder.root().is_some(),
            app.folder.root().is_some() && app.chrome.files.open && !app.chrome.sidebars_hidden,
        ),
        Command::Outline => (
            ready,
            app.chrome.outline.open && !app.chrome.sidebars_hidden,
        ),
        Command::FilterOutline => (ready && !app.outline.sections.is_empty(), false),
        Command::HideSidebars => (
            ready && (app.chrome.files.open || app.chrome.outline.open),
            false,
        ),
        Command::Preference(preference) => (
            true,
            match preference {
                chrome::Preference::WordCount => app.preferences.show_word_count,
                chrome::Preference::ReadingTime => app.preferences.show_reading_time,
            },
        ),
        Command::WritingFont(font) => (true, app.preferences.writing_font == font),
        Command::Close => (!app.busy, false),
        _ => (ready, false),
    }
}

impl App {
    pub(super) fn attach_native_menus(&self) {
        if let Some(menus) = &self.native_menus {
            // Winit installs its default menu during finishLaunching. Iced runs
            // the boot task after the first window opens, so attach ours then.
            menus.root.init_for_nsapp();
            menus.window.set_as_windows_menu_for_nsapp();
            menus.help.set_as_help_menu_for_nsapp();
        }
    }
    pub(in crate::app) fn install_native_menus(&mut self) -> Task<Message> {
        match State::new() {
            Ok((mut menus, events)) => {
                menus.sync(self);
                self.native_menus = Some(menus);
                events
            }
            Err(error) => {
                self.notification = Some(format!("Could not create macOS menus: {error}"));
                Task::none()
            }
        }
    }
    pub(super) fn sync_native_menus(&mut self) {
        if let Some(mut menus) = self.native_menus.take() {
            menus.sync(self);
            self.native_menus = Some(menus);
        }
    }
    pub(super) fn native_menu_command(&mut self, command: Command) -> Task<Message> {
        self.native_menu_queue.push_back(command);
        self.drain_native_menu_commands()
    }
    pub(super) fn drain_native_menu_commands(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        // Focused-widget edits arrive on redraw. Keep later menu commands,
        // especially snapshotting Save, behind those edits in the same order.
        while self.menu_input.is_empty() {
            let Some(command) = self.native_menu_queue.pop_front() else {
                break;
            };
            tasks.push(self.apply_native_menu_command(command));
        }
        Task::batch(tasks)
    }
    fn apply_native_menu_command(&mut self, command: Command) -> Task<Message> {
        if !availability(self, command).0 {
            return Task::none();
        }
        if self.overlay == Some(Overlay::Chrome) && !matches!(command, Command::Edit(_)) {
            self.overlay = None;
            self.chrome.menu = None;
        }
        let message = match command {
            Command::New => self.folder.root().map_or(Message::New, |root| {
                Message::Folder(folder::Message::NewDocument(root.to_owned()))
            }),
            Command::Open => Message::Open,
            Command::QuickOpen => Message::QuickOpen(super::quick_open::Message::Open),
            Command::OpenFolder => Message::Folder(folder::Message::Pick),
            Command::OpenRecent(index) => Message::Workspace(workspace::Message::OpenRecent(index)),
            Command::ClearRecent => Message::Workspace(workspace::Message::ClearRecent),
            Command::Save => Message::Save,
            Command::SaveAs => Message::SaveAs,
            Command::Export => Message::Export,
            Command::Find => Message::ToggleSearch,
            Command::Settings => Message::ShowSettings,
            Command::Welcome => Message::Welcome,
            Command::Guide => Message::Guide,
            Command::Sample => Message::Writing(writing::Message::Sample),
            Command::Recovery => Message::Recovery(recovery::Message::Show),
            Command::Edit(command) => Message::MenuEdit(command),
            Command::Format(Format::Quote) => {
                Message::Writing(writing::Message::Details(writing::Kind::Quote))
            }
            Command::Format(format) => Message::Format(format),
            Command::Image => Message::Writing(writing::Message::Details(writing::Kind::Image)),
            Command::Files => Message::ToggleSidebar(Sidebar::Files),
            Command::Outline => Message::ToggleSidebar(Sidebar::Outline),
            Command::FilterOutline => Message::Outline(super::outline::Message::Filter),
            Command::HideSidebars => Message::HideSidebars,
            Command::Mode(mode) => Message::SetMode(mode),
            Command::WritingFont(font) => Message::WritingFont(font),
            Command::Preference(preference) => Message::Chrome(chrome::Message::Preference(
                preference,
                !availability(self, command).1,
            )),
            Command::UndoMove => Message::Folder(folder::Message::UndoMove),
            Command::BackToReference => Message::Footnote(super::footnotes::Message::Back),
            Command::Close => {
                return window::latest()
                    .map(|id| id.map_or(Message::Dismiss, Message::CloseRequested));
            }
            Command::Rename | Command::Move | Command::Location => {
                let Some(path) = self.path.clone() else {
                    return Task::none();
                };
                Message::Folder(match command {
                    Command::Rename => folder::Message::Rename(path),
                    Command::Move => folder::Message::MoveTo(path),
                    _ => folder::Message::ShowLocation(path),
                })
            }
        };
        self.update_message(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::selection::Selection;

    #[test]
    fn native_new_uses_the_unsaved_guard_and_closes_only_transient_commands() {
        let (mut app, initial) = App::new();
        drop(initial);
        let source = "café and 🦀";
        app.set_document(source.into(), None, "Draft.md".into());
        app.selection = Selection {
            anchor: source.len(),
            head: 0,
        };
        app.sync_editor();
        app.dirty = true;
        app.overlay = Some(Overlay::Chrome);
        app.chrome.menu = Some(chrome::Menu::Document);
        drop(app.native_menu_command(Command::New));
        assert_eq!(app.overlay, Some(Overlay::Unsaved));
        assert!(app.chrome.menu.is_none());
        assert_eq!(app.document.source(), source);
        assert_eq!(
            app.selection,
            Selection {
                anchor: source.len(),
                head: 0
            }
        );
        assert!(app.dirty);
    }

    #[test]
    fn native_menu_availability_tracks_views_modal_guards_and_both_sidebars() {
        let (mut app, initial) = App::new();
        drop(initial);
        app.set_document("café and 🦀".into(), None, "Draft.md".into());
        app.mode = EditorMode::Read;
        assert!(!availability(&app, Command::Format(Format::Bold)).0);
        drop(app.native_menu_command(Command::Format(Format::Bold)));
        assert_eq!(app.document.source(), "café and 🦀");
        drop(app.native_menu_command(Command::Mode(EditorMode::Source)));
        assert!(availability(&app, Command::Mode(EditorMode::Source)).1);
        assert!(!availability(&app, Command::Files).0);
        drop(app.update_folder(folder::Message::Scanned {
            generation: 0,
            result: Ok(crate::io::folder::Snapshot {
                root: "/writing".into(),
                entries: Vec::new(),
                limited: false,
                warnings: Vec::new(),
            }),
        }));
        app.chrome.files.open = true;
        app.chrome.outline.open = true;
        assert!(availability(&app, Command::Files).1 && availability(&app, Command::Outline).1);
        drop(app.native_menu_command(Command::HideSidebars));
        assert!(!availability(&app, Command::Files).1 && !availability(&app, Command::Outline).1);
        app.overlay = Some(Overlay::Settings);
        assert!(!availability(&app, Command::Mode(EditorMode::Read)).0);
        drop(app.native_menu_command(Command::Mode(EditorMode::Read)));
        assert_eq!(app.mode, EditorMode::Source);
    }
}
