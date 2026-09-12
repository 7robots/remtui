//! Integration tests: `RemctlClient` driving the fake remctl subprocess.

mod common;

use chrono::{Datelike, Timelike};
use common::{Fake, numeric_id, status};
use remtui::client::{AddFields, EditFields, RemctlClient};
use remtui::models::Priority;
use remtui::todos::Todo;
use remtui::triage::{link_key, link_notes};

fn add(title: &str, list: &str) -> AddFields {
    AddFields {
        title: title.into(),
        list_title: list.into(),
        ..AddFields::default()
    }
}

async fn find(client: &RemctlClient, list: &str, id: i64) -> Option<remtui::models::Reminder> {
    client
        .get_reminders(list, false)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.id == id)
}

#[tokio::test]
async fn get_lists() {
    let fake = Fake::new();
    let lists = fake.client().get_lists().await.unwrap();
    let titles: Vec<&str> = lists.iter().map(|l| l.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["Personal", "Work", "Groceries", "Reading", "Home"]
    );
    let personal = &lists[0];
    assert_eq!(personal.color_hex, "#007AFF");
    assert!(personal.active > 0);
    let groceries = &lists[2];
    assert!(groceries.is_groceries);
    assert_eq!(groceries.emoji, "🛒");
}

#[tokio::test]
async fn get_reminders_excludes_completed_by_default() {
    let fake = Fake::new();
    let client = fake.client();
    let active = client.get_reminders("Personal", false).await.unwrap();
    assert!(active.iter().all(|r| !r.completed));
    let everything = client.get_reminders("Personal", true).await.unwrap();
    assert!(everything.len() > active.len());
    assert!(everything.iter().any(|r| r.completed));
    let passport = everything
        .iter()
        .find(|r| r.title.to_lowercase().contains("passport"))
        .unwrap();
    assert_eq!(passport.priority, Priority::High);
    assert!(passport.flagged && passport.all_day && passport.due().is_some());
    assert!(passport.tags.contains(&"errands".to_string()));
    assert!(!passport.notes.is_empty());
}

#[tokio::test]
async fn smart_views_and_search() {
    let fake = Fake::new();
    let client = fake.client();
    let today = client.today().await.unwrap();
    assert!(!today.is_empty() && today.iter().all(|r| !r.completed));
    let overdue = client.overdue().await.unwrap();
    assert!(
        overdue
            .iter()
            .any(|r| r.title.to_lowercase().contains("passport"))
    );
    let flagged = client.flagged().await.unwrap();
    assert!(flagged.iter().all(|r| r.flagged));
    let upcoming = client.upcoming(30).await.unwrap();
    assert!(upcoming.len() >= today.len());
    let hits = client.search("dentist", false).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "Call the dentist");
    // "-milk" is a query, not an option
    assert!(client.search("-milk", false).await.unwrap().is_empty());
}

#[tokio::test]
async fn add_edit_done_delete_roundtrip() {
    let fake = Fake::new();
    let client = fake.client();
    let result = client
        .add(&AddFields {
            title: "Wash the car".into(),
            list_title: "Home".into(),
            notes: "Use the good soap".into(),
            due: "tomorrow 09:30".into(),
            priority: "medium".into(),
            flagged: true,
            tags: "chores,car".into(),
            url: String::new(),
        })
        .await
        .unwrap();
    assert_eq!(status(&result), "created");
    let id = numeric_id(&result);
    let added = find(&client, "Home", id).await.unwrap();
    assert_eq!(added.priority, Priority::Medium);
    assert!(added.flagged);
    assert!(added.due().is_some() && !added.all_day);
    assert!(added.tags.contains(&"chores".to_string()));

    let result = client
        .edit(
            id,
            &EditFields {
                title: Some("Wash & wax the car".into()),
                priority: Some("high".into()),
                due: Some(String::new()),
                ..EditFields::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(status(&result), "updated");
    let edited = find(&client, "Home", id).await.unwrap();
    assert_eq!(edited.title, "Wash & wax the car");
    assert_eq!(edited.priority, Priority::High);
    assert!(edited.due().is_none()); // due "" means clear

    assert_eq!(status(&client.done(id).await.unwrap()), "completed");
    assert_eq!(status(&client.undone(id).await.unwrap()), "uncompleted");
    assert_eq!(status(&client.unflag(id).await.unwrap()), "unflagged");
    assert_eq!(status(&client.flag(id).await.unwrap()), "flagged");
    assert_eq!(status(&client.delete(id).await.unwrap()), "deleted");
    assert!(find(&client, "Home", id).await.is_none());
}

#[tokio::test]
async fn leading_dash_title_and_url_and_weekday_due() {
    let fake = Fake::new();
    let client = fake.client();
    let result = client.add(&add("-dangerous title", "Home")).await.unwrap();
    assert_eq!(status(&result), "created");
    assert!(find(&client, "Home", numeric_id(&result)).await.is_some());

    let result = client
        .add(&AddFields {
            url: "https://example.com/x".into(),
            ..add("With link", "Home")
        })
        .await
        .unwrap();
    assert_eq!(
        find(&client, "Home", numeric_id(&result))
            .await
            .unwrap()
            .url,
        "https://example.com/x"
    );

    let result = client
        .add(&AddFields {
            due: "fri 9:00".into(),
            ..add("Weekday due", "Home")
        })
        .await
        .unwrap();
    let due = find(&client, "Home", numeric_id(&result))
        .await
        .unwrap()
        .due()
        .unwrap();
    assert_eq!(due.weekday().num_days_from_monday(), 4);
    assert_eq!((due.hour(), due.minute()), (9, 0));
}

#[tokio::test]
async fn move_between_lists() {
    let fake = Fake::new();
    let client = fake.client();
    let id = numeric_id(&client.add(&add("Migrating task", "Home")).await.unwrap());
    client
        .edit(
            id,
            &EditFields {
                list_title: Some("Work".into()),
                ..EditFields::default()
            },
        )
        .await
        .unwrap();
    assert!(find(&client, "Work", id).await.is_some());
    assert!(find(&client, "Home", id).await.is_none());
}

#[tokio::test]
async fn errors_keep_their_shapes() {
    let fake = Fake::new();
    let client = fake.client();
    let err = client.done(99999).await.unwrap_err();
    assert!(err.message.contains("not found"));
    assert_eq!(err.exit_code, 1);
    let err = client
        .add(&AddFields {
            due: "whenever".into(),
            ..add("Bad due", "")
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "invalid_due_date");
    assert_eq!(err.exit_code, 2);
    let err = RemctlClient::new(vec!["definitely-not-a-real-binary-xyz".into()])
        .get_lists()
        .await
        .unwrap_err();
    assert_eq!(err.code, "not_found");
}

#[tokio::test]
async fn edit_flag_routes_through_flag_commands() {
    let fake = Fake::new();
    let client = fake.client();
    let id = numeric_id(&client.add(&add("Flag me", "Home")).await.unwrap());
    let flag_only = EditFields {
        flagged: Some(true),
        ..EditFields::default()
    };
    assert_eq!(
        status(&client.edit(id, &flag_only).await.unwrap()),
        "flagged"
    );
    assert!(find(&client, "Home", id).await.unwrap().flagged);
    let unflag = EditFields {
        flagged: Some(false),
        ..EditFields::default()
    };
    assert_eq!(
        status(&client.edit(id, &unflag).await.unwrap()),
        "unflagged"
    );
    assert!(!find(&client, "Home", id).await.unwrap().flagged);

    // fields and flag together: the field edit's payload wins
    let both = EditFields {
        title: Some("Both changed".into()),
        priority: Some("high".into()),
        flagged: Some(true),
        ..EditFields::default()
    };
    assert_eq!(status(&client.edit(id, &both).await.unwrap()), "updated");
    let edited = find(&client, "Home", id).await.unwrap();
    assert_eq!(edited.title, "Both changed");
    assert_eq!(edited.priority, Priority::High);
    assert!(edited.flagged);

    // nothing to change: no subprocess, no payload
    assert!(
        client
            .edit(id, &EditFields::default())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn partial_flag_failures() {
    let fake = Fake::new();
    let client = fake.client_failing_flags();
    let result = client
        .add(&AddFields {
            flagged: true,
            ..add("Created but unflagged", "Home")
        })
        .await
        .unwrap();
    assert_eq!(status(&result), "created");
    assert!(
        !find(&client, "Home", numeric_id(&result))
            .await
            .unwrap()
            .flagged
    );
    let warnings = remtui::client::warnings_of(result.as_ref());
    assert!(warnings.iter().any(|w| w.starts_with("flag_not_set:")));

    let id = numeric_id(&client.add(&add("Cannot flag", "Home")).await.unwrap());
    assert_eq!(
        client.flag(id).await.unwrap_err().code,
        "applescript_flag_failed"
    );

    let id = numeric_id(&client.add(&add("Partial edit", "Home")).await.unwrap());
    let err = client
        .edit(
            id,
            &EditFields {
                title: Some("Renamed anyway".into()),
                flagged: Some(true),
                ..EditFields::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "applescript_flag_failed");
    let edited = find(&client, "Home", id).await.unwrap();
    assert_eq!(edited.title, "Renamed anyway");
    assert!(!edited.flagged);
}

#[tokio::test]
async fn linked_reminders_sweep_notes_across_lists() {
    let fake = Fake::new();
    let client = fake.client();
    let todo = Todo::new(
        "NOTE-X",
        "Some note",
        &[],
        "ship it",
        "- [ ] ship it",
        "## Tasks",
    );
    let created = client
        .add(&AddFields {
            notes: link_notes(&todo),
            due: "today".into(),
            ..add("ship it", "Work")
        })
        .await
        .unwrap();
    let other = client
        .add(&AddFields {
            notes: link_notes(&todo),
            due: "today".into(),
            ..add("ship it too", "Home")
        })
        .await
        .unwrap();
    client.done(numeric_id(&other)).await.unwrap();
    let linked = client.linked_reminders().await.unwrap();
    let ids: Vec<i64> = linked.iter().map(|r| r.id).collect();
    assert!(ids.contains(&numeric_id(&created)) && ids.contains(&numeric_id(&other)));
    assert!(linked.iter().all(|r| link_key(&r.notes) == todo.key()));
    assert!(linked.iter().any(|r| r.completed) && linked.iter().any(|r| !r.completed));
}

#[test]
fn fake_remctl_exit_codes_and_shapes() {
    use std::process::Command;
    let fake = Fake::new();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_fake-remctl"))
            .args(args)
            .env("REMTUI_FAKE_STATE", fake.remctl_state())
            .output()
            .unwrap()
    };
    let out = run(&["lists", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let lists: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(lists.as_array().unwrap().len(), 5);
    assert_eq!(lists[2]["badge"]["emoji"], "🛒");
    assert_eq!(lists[0]["counts"]["active"], 4);

    let out = run(&["edit", "--json", "101", "--flagged"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("require --private"));

    let out = run(&["delete", "--json", "101"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "Cancelled.\n");

    let out = run(&["add", "--json", "--due=never", "--", "x"]);
    assert_eq!(out.status.code(), Some(2));
    let err: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["code"], "invalid_due_date");
    assert_eq!(err["message"], "Could not parse due date: 'never'");

    let out = run(&["show", "--json", "--", "Nope"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "Error: list 'Nope' not found\n"
    );

    let out = run(&["info", "--json", "101"]);
    let info: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(info["subtasks"], serde_json::json!([]));
}
