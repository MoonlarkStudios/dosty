use std::path::{Path, PathBuf};

use iced::futures::StreamExt;
use iced::window;

use super::{AccessKey, App, AppMessage, Client, Details, HostedAddress, HostedDraft, Message};
use crate::app::application::{Overlay, recovery};
use crate::document::Document;
use crate::document::selection::Selection;
use crate::io::{recovery as store, workspace_document::WorkspaceDocument};

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
    app.install_document(
        Document::new("Saved body.".into()),
        None,
        "Saved title".into(),
    );
    let address = HostedAddress {
        origin: "https://workspace.example.invalid/".into(),
        id: uuid::Uuid::new_v4(),
    };
    let details = Details {
        title: "Saved title".into(),
        section: "essays".into(),
        cover: None,
    };
    app.session.backing = WorkspaceDocument::Hosted(HostedDraft {
        address: address.clone(),
        revision: 7,
        saved_details: details.clone(),
        details: details.clone(),
    });
    let initial = app.session.recovery_version();
    send(
        &mut app,
        AppMessage::Hosted(Message::Title("Unsaved title".into())),
    )
    .await;
    assert_eq!(app.session.recovery_version().0, initial.0);
    assert_ne!(app.session.recovery_version(), initial);
    assert!(app.session.dirty);
    keep_and_close(&mut app).await;

    let mut reopened = recover(directory.path()).await;
    let draft = reopened.session.hosted().unwrap();
    assert_eq!(draft.address, address);
    assert_eq!(draft.revision, 7);
    assert_eq!(draft.saved_details, details);
    assert_eq!(draft.details.title, "Unsaved title");
    assert_eq!(reopened.session.document.source(), "Saved body.");
    assert!(reopened.session.dirty);
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
            assert!(app.session.dirty);
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
            assert!(!app.session.dirty, "{:?}", app.hosted.issue);
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
