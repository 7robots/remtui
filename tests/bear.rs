//! Integration tests: `BearClient` driving the fake bearcli subprocess.

mod common;

use common::Fake;
use remtui::bear::BearClient;

#[tokio::test]
async fn search_todo_notes_collects_every_open_todo() {
    let fake = Fake::new();
    let scan = fake.bear().search_todo_notes(&[]).await.unwrap();
    let texts: Vec<&str> = scan.todos.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "write the release notes",
            "ask Priya about the API deprecation",
            "confirm the sunset date",
            "order bulbs for the front bed",
            "move the hydrangea",
        ]
    );
    assert_eq!(scan.locked, 1);
    let hydrangea = &scan.todos[4];
    assert_eq!(hydrangea.section, "## Next spring");
    assert_eq!(hydrangea.note_title, "Garden Plan");
    assert_eq!(hydrangea.note_tags, vec!["#home", "#home/garden"]);
    assert_eq!(scan.todos[2].line, "  - [ ] confirm the sunset date");
}

#[tokio::test]
async fn search_todo_notes_scopes_by_tag() {
    let fake = Fake::new();
    let scan = fake
        .bear()
        .search_todo_notes(&["work".to_string()])
        .await
        .unwrap();
    assert!(scan.todos.iter().all(|t| t.note_id == "NOTE-PLANNING"));
    assert_eq!(scan.todos.len(), 3);
    assert_eq!(scan.locked, 0);
    let both = fake
        .bear()
        .search_todo_notes(&["#work".to_string(), "home/garden".to_string()])
        .await
        .unwrap();
    assert_eq!(both.todos.len(), 5);
}

#[tokio::test]
async fn complete_todo_flips_only_that_line() {
    let fake = Fake::new();
    let bear = fake.bear();
    let scan = bear.search_todo_notes(&[]).await.unwrap();
    let todo = scan
        .todos
        .iter()
        .find(|t| t.text == "write the release notes")
        .unwrap()
        .clone();
    bear.complete_todo(&todo).await.unwrap();
    let state = fake.read_bear_state();
    let content = state["notes"][0]["content"].as_str().unwrap();
    assert!(content.contains("- [x] write the release notes"));
    assert!(content.contains("- [ ] ask Priya about the API deprecation"));
    // second time: the line is gone
    let err = bear.complete_todo(&todo).await.unwrap_err();
    assert!(err.message.starts_with("Text not found"));
    let after = bear.search_todo_notes(&[]).await.unwrap();
    assert_eq!(after.todos.len(), 4);
}

#[tokio::test]
async fn open_note_passes_the_header() {
    let fake = Fake::new();
    let bear = fake.bear();
    let scan = bear.search_todo_notes(&[]).await.unwrap();
    let todo = scan
        .todos
        .iter()
        .find(|t| t.text == "move the hydrangea")
        .unwrap()
        .clone();
    bear.open_note(&todo).await.unwrap();
    let bulbs = scan
        .todos
        .iter()
        .find(|t| t.text == "order bulbs for the front bed")
        .unwrap()
        .clone();
    bear.open_note(&bulbs).await.unwrap();
    let state = fake.read_bear_state();
    assert_eq!(
        state["opened"],
        serde_json::json!([
            {"id": "NOTE-GARDEN", "header": "Next spring"},
            {"id": "NOTE-GARDEN", "header": "Garden Plan"}
        ])
    );
}

#[tokio::test]
async fn errors_and_missing_binary() {
    let fake = Fake::new();
    let err = BearClient::new(vec!["definitely-not-bearcli-xyz".into()])
        .search_todo_notes(&[])
        .await
        .unwrap_err();
    assert_eq!(err.code, "not_found");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_fake-bearcli"))
        .args(["cat", "NOTE-LOCKED", "--format", "json"])
        .env("REMTUI_FAKE_BEAR_STATE", fake.bear_state())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(payload["error"]["code"], "locked");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_fake-bearcli"))
        .args([
            "search",
            "@todo",
            "--format",
            "json",
            "--fields",
            "id,title,tags,locked,content",
        ])
        .env("REMTUI_FAKE_BEAR_STATE", fake.bear_state())
        .output()
        .unwrap();
    let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 3);
    assert_eq!(rows[2]["locked"], "yes");
    assert!(rows[2]["content"].is_null());
}
