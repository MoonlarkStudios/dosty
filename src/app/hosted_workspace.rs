//! A hosted workspace uses the normal editor with a remote saved revision.

use std::time::Duration;

use iced::widget::{button, column, container, operation, pick_list, row, text, text_input};
use iced::{Element, Fill, Task};

use crate::document::Document;
use crate::io::hosted_workspace::{self, Client, Job, JobState, Post, RequestError, Workspace};
use crate::io::workspace_document::{
    Cover, Details, HostedAddress, HostedDraft, WorkspaceDocument,
};
use crate::theme::styles::{self, Button};

use super::session::Session;
use super::{App, EDITOR, Message as AppMessage, Overlay};

#[derive(Default)]
pub(super) struct State {
    origin: String,
    key: String,
    client: Option<Client>,
    workspace: Option<Workspace>,
    posts: Vec<Post>,
    issue: Option<String>,
    job: Option<Job>,
    active_post: Option<Post>,
    conflict: Option<HostedAddress>,
}

#[derive(Clone, PartialEq, Eq)]
struct Section {
    id: String,
    label: String,
}

impl std::fmt::Display for Section {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

#[derive(Clone)]
pub struct AccessKey(String);

impl std::fmt::Debug for AccessKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AccessKey(…)")
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Show,
    Origin(String),
    Key(AccessKey),
    Connect,
    Disconnect,
    Connected(Result<(Workspace, Vec<Post>), String>),
    Refresh,
    Posts(Result<Vec<Post>, String>),
    Open(uuid::Uuid),
    New,
    Opened(Result<Box<Post>, String>),
    Title(String),
    Section(String),
    Alt(String),
    Photo,
    PhotoUploaded(Result<Option<Cover>, String>),
    RemovePhoto,
    ActionSave,
    SaveCopy,
    Action(String),
    Saved {
        post: Result<Box<Post>, RequestError>,
        address: HostedAddress,
        copy: bool,
        generation: u64,
        action: Option<String>,
    },
    JobStarted(Result<Job, String>),
    JobFinished(String, Result<Job, String>),
    OpenResult(String),
}

impl State {
    fn connected_to(&self, origin: &str) -> bool {
        self.client
            .as_ref()
            .is_some_and(|client| client.origin() == origin)
    }

    fn draft_client(&self, session: &Session) -> Option<&Client> {
        let draft = session.hosted()?;
        self.client
            .as_ref()
            .filter(|client| client.origin() == draft.address.origin)
    }

    fn disconnect(&mut self) {
        self.client = None;
        self.workspace = None;
        self.posts.clear();
        self.active_post = None;
        self.job = None;
        self.issue = None;
        self.key.clear();
    }

    fn remember_post(&mut self, post: Post) {
        if let Some(summary) = self.posts.iter_mut().find(|summary| summary.id == post.id) {
            *summary = post.clone();
        } else {
            self.posts.insert(0, post.clone());
        }
        self.active_post = Some(post);
    }
}

impl App {
    pub(super) fn update_hosted(&mut self, message: Message) -> Task<AppMessage> {
        match message {
            Message::Show if !self.busy => {
                self.chrome.menu = None;
                self.overlay = Some(Overlay::Hosted);
                if let Some(origin) = self
                    .session
                    .hosted()
                    .map(|draft| draft.address.origin.clone())
                    && !self.hosted.connected_to(&origin)
                {
                    return self.connect_hosted_draft(origin);
                }
                if self.hosted.client.is_none() {
                    return operation::focus("hosted-workspace-origin");
                }
                return self.update_hosted(Message::Refresh);
            }
            Message::Origin(value) => self.hosted.origin = value,
            Message::Key(value) => self.hosted.key = value.0,
            Message::Connect if !self.busy => {
                match Client::new(self.hosted.origin.trim(), self.hosted.key.trim().to_owned()) {
                    Ok(client) => {
                        self.hosted.key.clear();
                        self.hosted.client = Some(client.clone());
                        self.hosted.issue = None;
                        self.busy = true;
                        return Task::perform(
                            async move {
                                let workspace = client.workspace().await?;
                                let posts = client.posts().await?;
                                Ok((workspace, posts))
                            },
                            |result| AppMessage::Hosted(Message::Connected(result)),
                        );
                    }
                    Err(error) => self.hosted.issue = Some(error),
                }
            }
            Message::Connected(result) => {
                self.busy = false;
                match result {
                    Ok((workspace, posts)) => {
                        self.hosted.workspace = Some(workspace);
                        self.hosted.posts = posts;
                    }
                    Err(error) => {
                        self.hosted.client = None;
                        self.hosted.issue = Some(error);
                    }
                }
            }
            Message::Disconnect if !self.busy => {
                self.hosted.disconnect();
                return operation::focus("hosted-workspace-origin");
            }
            Message::Refresh if !self.busy => {
                if let Some(client) = self.hosted.client.clone() {
                    self.busy = true;
                    self.hosted.issue = None;
                    return Task::perform(async move { client.posts().await }, |result| {
                        AppMessage::Hosted(Message::Posts(result))
                    });
                }
            }
            Message::Posts(result) => {
                self.busy = false;
                match result {
                    Ok(posts) => self.hosted.posts = posts,
                    Err(error) => self.hosted.issue = Some(error),
                }
            }
            Message::Open(id) if !self.busy && self.commit_cell() => {
                if let Some(client) = self.hosted.client.clone() {
                    let address = HostedAddress {
                        origin: client.origin().to_owned(),
                        id,
                    };
                    if self.session.dirty
                        && self
                            .session
                            .hosted()
                            .is_some_and(|draft| draft.address == address)
                    {
                        self.overlay = None;
                        return operation::focus(EDITOR);
                    }
                    if let Some(parked) = self.background.take_hosted(&address) {
                        return self.resume_at(parked, None, false);
                    }
                    self.busy = true;
                    self.hosted.issue = None;
                    return Task::perform(async move { client.post(id).await }, |result| {
                        AppMessage::Hosted(Message::Opened(result.map(Box::new)))
                    });
                }
            }
            Message::New if !self.busy && self.commit_cell() => {
                if let Some(client) = self.hosted.client.clone() {
                    self.busy = true;
                    self.hosted.issue = None;
                    return Task::perform(async move { client.new_post().await }, |result| {
                        AppMessage::Hosted(Message::Opened(result.map(Box::new)))
                    });
                }
            }
            Message::Opened(result) => {
                self.busy = false;
                match result {
                    Ok(mut post) => {
                        let Some(client) = &self.hosted.client else {
                            return Task::none();
                        };
                        let source = std::mem::take(&mut post.body);
                        let mut session = Session::new(
                            Document::new(source),
                            None,
                            post_name(&post.details.title),
                        );
                        session.backing = WorkspaceDocument::Hosted(HostedDraft {
                            address: HostedAddress {
                                origin: client.origin().to_owned(),
                                id: post.id,
                            },
                            revision: post.revision,
                            saved_details: post.details.clone(),
                            details: post.details.clone(),
                        });
                        let job = [&post.publish, &post.preview]
                            .into_iter()
                            .flatten()
                            .find(|job| job.active())
                            .cloned();
                        self.hosted.remember_post(*post);
                        self.install_session(session);
                        return Task::batch([
                            operation::focus(EDITOR),
                            job.map_or_else(Task::none, |job| self.watch_hosted_job(job)),
                        ]);
                    }
                    Err(error) => self.hosted.issue = Some(error),
                }
            }
            Message::Title(value) => self.hosted_details(|details| details.title = value),
            Message::Section(value) => self.hosted_details(|details| details.section = value),
            Message::Alt(value) => self.hosted_details(|details| {
                if let Some(cover) = &mut details.cover {
                    cover.alt = value;
                }
            }),
            Message::RemovePhoto => self.hosted_details(|details| details.cover = None),
            Message::Photo if !self.busy => {
                if let Some(client) = self.hosted.draft_client(&self.session).cloned() {
                    self.busy = true;
                    return Task::perform(async move { client.photo().await }, |result| {
                        AppMessage::Hosted(Message::PhotoUploaded(result))
                    });
                }
            }
            Message::PhotoUploaded(result) => {
                self.busy = false;
                match result {
                    Ok(Some(cover)) => self.hosted_details(|details| details.cover = Some(cover)),
                    Ok(None) => {}
                    Err(error) => self.hosted.issue = Some(error),
                }
            }
            Message::ActionSave if self.commit_cell() => return self.save_hosted(None),
            Message::SaveCopy if self.commit_cell() => return self.write_hosted(None, true),
            Message::Action(action)
                if matches!(action.as_str(), "preview" | "publish") && self.commit_cell() =>
            {
                return self.save_hosted(Some(action));
            }
            Message::Saved {
                post,
                address,
                copy,
                generation,
                action,
            } => {
                self.busy = false;
                self.chrome.saving = false;
                if !self
                    .session
                    .hosted()
                    .is_some_and(|draft| draft.address == address)
                {
                    return Task::none();
                }
                match post {
                    Ok(mut post) => {
                        let source = std::mem::take(&mut post.body);
                        if let Some(active) = self.session.hosted_mut() {
                            active.address.id = post.id;
                            active.revision = post.revision;
                            active.saved_details = post.details.clone();
                            active.details = post.details.clone();
                        }
                        self.session.display_name = post_name(&post.details.title);
                        self.hosted.remember_post(*post);
                        self.session.mark_hosted_saved(source, generation);
                        self.chrome.save_error = None;
                        self.hosted.issue = None;
                        self.hosted.conflict = None;
                        if !self.session.dirty {
                            self.recovery.retire();
                            if let Some(pending) = self.pending.take() {
                                self.overlay = None;
                                return self.execute(pending);
                            }
                        }
                        if copy {
                            self.notification = Some("Saved as a new draft.".into());
                        }
                        if let Some(action) = action {
                            return self.run_hosted_action(action);
                        }
                    }
                    Err(error) => {
                        let message = if !copy && error.conflict() {
                            self.hosted.conflict = Some(address);
                            self.overlay = Some(Overlay::Hosted);
                            "A newer revision is saved in the workspace. You can keep editing or save your work as a new draft.".to_owned()
                        } else {
                            error.message
                        };
                        self.chrome.save_error = Some(message.clone());
                        self.hosted.issue = Some(message.clone());
                        self.notification = Some(message);
                    }
                }
            }
            Message::JobStarted(result) => {
                self.busy = false;
                match result {
                    Ok(job) => return self.watch_hosted_job(job),
                    Err(error) => self.hosted.issue = Some(error),
                }
            }
            Message::JobFinished(origin, result) if self.hosted.connected_to(&origin) => {
                match result {
                    Ok(job) => {
                        if let Some(post) = self.hosted.active_post.as_mut()
                            && post.id == job.post_id
                        {
                            if job.mode == "preview" {
                                post.preview = Some(job.clone());
                            } else {
                                if job.state == JobState::Succeeded {
                                    post.published_revision = Some(job.revision);
                                }
                                post.publish = Some(job.clone());
                            }
                        }
                        if self.session.hosted().is_some_and(|draft| {
                            draft.address.origin == origin && draft.address.id == job.post_id
                        }) && self.overlay.is_none()
                        {
                            self.notification = Some(job.label().to_owned());
                        }
                        self.hosted.job = Some(job);
                    }
                    Err(error) => {
                        self.hosted.job = None;
                        self.hosted.issue = Some(error);
                    }
                }
            }
            Message::OpenResult(url) => {
                if let Err(error) = hosted_workspace::open_result(&url) {
                    self.hosted.issue = Some(error);
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn connect_hosted_draft(&mut self, origin: String) -> Task<AppMessage> {
        self.hosted.disconnect();
        self.hosted.origin = origin;
        self.overlay = Some(Overlay::Hosted);
        operation::focus("hosted-workspace-origin")
    }

    fn hosted_details(&mut self, update: impl FnOnce(&mut Details)) {
        if !self.busy
            && let Some(active) = self.session.hosted_mut()
        {
            update(&mut active.details);
            self.session.display_name = post_name(&active.details.title);
            self.session.workspace_changed();
            self.recovery.edited();
        }
    }

    pub(super) fn save_hosted(&mut self, action: Option<String>) -> Task<AppMessage> {
        self.write_hosted(action, false)
    }

    fn write_hosted(&mut self, action: Option<String>, copy: bool) -> Task<AppMessage> {
        if self.busy {
            return Task::none();
        }
        let Some(active) = self.session.hosted() else {
            return Task::none();
        };
        let Some(client) = self.hosted.draft_client(&self.session).cloned() else {
            return self.connect_hosted_draft(active.address.origin.clone());
        };
        if copy && self.hosted.conflict.as_ref() != Some(&active.address) {
            return Task::none();
        }
        if !self.session.dirty && !copy {
            return action.map_or_else(Task::none, |action| self.run_hosted_action(action));
        }
        let address = active.address.clone();
        let id = address.id;
        let revision = active.revision;
        let details = active.details.clone();
        let source = self.session.document.source().to_owned();
        let generation = self.session.document.collaboration().generation();
        self.busy = true;
        self.chrome.saving = true;
        self.hosted.issue = None;
        Task::perform(
            async move {
                if copy {
                    client.copy_draft(details, &source).await
                } else {
                    client.save(id, revision, details, &source).await
                }
            },
            move |post| {
                AppMessage::Hosted(Message::Saved {
                    post: post.map(Box::new),
                    address: address.clone(),
                    copy,
                    generation,
                    action: action.clone(),
                })
            },
        )
    }

    fn run_hosted_action(&mut self, action: String) -> Task<AppMessage> {
        let (Some(client), Some(active)) = (
            self.hosted.draft_client(&self.session).cloned(),
            self.session.hosted(),
        ) else {
            return Task::none();
        };
        let id = active.address.id;
        let revision = active.revision;
        self.hosted.issue = None;
        self.busy = true;
        Task::perform(
            async move { client.action(id, revision, &action).await },
            |result| AppMessage::Hosted(Message::JobStarted(result)),
        )
    }

    fn watch_hosted_job(&mut self, mut job: Job) -> Task<AppMessage> {
        self.hosted.job = Some(job.clone());
        let Some(client) = self.hosted.client.clone() else {
            return Task::none();
        };
        let origin = client.origin().to_owned();
        Task::perform(
            async move {
                while job.active() {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    job = client.job(job.id).await?;
                }
                Ok(job)
            },
            move |result| AppMessage::Hosted(Message::JobFinished(origin.clone(), result)),
        )
    }

    pub(super) fn hosted_view(&self) -> Element<'_, AppMessage> {
        let tokens = self.resolved_theme.tokens;
        let send = AppMessage::Hosted;
        let ready = !self.busy;
        let control = |label: String, message, style| {
            button(text(label))
                .padding([7, 12])
                .on_press_maybe(ready.then_some(send(message)))
                .style(styles::button_style(tokens, style))
        };
        let label = self
            .hosted
            .workspace
            .as_ref()
            .map_or("Hosted workspace", |workspace| workspace.name.as_str());
        let mut body = column![text(label).size(24)].spacing(12);
        if let Some(workspace) = &self.hosted.workspace {
            let mut toolbar = row![].spacing(8);
            if let Some(action) = workspace
                .actions
                .iter()
                .find(|action| action.id == "new-post")
            {
                toolbar =
                    toolbar.push(control(action.label.clone(), Message::New, Button::Primary));
            }
            toolbar = toolbar.push(control(
                "Refresh posts".into(),
                Message::Refresh,
                Button::Quiet,
            ));
            body = body.push(toolbar);
            body = body.push(control(
                "Disconnect".into(),
                Message::Disconnect,
                Button::Quiet,
            ));
        } else {
            body = body
                .push(
                    text_input("Workspace URL", &self.hosted.origin)
                        .id("hosted-workspace-origin")
                        .on_input(move |value| send(Message::Origin(value))),
                )
                .push(
                    text_input("Workspace access key", &self.hosted.key)
                        .on_submit(send(Message::Connect))
                        .secure(true)
                        .on_input(move |value| send(Message::Key(AccessKey(value)))),
                )
                .push(control("Connect".into(), Message::Connect, Button::Primary));
        }
        if let Some(active) = self.session.hosted() {
            let details = &active.details;
            let connected = self.hosted.connected_to(&active.address.origin);
            let sections = self
                .hosted
                .workspace
                .as_ref()
                .filter(|_| connected)
                .into_iter()
                .flat_map(|workspace| &workspace.sections)
                .map(|(id, label)| Section {
                    id: id.clone(),
                    label: label.clone(),
                })
                .collect::<Vec<_>>();
            let selected = sections
                .iter()
                .find(|section| section.id == details.section)
                .cloned();
            let post = self
                .hosted
                .active_post
                .as_ref()
                .filter(|post| connected && post.id == active.address.id)
                .or_else(|| {
                    self.hosted
                        .posts
                        .iter()
                        .find(|post| connected && post.id == active.address.id)
                });
            let mut actions = row![control(
                "Save draft".into(),
                Message::ActionSave,
                Button::Quiet
            )]
            .spacing(8);
            for action in post.into_iter().flat_map(|post| &post.actions) {
                if !matches!(action.id.as_str(), "preview" | "publish") {
                    continue;
                }
                let style = if action.id == "publish" {
                    Button::Primary
                } else {
                    Button::Quiet
                };
                actions = actions.push(control(
                    action.label.clone(),
                    Message::Action(action.id.clone()),
                    style,
                ));
            }
            body = body.push(text("Post details").size(16)).push(
                text_input("Title", &details.title)
                    .on_input_maybe(ready.then_some(move |value| send(Message::Title(value)))),
            );
            if !sections.is_empty() {
                body = body.push(
                    pick_list(sections, selected, move |value: Section| {
                        send(Message::Section(value.id))
                    })
                    .placeholder(details.section.as_str()),
                );
            } else if !details.section.is_empty() {
                body = body.push(text(&details.section).size(14));
            }
            if connected {
                body = body.push(control(
                    "Choose cover photo…".into(),
                    Message::Photo,
                    Button::Quiet,
                ));
            }
            if let Some(cover) = &details.cover {
                body = body
                    .push(
                        text_input("Describe the cover photo", &cover.alt).on_input_maybe(
                            ready.then_some(move |value| send(Message::Alt(value))),
                        ),
                    )
                    .push(control(
                        "Remove photo".into(),
                        Message::RemovePhoto,
                        Button::Quiet,
                    ));
            }
            if connected
                && self.hosted.conflict.as_ref() == Some(&active.address)
                && self.hosted.workspace.as_ref().is_some_and(|workspace| {
                    workspace
                        .actions
                        .iter()
                        .any(|action| action.id == "new-post")
                })
            {
                actions = actions.push(control(
                    "Save as new draft".into(),
                    Message::SaveCopy,
                    Button::Primary,
                ));
            }
            body = body.push(actions);
            let job = self
                .hosted
                .job
                .as_ref()
                .filter(|job| connected && job.post_id == active.address.id)
                .or_else(|| post.and_then(|post| post.publish.as_ref().or(post.preview.as_ref())));
            if let Some(job) = job {
                body = body.push(text(job.label()).size(14));
                if let Some(url) = &job.url {
                    let caption = if job.mode == "preview" {
                        "Open preview"
                    } else {
                        "View live post"
                    };
                    body = body.push(control(
                        caption.into(),
                        Message::OpenResult(url.clone()),
                        Button::Quiet,
                    ));
                }
            }
        }
        if self.hosted.workspace.is_some() {
            body = body.push(text("Posts").size(16));
            let mut posts = column![].spacing(4);
            for post in &self.hosted.posts {
                let status = match post.published_revision {
                    Some(revision) if revision == post.revision => "Published",
                    Some(_) => "Unpublished changes",
                    None => "Draft",
                };
                posts = posts.push(control(
                    format!("{} · {status}", post_name(&post.details.title)),
                    Message::Open(post.id),
                    Button::Quiet,
                ));
            }
            body = body.push(iced::widget::scrollable(posts).height(140));
        }
        if let Some(issue) = &self.hosted.issue {
            body = body.push(text(issue).color(tokens.danger).size(14));
        }
        body = body.push(
            button(text("Close"))
                .on_press_maybe(ready.then_some(AppMessage::CloseOverlay))
                .style(styles::button_style(tokens, Button::Quiet)),
        );
        container(body.width(Fill))
            .width(460)
            .padding(20)
            .style(styles::panel(tokens.surface))
            .into()
    }
}

fn post_name(title: &str) -> String {
    if title.trim().is_empty() {
        "Untitled post".into()
    } else {
        title.trim().into()
    }
}

#[cfg(test)]
#[path = "hosted_workspace_tests.rs"]
mod tests;
