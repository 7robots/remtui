//! UI tests for the writes: the form, delete confirm, done, flag, priority.

mod common;

use std::collections::HashMap;

use common::{Fake, SIZE};
use remtui::app::{Pane, ViewKind};
use remtui::config::Config;
use remtui::harness::Harness;
use remtui::models::Priority;

/// Open the Personal list with the reminders pane focused.
async fn personal(h: &mut Harness) {
    h.load().await;
    for _ in 0..4 {
        h.press("j");
    }
    h.reloaded().await;
    h.press("l");
    assert_eq!(h.app.view, ViewKind::List(1));
    assert_eq!(h.app.focus, Pane::Reminders);
}

fn overlay(h: &Harness) -> Option<&'static str> {
    h.app.overlay.as_ref().map(|o| o.name())
}

#[tokio::test]
async fn add_via_form_creates_a_reminder() {
    let fake = Fake::new();
    let mut h = fake.harness();
    personal(&mut h).await;
    let before = h.shown_titles().len();
    h.press("a");
    assert_eq!(overlay(&h), Some("form"));
    let screen = h.text();
    assert!(screen.contains("New Reminder"), "{screen}");
    assert!(
        screen.contains("Personal"),
        "default list is the current one"
    );
    assert!(
        screen.contains("esc Cancel") && screen.contains("^s Save") && screen.contains("^e Editor")
    );
    h.type_text("Wash the car");
    h.press("tab"); // notes
    h.type_text("Use the good soap");
    h.press("tab"); // due
    h.type_text("tomorrow 09:30");
    h.press("tab"); // priority
    h.press("space");
    h.press("space"); // medium
    h.press("tab"); // list
    h.press("tab"); // flag
    h.press("space");
    h.press("ctrl+s");
    h.until(|app| app.overlay.is_none()).await;
    h.reloaded().await;
    h.until(|app| app.shown.len() == before + 1).await;
    assert!(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m == "＋ Reminder added")
    );
    let added = h
        .app
        .reminders
        .iter()
        .find(|r| r.title == "Wash the car")
        .unwrap();
    assert_eq!(added.priority, Priority::Medium);
    assert!(added.flagged);
    assert_eq!(added.notes, "Use the good soap");
    assert!(added.due().is_some() && !added.all_day);
    // the sidebar count followed
    h.until(|app| app.lists[0].active as usize == before + 1)
        .await;
    assert!(h.text().contains("5 active"));
}

#[tokio::test]
async fn enter_saves_and_a_double_enter_creates_one() {
    let fake = Fake::new();
    let mut h = fake.harness();
    personal(&mut h).await;
    let before = h.shown_titles().len();
    h.press("n");
    h.type_text("Only once");
    h.press("enter");
    h.press("enter");
    h.until(|app| app.overlay.is_none()).await;
    h.reloaded().await;
    h.settle().await;
    h.reloaded().await;
    let count = h
        .app
        .reminders
        .iter()
        .filter(|r| r.title == "Only once")
        .count();
    assert_eq!(count, 1);
    assert_eq!(h.shown_titles().len(), before + 1);
}

#[tokio::test]
async fn empty_title_is_rejected_and_escape_cancels() {
    let fake = Fake::new();
    let mut h = fake.harness();
    personal(&mut h).await;
    h.press("a");
    h.press("ctrl+s");
    assert_eq!(overlay(&h), Some("form"));
    assert!(h.text().contains("⚠ A title is required."));
    // main-screen keys do not leak through the modal
    h.press("q");
    assert!(h.app.running);
    h.press("escape");
    assert!(h.app.overlay.is_none());
    h.settle().await;
    assert_eq!(h.shown_titles().len(), 4);
}

#[tokio::test]
async fn edit_sends_changed_fields_and_keeps_selection() {
    let fake = Fake::new();
    let mut h = fake.harness();
    personal(&mut h).await;
    h.press("j"); // second row
    let target = h.app.selected().unwrap().clone();
    h.press("e");
    assert_eq!(overlay(&h), Some("form"));
    assert!(h.text().contains("Edit Reminder"));
    assert_eq!(h.app.overlay.as_ref().map(|o| o.name()), Some("form"));
    h.press("end");
    h.type_text(" (edited)");
    h.press("ctrl+s");
    h.until(|app| app.overlay.is_none()).await;
    h.reloaded().await;
    assert!(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m == "✎ Reminder updated")
    );
    let edited = h.app.reminders.iter().find(|r| r.id == target.id).unwrap();
    assert_eq!(edited.title, format!("{} (edited)", target.title));
    assert_eq!(edited.notes, target.notes, "untouched fields stay");
    assert_eq!(h.app.selected().map(|r| r.id), Some(target.id));
    // enter on a row also opens the editor
    h.press("enter");
    assert_eq!(overlay(&h), Some("form"));
    h.press("escape");
}

#[tokio::test]
async fn form_flag_change_dismisses_and_writes_in_the_background() {
    let fake = Fake::new();
    let mut h = fake.harness();
    personal(&mut h).await;
    let target = h.app.selected().unwrap().clone();
    assert!(target.flagged);
    h.press("e");
    for _ in 0..5 {
        h.press("tab");
    }
    h.press("space"); // unflag
    h.press("ctrl+s");
    assert!(
        h.app.overlay.is_none(),
        "a flag-only change closes the form at once"
    );
    assert!(h.app.is_pending(target.id));
    assert!(
        !h.app
            .reminders
            .iter()
            .find(|r| r.id == target.id)
            .unwrap()
            .flagged
    );
    h.until(|app| !app.is_pending(target.id)).await;
    h.reloaded().await;
    assert!(
        !h.app
            .reminders
            .iter()
            .find(|r| r.id == target.id)
            .unwrap()
            .flagged
    );
}

#[tokio::test]
async fn space_toggles_done_and_selection_stays_put() {
    let fake = Fake::new();
    let mut h = fake.harness();
    personal(&mut h).await;
    h.press("j");
    let target = h.app.selected().unwrap().clone();
    h.press("space");
    h.until(|app| {
        app.toasts
            .iter()
            .any(|t| t.message.starts_with("✓ Completed"))
    })
    .await;
    h.reloaded().await;
    h.until(|app| app.shown.len() == 3).await;
    assert!(
        h.app.reminders.iter().all(|r| r.id != target.id),
        "hidden once completed"
    );
    assert_eq!(h.app.cursor, 1, "the cursor stays at the old index");
    h.until(|app| app.lists[0].active == 3).await;
    assert!(h.text().contains("3 active · 2 done"), "{}", h.text());
    // show completed, reopen it
    h.press("c");
    h.reloaded().await;
    let y = h.find_row(&target.title).unwrap();
    h.press("g");
    while h.app.selected().map(|r| r.id) != Some(target.id) {
        h.press("j");
    }
    assert!(h.row(y).contains('⬤'));
    h.press("space");
    h.until(|app| {
        app.toasts
            .iter()
            .any(|t| t.message.starts_with("↺ Reopened"))
    })
    .await;
    h.reloaded().await;
    assert!(
        !h.app
            .reminders
            .iter()
            .find(|r| r.id == target.id)
            .unwrap()
            .completed
    );
}

#[tokio::test]
async fn delete_asks_first_and_cancel_is_the_safe_default() {
    let fake = Fake::new();
    let mut h = fake.harness();
    personal(&mut h).await;
    let target = h.app.selected().unwrap().clone();
    h.press("d");
    assert_eq!(overlay(&h), Some("confirm"));
    assert!(h.text().contains("Delete Reminder"));
    assert!(
        h.text()
            .contains(&format!("Delete “{}” from Personal?", target.title))
    );
    h.press("enter"); // Cancel has focus
    assert!(h.app.overlay.is_none());
    h.settle().await;
    assert!(h.app.reminders.iter().any(|r| r.id == target.id));
    h.press("backspace");
    h.press("n");
    assert!(h.app.overlay.is_none());
    h.press("delete");
    h.press("y");
    h.until(|app| {
        app.toasts
            .iter()
            .any(|t| t.message.starts_with("🗑 Deleted"))
    })
    .await;
    h.reloaded().await;
    h.until(|app| app.reminders.iter().all(|r| r.id != target.id))
        .await;
    assert_eq!(h.app.cursor, 0);
    // tab moves focus to the Delete button; enter then deletes
    let next = h.app.selected().unwrap().clone();
    h.press("d");
    h.press("tab");
    h.press("enter");
    h.until(|app| app.reminders.iter().all(|r| r.id != next.id))
        .await;
}

#[tokio::test]
async fn flag_toggles_optimistically_then_lands() {
    let fake = Fake::new();
    let mut h = fake.harness();
    personal(&mut h).await;
    h.press("j");
    let target = h.app.selected().unwrap().clone();
    assert!(!target.flagged);
    h.press("f");
    assert!(h.app.is_pending(target.id));
    assert!(
        h.app.selected().unwrap().flagged,
        "flipped before remctl answers"
    );
    let y = h.find_row(&target.title).unwrap();
    assert!(
        h.row(y).starts_with("◌") || h.row(y).contains("◌"),
        "{}",
        h.row(y)
    );
    h.until(|app| !app.is_pending(target.id)).await;
    h.reloaded().await;
    assert!(
        h.app
            .reminders
            .iter()
            .find(|r| r.id == target.id)
            .unwrap()
            .flagged
    );
    assert!(
        h.app.toast_messages().is_empty(),
        "no toast for a flag write"
    );
    // the Flagged smart view now lists it
    h.press("h");
    h.press("k");
    h.reloaded().await;
    assert!(h.shown_titles().contains(&target.title));
}

#[tokio::test]
async fn failed_flag_write_reverts_the_row() {
    let fake = Fake::new();
    let mut h = Harness::new(
        Config::default(),
        std::sync::Arc::new(fake.client_failing_flags()),
        false,
        SIZE,
    );
    h.start();
    personal(&mut h).await;
    h.press("j");
    let target = h.app.selected().unwrap().clone();
    h.press("f");
    assert!(h.app.selected().unwrap().flagged);
    h.until(|app| !app.toasts.is_empty()).await;
    assert!(!h.app.selected().unwrap().flagged, "reverted");
    assert!(!h.app.is_pending(target.id));
    assert!(h.text().contains("Could not set the flag"));
    // add --flag: created, with the warning surfaced
    h.press("a");
    h.type_text("Created but unflagged");
    for _ in 0..5 {
        h.press("tab");
    }
    h.press("space");
    h.press("ctrl+s");
    h.until(|app| app.overlay.is_none()).await;
    assert!(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m.starts_with("flag_not_set:"))
    );
    h.reloaded().await;
    h.until(|app| {
        app.reminders
            .iter()
            .any(|r| r.title == "Created but unflagged")
    })
    .await;
}

#[tokio::test]
async fn priority_cycles_with_p() {
    let fake = Fake::new();
    let mut h = fake.harness();
    personal(&mut h).await;
    h.press("G");
    let target = h.app.selected().unwrap().clone();
    assert_eq!(target.priority, Priority::None);
    h.press("p");
    h.until(|app| app.toasts.iter().any(|t| t.message == "Priority → low"))
        .await;
    h.reloaded().await;
    h.until(|app| {
        app.reminders
            .iter()
            .find(|r| r.id == target.id)
            .unwrap()
            .priority
            == Priority::Low
    })
    .await;
    assert_eq!(h.app.selected().map(|r| r.id), Some(target.id));
}

#[tokio::test]
async fn ctrl_e_edits_the_notes_in_the_editor() {
    let fake = Fake::new();
    let script = fake.dir.path().join("editor.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf 'from the editor\\nline two' > \"$1\"\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = HashMap::new();
    env.insert("EDITOR".to_string(), script.to_string_lossy().into_owned());
    let mut h = Harness::with_env(Config::default(), fake.shared_client(), false, SIZE, env);
    h.start();
    personal(&mut h).await;
    h.press("a");
    h.type_text("With notes");
    h.press("ctrl+e");
    let notes = match h.app.overlay.as_ref() {
        Some(remtui::app::Overlay::Form(form)) => form.notes.text.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(notes, "from the editor\nline two");
    assert!(h.text().contains("from the editor"));
    h.press("ctrl+s");
    h.until(|app| app.overlay.is_none()).await;
    h.reloaded().await;
    h.until(|app| app.reminders.iter().any(|r| r.title == "With notes"))
        .await;
    assert_eq!(
        h.app
            .reminders
            .iter()
            .find(|r| r.title == "With notes")
            .unwrap()
            .notes,
        "from the editor\nline two"
    );
}

#[tokio::test]
async fn add_without_lists_warns() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.press("a");
    assert!(h.app.overlay.is_none());
    assert!(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m == "No lists loaded yet.")
    );
}
