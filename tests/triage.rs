//! UI tests for the Bear triage screen, on both fakes.

mod common;

use std::sync::Arc;

use common::{Fake, SIZE};
use remtui::app::Overlay;
use remtui::config::Config;
use remtui::harness::Harness;
use remtui::triage::{Status, link_key};

fn bear_config() -> Config {
    let mut config = Config::default();
    config.bear.enabled = true;
    config.bear.list = "Work".into();
    config.bear.due = "today".into();
    config
}

/// A harness with the fake bearcli installed and the app loaded.
async fn triage_harness(fake: &Fake) -> Harness {
    let mut h = Harness::new(bear_config(), fake.shared_client(), false, SIZE);
    h.app.bear = Some(Arc::new(fake.bear()));
    h.start();
    h.load().await;
    h
}

async fn open_triage(h: &mut Harness) {
    h.press("b");
    h.until(|app| app.triage().is_some_and(|t| t.loaded)).await;
}

fn triage_rows(h: &Harness) -> Vec<(String, Status)> {
    h.app
        .triage()
        .unwrap()
        .rows
        .iter()
        .map(|r| (r.todo.text.clone(), r.status()))
        .collect()
}

#[tokio::test]
async fn b_is_inert_when_disabled() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    h.press("b");
    h.settle().await;
    assert!(h.app.overlay.is_none());
    assert!(!h.text().contains(" b Bear"));
}

#[tokio::test]
async fn rows_are_grouped_by_note_with_stats() {
    let fake = Fake::new();
    let mut h = triage_harness(&fake).await;
    open_triage(&mut h).await;
    let triage = h.app.triage().unwrap();
    assert_eq!(triage.lines.len(), 8);
    assert_eq!(triage.cursor, 1);
    assert_eq!(
        triage.stats(),
        "5 todos in 2 notes · 0 added · 0 done · 1 locked skipped · add → Work, due today"
    );
    let screen = h.text();
    assert!(screen.contains("🐻 Bear Todos"), "{screen}");
    assert!(
        screen.contains("Sprint Planning  #work #work/sprint"),
        "{screen}"
    );
    assert!(screen.contains("Garden Plan"), "{screen}");
    assert!(screen.contains("◯ write the release notes"), "{screen}");
    assert!(
        screen.contains("  ◯ confirm the sunset date"),
        "nested todos keep their indent\n{screen}"
    );
    assert!(screen.contains("esc Close") && screen.contains("space Mark"));
}

#[tokio::test]
async fn mark_and_add_creates_linked_reminders() {
    let fake = Fake::new();
    let mut h = triage_harness(&fake).await;
    open_triage(&mut h).await;
    h.press("space"); // write the release notes
    h.press("j");
    h.press("space"); // ask Priya
    assert!(h.text().contains("2 marked"));
    assert!(h.text().contains("◆ write the release notes"));
    h.press("enter");
    h.until(|app| app.triage().is_some_and(|t| !t.adding && t.added_any))
        .await;
    assert!(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m == "＋ 2 added to Work")
    );
    let rows = triage_rows(&h);
    assert_eq!(rows[0], ("write the release notes".into(), Status::Added));
    assert_eq!(
        rows[1],
        ("ask Priya about the API deprecation".into(), Status::Added)
    );
    assert_eq!(rows[2].1, Status::New);
    assert!(h.text().contains("⏰ Today"), "{}", h.text());
    assert!(h.app.triage().unwrap().marked.is_empty());
    // the reminders carry the link
    let work = fake.client().get_reminders("Work", false).await.unwrap();
    let linked: Vec<_> = work
        .iter()
        .filter(|r| !link_key(&r.notes).is_empty())
        .collect();
    assert_eq!(linked.len(), 2);
    let key = h.app.triage().unwrap().rows[0].todo.key();
    assert!(
        linked
            .iter()
            .any(|r| link_key(&r.notes) == key && r.title == "write the release notes")
    );
    assert!(
        linked[0]
            .notes
            .contains("bear://x-callback-url/open-note?id=NOTE-PLANNING")
    );
    // closing after an add reloads the panel
    h.press("escape");
    assert!(h.app.overlay.is_none());
    assert!(h.app.loading);
    h.reloaded().await;
}

#[tokio::test]
async fn enter_without_marks_adds_the_current_row_only() {
    let fake = Fake::new();
    let mut h = triage_harness(&fake).await;
    open_triage(&mut h).await;
    h.press("j");
    h.press("j");
    h.press("j"); // order bulbs (Garden Plan)
    assert_eq!(
        h.app.triage().unwrap().current_row().unwrap().todo.text,
        "order bulbs for the front bed"
    );
    h.press("ctrl+s");
    h.until(|app| app.triage().is_some_and(|t| !t.adding && t.added_any))
        .await;
    let rows = triage_rows(&h);
    assert_eq!(rows.iter().filter(|(_, s)| *s == Status::Added).count(), 1);
    assert_eq!(
        rows[3],
        ("order bulbs for the front bed".into(), Status::Added)
    );
    // the cursor stayed on it; space on an added row is refused
    assert_eq!(
        h.app.triage().unwrap().current_row().unwrap().todo.text,
        "order bulbs for the front bed"
    );
    h.press("space");
    assert!(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m == "Already in Reminders")
    );
    h.press("enter");
    assert!(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m == "Mark a todo first (space)")
    );
}

#[tokio::test]
async fn done_status_after_remctl_done_and_reload() {
    let fake = Fake::new();
    let mut h = triage_harness(&fake).await;
    open_triage(&mut h).await;
    h.press("enter");
    h.until(|app| app.triage().is_some_and(|t| !t.adding && t.added_any))
        .await;
    let work = fake.client().get_reminders("Work", false).await.unwrap();
    let added = work
        .iter()
        .find(|r| r.title == "write the release notes")
        .unwrap();
    fake.client().done(added.id).await.unwrap();
    h.press("r");
    h.until(|app| {
        app.triage()
            .is_some_and(|t| t.rows[0].status() == Status::Done)
    })
    .await;
    assert!(h.text().contains("✓ done"));
    assert!(h.text().contains("1 done"));
    // x ticks it in Bear after a confirm; cancel first
    h.press("x");
    assert_eq!(h.app.overlay.as_ref().map(|o| o.name()), Some("confirm"));
    assert!(h.text().contains("Tick in Bear"));
    h.press("n");
    assert_eq!(h.app.overlay.as_ref().map(|o| o.name()), Some("triage"));
    assert_eq!(triage_rows(&h).len(), 5);
    h.press("x");
    h.press("y");
    h.until(|app| app.triage().is_some_and(|t| t.rows.len() == 4))
        .await;
    assert!(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m == "✓ Ticked “write the release notes” in Bear")
    );
    let state = fake.read_bear_state();
    let content = state["notes"][0]["content"].as_str().unwrap();
    assert!(content.contains("- [x] write the release notes"));
    assert!(content.contains("- [ ] ask Priya"));
}

#[tokio::test]
async fn x_does_nothing_on_new_or_added_rows() {
    let fake = Fake::new();
    let mut h = triage_harness(&fake).await;
    open_triage(&mut h).await;
    h.press("x");
    assert_eq!(h.app.overlay.as_ref().map(|o| o.name()), Some("triage"));
    assert!(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m.starts_with("Only a todo whose reminder is done"))
    );
    h.press("enter");
    h.until(|app| app.triage().is_some_and(|t| !t.adding && t.added_any))
        .await;
    h.press("x");
    assert_eq!(h.app.overlay.as_ref().map(|o| o.name()), Some("triage"));
    let state = fake.read_bear_state();
    assert!(
        !state["notes"][0]["content"]
            .as_str()
            .unwrap()
            .contains("[x] write")
    );
}

#[tokio::test]
async fn filter_then_double_escape() {
    let fake = Fake::new();
    let mut h = triage_harness(&fake).await;
    open_triage(&mut h).await;
    h.press("slash");
    h.type_text("hydr");
    let triage = h.app.triage().unwrap();
    assert_eq!(triage.lines.len(), 2);
    assert!(triage.stats().contains("filter \"hydr\" → 1"));
    h.press("enter");
    assert!(!h.app.triage().unwrap().filter_focused);
    assert_eq!(
        h.app.triage().unwrap().current_row().unwrap().todo.text,
        "move the hydrangea"
    );
    h.press("escape");
    let triage = h.app.triage().unwrap();
    assert!(triage.filter.is_none() && triage.filter_text.is_empty());
    assert_eq!(triage.lines.len(), 8);
    h.press("escape");
    assert!(h.app.overlay.is_none());
    assert!(!h.app.loading, "nothing added, no reload");
}

#[tokio::test]
async fn o_opens_the_note_at_its_section() {
    let fake = Fake::new();
    let mut h = triage_harness(&fake).await;
    open_triage(&mut h).await;
    for _ in 0..4 {
        h.press("j");
    }
    assert_eq!(
        h.app.triage().unwrap().current_row().unwrap().todo.text,
        "move the hydrangea"
    );
    h.press("o");
    h.until(|_| {
        std::fs::read_to_string(fake.bear_state())
            .map(|s| s.contains("\"opened\""))
            .unwrap_or(false)
    })
    .await;
    let state = fake.read_bear_state();
    assert_eq!(
        state["opened"],
        serde_json::json!([{"id": "NOTE-GARDEN", "header": "Next spring"}])
    );
}

#[tokio::test]
async fn main_keys_are_gated_and_help_mentions_bear() {
    let fake = Fake::new();
    let mut h = triage_harness(&fake).await;
    open_triage(&mut h).await;
    let view = h.app.view;
    h.press("q");
    assert!(h.app.running);
    h.press("h");
    h.press("k");
    assert_eq!(h.app.view, view);
    h.press("escape");
    h.press("question_mark");
    assert!(matches!(h.app.overlay, Some(Overlay::Help { .. })));
    assert!(
        remtui::ui::help::text(true)
            .contains("b                   review open todos from Bear notes")
    );
}

#[tokio::test]
async fn missing_bearcli_reports_and_stays() {
    let fake = Fake::new();
    let mut config = bear_config();
    config.bear.bearcli = "/definitely/not/here/bearcli".into();
    let mut h = Harness::new(config, fake.shared_client(), false, SIZE);
    h.start();
    h.load().await;
    h.press("b");
    assert!(h.app.overlay.is_none());
    assert!(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m.starts_with("bearcli not found (/definitely/not/here/bearcli)"))
    );
}

#[tokio::test]
async fn target_list_falls_back_to_the_current_list() {
    let fake = Fake::new();
    let mut config = bear_config();
    config.bear.list = String::new();
    let mut h = Harness::new(config, fake.shared_client(), false, SIZE);
    h.app.bear = Some(Arc::new(fake.bear()));
    h.start();
    h.load().await;
    for _ in 0..4 {
        h.press("j");
    }
    h.reloaded().await;
    open_triage(&mut h).await;
    assert_eq!(h.app.triage().unwrap().target_list, "Personal");
    assert!(h.text().contains("add → Personal, due today"));
}
