//! New pages: where they belong, how they are titled, and their first save.

use std::path::{Path, PathBuf};

use iced::Task;
use iced::widget::text_editor::{Action, Edit};

use super::{App, Message, Overlay, folder};
use crate::io::file_ops;
use crate::io::folder as discovery;

async fn settle(app: &mut App, task: Task<Message>) {
    use iced::futures::StreamExt;
    let mut tasks = vec![task];
    while let Some(task) = tasks.pop() {
        let Some(mut effects) = iced_runtime::task::into_stream(task) else {
            continue;
        };
        while let Some(effect) = effects.next().await {
            if let iced_runtime::Action::Output(message) = effect {
                tasks.push(app.update(message));
            }
        }
    }
}

fn paste(app: &mut App, text: &str) {
    drop(app.update(Message::Edit(Action::Edit(Edit::Paste(
        std::sync::Arc::new(text.into()),
    )))));
}

fn place(app: &mut App, root: &Path) {
    drop(app.update_folder(folder::Message::Scanned {
        generation: 0,
        result: Ok(discovery::Snapshot {
            root: root.to_owned(),
            entries: Vec::new(),
            limited: false,
            warnings: Vec::new(),
        }),
    }));
}

#[test]
fn a_new_page_belongs_beside_the_document_the_writer_was_in() {
    let (mut app, _task) = App::new();
    drop(app.update(Message::New));
    assert_eq!(app.session.folder, None, "nowhere to belong yet");

    place(&mut app, Path::new("/writing"));
    drop(app.update(Message::New));
    assert_eq!(app.session.folder.as_deref(), Some(Path::new("/writing")));

    app.set_document(
        "A post".into(),
        Some("/blog/content/posts/a.md".into()),
        "a.md".into(),
    );
    drop(app.update(Message::New));
    let posts = Some(Path::new("/blog/content/posts"));
    assert_eq!(app.session.folder.as_deref(), posts);
    assert!(app.overlay.is_none(), "no name is asked for");
    paste(&mut app, "# Second post");
    assert_eq!(app.session.display_name, "Second post.md");

    drop(app.update(Message::New));
    assert_eq!(
        app.session.folder.as_deref(),
        posts,
        "a page started from a draft stays in that draft's folder"
    );
}

#[tokio::test]
async fn the_first_save_writes_a_draft_into_its_folder_without_replacing_anything() {
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let existing = root.join("First thoughts.md");
    std::fs::write(&existing, "Already here.").unwrap();
    let (mut app, _task) = App::new();
    place(&mut app, &root);

    drop(app.update(Message::New));
    paste(&mut app, "# First thoughts\n\nA beginning.");
    let task = app.update(Message::Save);
    settle(&mut app, task).await;
    let saved = root.join("First thoughts 2.md");
    assert_eq!(app.session.path.as_ref(), Some(&saved));
    assert_eq!(app.session.display_name, "First thoughts 2.md");
    assert!(!app.session.dirty && app.session.folder.is_none());
    assert_eq!(
        std::fs::read_to_string(&saved).unwrap(),
        "# First thoughts\n\nA beginning."
    );
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "Already here.");

    // Later saves go to the same file; its title no longer follows the text.
    paste(&mut app, " More.");
    assert_eq!(app.session.display_name, "First thoughts 2.md");
    let task = app.update(Message::Save);
    settle(&mut app, task).await;
    assert!(!app.session.dirty);
    assert!(std::fs::read_to_string(&saved).unwrap().ends_with(" More."));

    // A name the writer chose is used as written.
    drop(app.update(Message::New));
    paste(&mut app, "Configure runners");
    drop(app.update_folder(folder::Message::NameDraft));
    assert_eq!(app.overlay, Some(Overlay::Folder));
    drop(app.update_folder(folder::Message::Value("configure_runners".into())));
    drop(app.update_folder(folder::Message::Accept));
    assert!(app.overlay.is_none());
    paste(&mut app, " on your own machines");
    assert_eq!(app.session.display_name, "configure_runners.md");
    let task = app.update(Message::Save);
    settle(&mut app, task).await;
    assert_eq!(
        app.session.path.as_ref(),
        Some(&root.join("configure_runners.md"))
    );
}

#[tokio::test]
async fn a_draft_whose_folder_is_gone_reports_it_and_keeps_the_writing() {
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let (mut app, _task) = App::new();
    place(&mut app, &root.join("removed"));
    drop(app.update(Message::New));
    paste(&mut app, "Kept");
    let task = app.update(Message::Save);
    settle(&mut app, task).await;
    assert!(app.session.path.is_none() && app.session.dirty);
    assert!(app.notification.is_some());
    assert_eq!(app.session.document.source(), "Kept");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
}

#[tokio::test]
async fn unique_names_are_numbered_and_only_untouched_reservations_are_discarded() {
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let names = |root: &Path| {
        let mut names: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            // Dosty's own folder lock is hidden from the writer.
            .filter(|name| !name.starts_with('.'))
            .collect();
        names.sort();
        names
    };
    let mut created: Vec<PathBuf> = Vec::new();
    for _ in 0..3 {
        created.push(
            file_ops::create_unique_document(root.clone(), "Note.md".into())
                .await
                .unwrap(),
        );
    }
    assert_eq!(names(&root), ["Note 2.md", "Note 3.md", "Note.md"]);

    std::fs::write(&created[1], "Written since.").unwrap();
    for path in created {
        file_ops::discard_empty_document(path).await;
    }
    assert_eq!(names(&root), ["Note 2.md"]);
    assert!(
        file_ops::create_unique_document(root.join("missing"), "Note.md".into())
            .await
            .is_err()
    );
}
