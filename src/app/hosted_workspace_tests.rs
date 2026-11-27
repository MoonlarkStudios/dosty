use std::path::{Path, PathBuf};

use iced::futures::StreamExt;
use iced::window;

use super::{
    AccessKey, App, AppMessage, Client, Details, HostedAddress, Job, JobState, Message, Post,
    Session, Workspace,
};
use crate::app::application::trail::Direction;
use crate::app::application::{EditorMode, Overlay, Pending, recovery, table_grid};
use crate::document::Document;
use crate::document::selection::Selection;
use crate::io::recovery as store;

async fn send(app: &mut App, message: AppMessage) {
    let mut pending = vec![app.update(message)];
    while let Some(task) = pending.pop() {
        if let Some(mut events) = iced_runtime::task::into_stream(task) {
            while let Some(event) = events.next().await {
                if let iced_runtime::Action::Output(message) = event {
                    pending.push(app.update(message));
                }
            }
        }
    }
}

async fn keep_and_close(app: &mut App) {
    send(app, AppMessage::CloseRequested(window::Id::unique())).await;
    assert_eq!(app.overlay, Some(Overlay::Unsaved));
    send(app, AppMessage::Recovery(recovery::Message::KeepAndClose)).await;
    assert!(
        app.pending.is_none(),
        "{}",
        app.chrome.save_error.as_deref().unwrap_or("close pending")
    );
}

async fn recover(directory: &Path) -> App {
    let (mut app, _) = App::new();
    app.use_recovery_directory(directory.to_owned());
    let entries = store::list(directory).unwrap();
    assert_eq!(entries.len(), 1);
    send(&mut app, AppMessage::Recovery(recovery::Message::Show)).await;
    send(
        &mut app,
        AppMessage::Recovery(recovery::Message::Restore(entries[0].id)),
    )
    .await;
    app
}

#[tokio::test]
async fn closing_and_recovering_retains_metadata_only_edits_and_their_saved_base() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, _) = App::new();
    app.use_recovery_directory(directory.path().to_owned());
    let address = HostedAddress {
        origin: "https://workspace.example.invalid/".into(),
        id: uuid::Uuid::new_v4(),
    };
    let details = Details {
        title: "Saved title".into(),
        section: "essays".into(),
        cover: None,
    };
    app.install_session(Session::open_hosted(
        Document::new("Saved body.".into()),
        address.clone(),
        7,
        details.clone(),
    ));
    let initial = app.session.recovery_version();
    send(
        &mut app,
        AppMessage::Hosted(Message::Title("Unsaved title".into())),
    )
    .await;
    assert_eq!(app.session.recovery_version().0, initial.0);
    assert_ne!(app.session.recovery_version(), initial);
    assert!(app.session.is_dirty());
    keep_and_close(&mut app).await;

    let mut reopened = recover(directory.path()).await;
    let draft = reopened.session.hosted().unwrap();
    assert_eq!(draft.address, address);
    assert_eq!(draft.revision, 7);
    assert_eq!(draft.saved_details, details);
    assert_eq!(draft.details.title, "Unsaved title");
    assert_eq!(reopened.session.document.source(), "Saved body.");
    assert!(reopened.session.is_dirty());
    assert!(reopened.hosted.client.is_none());
    send(&mut reopened, AppMessage::Save).await;
    assert_eq!(reopened.hosted.origin, address.origin);
    assert_eq!(reopened.overlay, Some(Overlay::Hosted));
}

const LOCAL_BODY: &str = "# Local revision\r\n\r\nCafé 海 👩‍💻\r\n";
const LOCAL_TITLE: &str = "Unsaved from Dosty";
const LOCAL_ALT: &str = "Unsaved photo description";

async fn connect(app: &mut App, client: &Client, key: &str) {
    for message in [
        Message::Show,
        Message::Origin(client.origin().to_owned()),
        Message::Key(AccessKey(key.into())),
        Message::Connect,
    ] {
        send(app, AppMessage::Hosted(message)).await;
    }
    assert!(app.hosted.workspace.is_some(), "{:?}", app.hosted.issue);
}

#[tokio::test]
#[ignore = "requires an isolated hosted service and a second writer"]
async fn hosted_browser_conflict_survives_process_restart() {
    let origin = std::env::var("DOSTY_WORKSPACE_TEST_URL").unwrap();
    assert!(origin.starts_with("http://127.0.0.1:"));
    let key = std::env::var("DOSTY_WORKSPACE_TEST_KEY").unwrap();
    let post = std::env::var("DOSTY_WORKSPACE_TEST_POST")
        .unwrap()
        .parse()
        .unwrap();
    let directory = PathBuf::from(std::env::var_os("DOSTY_WORKSPACE_TEST_DIR").unwrap());
    let client = Client::new(&origin, key.clone()).unwrap();
    let original = client.post(post).await.unwrap();
    match std::env::var("DOSTY_WORKSPACE_TEST_PHASE")
        .unwrap()
        .as_str()
    {
        "edit" => {
            let (mut app, _) = App::new();
            app.use_recovery_directory(directory.join("recovery"));
            connect(&mut app, &client, &key).await;
            let section = app
                .hosted
                .workspace
                .as_ref()
                .unwrap()
                .sections
                .keys()
                .find(|section| **section != original.details.section)
                .unwrap()
                .clone();
            send(&mut app, AppMessage::Hosted(Message::Open(post))).await;
            let end = app.session.document.source().len();
            app.session.document.replace(0..end, LOCAL_BODY).unwrap();
            app.session.selection = Selection {
                anchor: 14,
                head: 2,
            };
            app.changed();
            app.sync_editor();
            for message in [
                Message::Title(LOCAL_TITLE.into()),
                Message::Section(section),
                Message::Alt(LOCAL_ALT.into()),
            ] {
                send(&mut app, AppMessage::Hosted(message)).await;
            }
            std::fs::write(directory.join("native-ready"), b"").unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(45), async {
                while !directory.join("browser-saved").exists() {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            })
            .await
            .unwrap();
            send(&mut app, AppMessage::Save).await;
            assert_eq!(
                app.hosted.conflict,
                Some(HostedAddress {
                    origin: client.origin().into(),
                    id: post
                })
            );
            assert_eq!(app.session.document.source(), LOCAL_BODY);
            assert_eq!(app.session.hosted().unwrap().revision, original.revision);
            assert!(app.session.is_dirty());
            keep_and_close(&mut app).await;
            let entries = store::list(&directory.join("recovery")).unwrap();
            let bytes = std::fs::read(
                directory
                    .join("recovery")
                    .join(format!("{}.daytripdraft", entries[0].id)),
            )
            .unwrap();
            assert!(
                !bytes
                    .windows(key.len())
                    .any(|window| window == key.as_bytes())
            );
        }
        "recover" => {
            let mut app = recover(&directory.join("recovery")).await;
            let retained = app.session.hosted().unwrap();
            assert_eq!(retained.address.id, post);
            assert!(retained.revision < original.revision);
            assert_eq!(retained.details.title, LOCAL_TITLE);
            assert_ne!(retained.details.section, retained.saved_details.section);
            assert_eq!(retained.details.cover.as_ref().unwrap().alt, LOCAL_ALT);
            let expected_details = retained.details.clone();
            assert_eq!(app.session.document.source(), LOCAL_BODY);
            assert_eq!(
                app.session.selection,
                Selection {
                    anchor: 14,
                    head: 2
                }
            );
            assert!(app.hosted.client.is_none());
            connect(&mut app, &client, &key).await;
            send(&mut app, AppMessage::Save).await;
            assert!(app.hosted.conflict.is_some());
            send(&mut app, AppMessage::Hosted(Message::SaveCopy)).await;
            assert!(!app.session.is_dirty(), "{:?}", app.hosted.issue);
            let copy = app.session.hosted().unwrap();
            assert_ne!(copy.address.id, post);
            let saved_copy = client.post(copy.address.id).await.unwrap();
            assert_eq!(saved_copy.body, LOCAL_BODY);
            assert_eq!(saved_copy.details, expected_details);
            let saved_original = client.post(post).await.unwrap();
            assert_eq!(saved_original.revision, original.revision);
            assert_eq!(saved_original.body, original.body);
            assert_eq!(saved_original.details, original.details);
            assert_eq!(store::list(&directory.join("recovery")).unwrap().len(), 1);
        }
        phase => panic!("Unknown workspace test phase: {phase}"),
    }
}

#[test]
fn closing_the_hosted_overlay_commits_a_pending_table_edit_before_the_save_guard() {
    let (mut app, _) = App::new();
    let source = "| A | B |\n|---|---|\n| old | value |\n";
    app.set_document(source.into(), None, "Table.md".into());
    drop(app.update(AppMessage::SetMode(EditorMode::Live)));
    drop(
        app.update(AppMessage::TableGrid(table_grid::Message::Select {
            revision: app.projection.source_id.clone(),
            table: 0,
            row: 1,
            column: 0,
            extend: false,
            extent: table_grid::Extent::Cell,
        })),
    );
    drop(
        app.update(AppMessage::TableGrid(table_grid::Message::Begin(Some(
            "pending café".into(),
        )))),
    );
    assert!(!app.session.is_dirty());
    drop(app.update(AppMessage::Hosted(Message::Show)));
    assert_eq!(app.overlay, Some(Overlay::Hosted));
    assert_eq!(app.session.document.source(), source);

    drop(app.update(AppMessage::CloseRequested(window::Id::unique())));
    assert!(app.session.document.source().contains("pending café"));
    assert!(app.session.is_dirty());
    assert!(app.grid.draft.is_none());
    assert_eq!(app.overlay, Some(Overlay::Unsaved));
    assert!(matches!(app.pending, Some(Pending::Close(_))));
}

const ORIGIN: &str = "https://workspace.example.invalid/";

fn connected_app() -> App {
    let (mut app, _) = App::new();
    app.hosted.client = Some(Client::new(ORIGIN, "test".into()).unwrap());
    app
}

fn post(id: uuid::Uuid, revision: u64, body: &str) -> Post {
    Post {
        id,
        revision,
        published_revision: None,
        actions: Vec::new(),
        details: Details {
            title: "Saved title".into(),
            section: "essays".into(),
            cover: None,
        },
        body: body.into(),
        publish: None,
        preview: None,
    }
}

fn deliver_opened(app: &mut App, post: Post) {
    drop(app.update(AppMessage::Hosted(Message::Opened(Ok(Box::new(post))))));
}

#[test]
fn back_and_forward_reopen_saved_hosted_drafts_and_reuse_only_the_unchanged_session() {
    let mut app = connected_app();
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let address = HostedAddress {
        origin: ORIGIN.into(),
        id: first,
    };

    deliver_opened(&mut app, post(first, 1, "Original"));
    let first_session = app.session.id();
    app.session.document.replace(0..8, "Local saved").unwrap();
    app.changed();
    let version = app.session.recovery_version();
    drop(app.update(AppMessage::Hosted(Message::Saved {
        post: Ok(Box::new(post(first, 2, "Local saved"))),
        address: address.clone(),
        version,
        action: None,
        copy: false,
    })));
    assert!(!app.session.is_dirty());
    assert!(app.session.document.can_undo());
    drop(app.update(AppMessage::Hosted(Message::Open(second))));
    deliver_opened(&mut app, post(second, 1, "Second"));
    let second_session = app.session.id();

    drop(app.update(AppMessage::Step(Direction::Back)));
    assert!(app.loading);
    assert_eq!(app.session.id(), second_session);
    deliver_opened(&mut app, post(first, 2, "Local saved"));
    assert_eq!(app.session.id(), first_session);
    assert!(app.session.document.can_undo());

    drop(app.update(AppMessage::Step(Direction::Forward)));
    assert!(app.loading);
    deliver_opened(&mut app, post(second, 1, "Second"));
    assert_eq!(app.session.id(), second_session);

    drop(app.update(AppMessage::Hosted(Message::Disconnect)));
    drop(app.update(AppMessage::Step(Direction::Back)));
    assert_eq!(app.hosted.open_after_connect.as_ref(), Some(&address));
    assert_eq!(app.overlay, Some(Overlay::Hosted));
    assert_eq!(app.hosted.origin, ORIGIN);
    app.hosted.client = Some(Client::new(ORIGIN, "test".into()).unwrap());
    drop(app.update(AppMessage::Hosted(Message::Connected(Ok((
        Workspace {
            version: 1,
            name: "Writing".into(),
            sections: Default::default(),
            actions: Vec::new(),
        },
        Vec::new(),
    ))))));
    assert!(app.loading);
    deliver_opened(&mut app, post(first, 3, "Browser saved"));
    assert_ne!(app.session.id(), first_session);
    assert_eq!(app.session.document.source(), "Browser saved");
    assert!(!app.session.document.can_undo());
    assert!(app.trail.can_step(Direction::Forward));
}

#[test]
fn older_job_completions_cannot_replace_the_tracked_job_or_report_its_error() {
    let mut app = connected_app();
    let post_id = uuid::Uuid::new_v4();
    deliver_opened(&mut app, post(post_id, 2, "Draft"));
    let job = |mode: &str| Job {
        id: uuid::Uuid::new_v4(),
        post_id,
        mode: mode.into(),
        state: JobState::Building,
        revision: 2,
        url: None,
        error: None,
    };
    let preview = job("preview");
    let publish = job("publish");
    let connection = app.hosted.connection;
    drop(app.watch_hosted_job(preview.clone()));
    drop(app.watch_hosted_job(publish.clone()));
    let finish = |connection, id, result| {
        AppMessage::Hosted(Message::JobFinished {
            connection,
            id,
            result,
        })
    };
    drop(app.update(finish(
        connection,
        preview.id,
        Err("Old poll failed".into()),
    )));
    let completed_preview = Job {
        state: JobState::Succeeded,
        ..preview
    };
    drop(app.update(finish(
        connection,
        completed_preview.id,
        Ok(completed_preview),
    )));
    assert_eq!(app.hosted.job.as_ref().unwrap().id, publish.id);
    assert!(app.hosted.issue.is_none());
    assert!(app.hosted.active_post.as_ref().unwrap().preview.is_none());

    // Even a response for the same job ID belongs to the connection that sent it.
    drop(app.update(AppMessage::Hosted(Message::Disconnect)));
    app.hosted.client = Some(Client::new(ORIGIN, "test".into()).unwrap());
    drop(app.watch_hosted_job(publish.clone()));
    drop(app.update(finish(
        connection,
        publish.id,
        Err("Old connection failed".into()),
    )));
    assert_eq!(app.hosted.job.as_ref().unwrap().id, publish.id);
    assert!(app.hosted.issue.is_none());

    let completed_publish = Job {
        state: JobState::Succeeded,
        ..publish
    };
    drop(app.update(finish(
        app.hosted.connection,
        completed_publish.id,
        Ok(completed_publish),
    )));
    assert_eq!(app.hosted.job.as_ref().unwrap().state, JobState::Succeeded);
}
