//! Explicit project setup, separate from opening a Markdown document.

use std::path::PathBuf;

use iced::widget::{column, container, opaque, operation, row, space, text, text_input};
use iced::{Alignment, Element, Event, Task, event, keyboard, window};

use super::{App, EDITOR, Message as AppMessage, Overlay, workspace};
use crate::io::{project, storage::FileError};
use crate::theme::styles::{self, Button};

const NAME: &str = "project-name";

#[derive(Debug, Clone)]
pub enum Message {
    New,
    SetUp(PathBuf),
    Name(String),
    Browse,
    LocationPicked(Option<PathBuf>),
    Inspected(Result<(PathBuf, bool), FileError>),
    Confirm,
    Finished {
        new: bool,
        result: Result<PathBuf, FileError>,
    },
}

#[derive(Default)]
pub(super) struct State {
    dialog: Option<Dialog>,
    working: bool,
    error: Option<String>,
}

enum Dialog {
    New {
        name: String,
        parent: Option<PathBuf>,
    },
    Existing(PathBuf),
}

pub(super) fn event_message(
    event: Event,
    status: event::Status,
    id: window::Id,
) -> Option<AppMessage> {
    if status == event::Status::Ignored
        && let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = &event
        && modifiers.is_empty()
        && matches!(
            key.as_ref(),
            keyboard::Key::Named(keyboard::key::Named::Enter)
        )
    {
        return Some(AppMessage::Project(Message::Confirm));
    }
    super::event_message(event, status, id)
}

impl App {
    pub(super) fn update_projects(&mut self, message: Message) -> Task<AppMessage> {
        match message {
            Message::New if !self.busy && self.overlay.is_none() => {
                self.projects.dialog = Some(Dialog::New {
                    name: String::new(),
                    parent: None,
                });
                self.projects.error = None;
                self.overlay = Some(Overlay::Project);
                operation::focus(NAME)
            }
            Message::SetUp(path) if !self.busy && self.overlay.is_none() => {
                self.inspect_project(path)
            }
            Message::Name(value) if !self.busy => {
                if let Some(Dialog::New { name, .. }) = &mut self.projects.dialog {
                    *name = value;
                    self.projects.error = None;
                }
                Task::none()
            }
            Message::Browse
                if !self.busy && matches!(self.projects.dialog, Some(Dialog::New { .. })) =>
            {
                self.pick_project_location()
            }
            Message::LocationPicked(path) => {
                self.stop_project_work();
                if let Some(Dialog::New { parent, .. }) = &mut self.projects.dialog
                    && let Some(path) = path
                {
                    *parent = Some(path);
                    self.projects.error = None;
                }
                operation::focus(NAME)
            }
            Message::Inspected(result) => {
                self.stop_project_work();
                match result {
                    Ok((path, true)) => self.open_workspace(path),
                    Ok((path, false)) => {
                        self.projects.dialog = Some(Dialog::Existing(path));
                        self.projects.error = None;
                        self.overlay = Some(Overlay::Project);
                        Task::none()
                    }
                    Err(error) => {
                        self.notification = Some(error.to_string());
                        self.restore_project_focus()
                    }
                }
            }
            Message::Confirm if !self.busy => {
                let (path, name) = match &self.projects.dialog {
                    Some(Dialog::New {
                        name,
                        parent: Some(parent),
                    }) => match project::folder_name(name) {
                        Ok(name) => (parent.clone(), Some(name)),
                        Err(error) => {
                            self.projects.error = Some(error.to_string());
                            return operation::focus(NAME);
                        }
                    },
                    Some(Dialog::Existing(path)) => (path.clone(), None),
                    _ => return Task::none(),
                };
                if !self.commit_cell() {
                    return Task::none();
                }
                let new = name.is_some();
                self.start_project_work();
                Task::perform(project::setup(path, name), move |result| {
                    AppMessage::Project(Message::Finished { new, result })
                })
            }
            Message::Finished { new, result } => {
                self.stop_project_work();
                match result {
                    Ok(path) => {
                        let focus = self.close_project_dialog();
                        Task::batch([focus, self.open_project_workspace(path, new)])
                    }
                    Err(error) => {
                        self.projects.error = Some(error.to_string());
                        Task::none()
                    }
                }
            }
            _ => Task::none(),
        }
    }

    fn start_project_work(&mut self) {
        self.projects.working = true;
        self.busy = true;
    }

    fn stop_project_work(&mut self) {
        self.projects.working = false;
        self.busy = false;
    }

    fn pick_project_location(&mut self) -> Task<AppMessage> {
        self.start_project_work();
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Choose Project Location")
                    .pick_folder()
                    .await
                    .map(|folder| folder.path().to_owned())
            },
            |path| AppMessage::Project(Message::LocationPicked(path)),
        )
    }

    fn inspect_project(&mut self, path: PathBuf) -> Task<AppMessage> {
        self.start_project_work();
        Task::perform(project::inspect(path), |result| {
            AppMessage::Project(Message::Inspected(result))
        })
    }

    fn restore_project_focus(&mut self) -> Task<AppMessage> {
        self.preserve_viewport_generation =
            Some(self.session.document.collaboration().generation());
        operation::focus(EDITOR)
    }

    pub(super) fn close_project_dialog(&mut self) -> Task<AppMessage> {
        if self.projects.working {
            return Task::none();
        }
        self.projects = State::default();
        self.overlay = None;
        self.restore_project_focus()
    }

    pub(super) fn project_dialog(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let Some(dialog) = &self.projects.dialog else {
            return space().into();
        };
        let ready = !self.projects.working;
        let (heading, action, mut content, confirm) = match dialog {
            Dialog::New { name, parent } => {
                let field = text_input("Project name", name)
                    .id(NAME)
                    .on_input_maybe(
                        ready.then_some(|value| AppMessage::Project(Message::Name(value))),
                    )
                    .on_submit_maybe(ready.then_some(AppMessage::Project(if parent.is_some() {
                        Message::Confirm
                    } else {
                        Message::Browse
                    })))
                    .padding(9)
                    .style(styles::input(tokens));
                let mut content = column![
                    field,
                    self.small_button_disabled(
                        if parent.is_some() {
                            "Change location…"
                        } else {
                            "Choose location…"
                        },
                        ready.then_some(AppMessage::Project(Message::Browse)),
                        Button::Quiet,
                    )
                ]
                .spacing(10);
                let valid = project::folder_name(name);
                if let Some(parent) = parent {
                    let destination = valid
                        .as_ref()
                        .map_or_else(|_| parent.clone(), |name| parent.join(name));
                    content = content.push(
                        text(workspace::project_path(&destination))
                            .size(12)
                            .color(tokens.muted),
                    );
                }
                (
                    "New Project",
                    "Create Project",
                    content,
                    ready && parent.is_some() && !name.trim().is_empty(),
                )
            }
            Dialog::Existing(path) => (
                "Use This Folder as a Project?",
                "Use as Project",
                column![
                    text(workspace::project_path(path)).size(13),
                    text("Dosty will add .dosty/project.toml to this folder.")
                        .size(13)
                        .color(tokens.muted),
                ]
                .spacing(10),
                ready,
            ),
        };
        if let Some(error) = &self.projects.error {
            content = content.push(text(error).size(13).color(tokens.danger));
        }
        opaque(
            container(
                column![
                    text(heading).size(21),
                    content,
                    row![
                        space::horizontal(),
                        self.small_button_disabled(
                            "Cancel",
                            ready.then_some(AppMessage::CloseOverlay),
                            Button::Quiet
                        ),
                        self.small_button_disabled(
                            action,
                            confirm.then_some(AppMessage::Project(Message::Confirm)),
                            Button::Primary
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(16),
            )
            .padding(24)
            .width(460)
            .style(styles::card(tokens)),
        )
    }
}
