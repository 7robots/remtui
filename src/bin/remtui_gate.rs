//! Acceptance gate against live remctl, driven headlessly through the harness.
//!
//! `remtui-gate [--list NAME] [--with-flag]` loads the real lists, then adds a
//! scratch reminder, edits it, completes it, reopens it, changes its priority,
//! optionally flags and unflags it, and deletes it, reading each step back
//! through remctl. `remtui-gate --bench` prints timings as `key=value` lines
//! that `tools/bench_python.py` mirrors for the Python remtui.

use std::sync::Arc;
use std::time::{Duration, Instant};

use remtui::app::{App, Pane, ViewKind};
use remtui::client::{RemctlClient, resolve_remctl};
use remtui::config::Config;
use remtui::harness::Harness;

const SIZE: (u16, u16) = (120, 40);

fn check(cond: bool, label: &str) {
    if cond {
        println!("ok   {label}");
    } else {
        println!("FAIL {label}");
        std::process::exit(1);
    }
}

async fn wait(h: &mut Harness, label: &str, secs: u64, pred: impl Fn(&App) -> bool) {
    match h.wait_until(pred, Duration::from_secs(secs)).await {
        Ok(()) => println!("ok   {label}"),
        Err(err) => {
            println!("FAIL {label}\n{err}");
            std::process::exit(1);
        }
    }
}

fn live_client() -> Arc<RemctlClient> {
    Arc::new(RemctlClient::new(vec![resolve_remctl()]))
}

/// Move the sidebar highlight onto the named list.
fn select_list(h: &mut Harness, id: i64) {
    h.press("h");
    for _ in 0..200 {
        if h.app.view == ViewKind::List(id) {
            return;
        }
        h.press("j");
    }
}

/// Move the reminder cursor onto the row with this title.
fn select_row(h: &mut Harness, title: &str) -> bool {
    h.press("l");
    h.press("g");
    for _ in 0..h.app.shown.len().max(1) {
        if h.app.selected().is_some_and(|r| r.title == title) {
            return true;
        }
        h.press("j");
    }
    h.app.selected().is_some_and(|r| r.title == title)
}

async fn gate(list_name: Option<String>, with_flag: bool) {
    let client = live_client();
    let mut h = Harness::new(Config::default(), client.clone(), false, SIZE);
    h.start();
    wait(&mut h, "lists and the Today view load", 30, |app| {
        app.loaded && !app.lists.is_empty() && app.view_loaded
    })
    .await;
    let list = match &list_name {
        Some(name) => h
            .app
            .lists
            .iter()
            .find(|l| l.title.eq_ignore_ascii_case(name))
            .cloned(),
        None => h
            .app
            .lists
            .iter()
            .find(|l| l.title.eq_ignore_ascii_case("inbox"))
            .or(h.app.lists.first())
            .cloned(),
    };
    let Some(list) = list else {
        println!("FAIL no such list: {list_name:?}");
        std::process::exit(1);
    };
    println!("     scratch list: {}", list.title);
    select_list(&mut h, list.id);
    wait(
        &mut h,
        &format!("list view '{}' loads", list.title),
        30,
        |app| !app.loading && app.view_loaded,
    )
    .await;

    let stamp = chrono::Local::now().format("%H%M%S");
    let title = format!("remtui-gate scratch {stamp}");
    let edited = format!("{title} edited");

    // add
    h.press("a");
    check(
        h.app.overlay.as_ref().map(|o| o.name()) == Some("form"),
        "form opens",
    );
    h.type_text(&title);
    h.press("ctrl+s");
    wait(&mut h, "form closes after add", 30, |app| {
        app.overlay.is_none()
    })
    .await;
    let title_c = title.clone();
    wait(
        &mut h,
        "added reminder appears in the list",
        30,
        move |app| !app.loading && app.reminders.iter().any(|r| r.title == title_c),
    )
    .await;
    let found = client.search(&title, false).await.unwrap_or_default();
    check(
        found.len() == 1,
        "remctl search reads the new reminder back",
    );
    let id = found[0].id;
    check(
        h.app
            .toast_messages()
            .iter()
            .any(|m| m == "＋ Reminder added"),
        "add toast",
    );

    let cleanup = |client: Arc<RemctlClient>| async move {
        let _ = client.delete(id).await;
    };

    // edit
    check(
        select_row(&mut h, &title),
        "cursor lands on the scratch row",
    );
    h.press("e");
    h.press("end");
    h.type_text(" edited");
    h.press("ctrl+s");
    wait(&mut h, "form closes after edit", 30, |app| {
        app.overlay.is_none()
    })
    .await;
    let edited_c = edited.clone();
    wait(&mut h, "edited title appears", 30, move |app| {
        !app.loading && app.reminders.iter().any(|r| r.title == edited_c)
    })
    .await;
    let back = client.search(&edited, false).await.unwrap_or_default();
    if back.len() != 1 {
        cleanup(client.clone()).await;
        check(false, "remctl reads the edited title back");
    }
    println!("ok   remctl reads the edited title back");

    // done / undone
    check(select_row(&mut h, &edited), "cursor on the edited row");
    h.press("space");
    wait(&mut h, "completed toast", 30, |app| {
        app.toasts
            .iter()
            .any(|t| t.message.starts_with("✓ Completed"))
    })
    .await;
    wait(&mut h, "completed row leaves the active list", 30, |app| {
        !app.loading && app.reminders.iter().all(|r| r.id != id)
    })
    .await;
    let done = client
        .get_reminders(&list.title, true)
        .await
        .unwrap_or_default();
    check(
        done.iter().any(|r| r.id == id && r.completed),
        "remctl reads it back as completed",
    );
    h.press("c");
    wait(&mut h, "completed rows shown", 30, |app| {
        !app.loading && app.show_completed
    })
    .await;
    check(select_row(&mut h, &edited), "cursor on the completed row");
    h.press("space");
    wait(&mut h, "reopened toast", 30, |app| {
        app.toasts
            .iter()
            .any(|t| t.message.starts_with("↺ Reopened"))
    })
    .await;
    wait(&mut h, "row is active again", 30, move |app| {
        !app.loading && app.reminders.iter().any(|r| r.id == id && !r.completed)
    })
    .await;

    // priority
    check(
        select_row(&mut h, &edited),
        "cursor on the row for priority",
    );
    h.press("p");
    wait(&mut h, "priority toast", 30, |app| {
        app.toasts.iter().any(|t| t.message == "Priority → low")
    })
    .await;
    wait(&mut h, "priority low reads back", 30, move |app| {
        !app.loading
            && app
                .reminders
                .iter()
                .any(|r| r.id == id && r.priority == remtui::models::Priority::Low)
    })
    .await;

    // flag
    if with_flag {
        check(select_row(&mut h, &edited), "cursor on the row for flag");
        h.press("f");
        check(
            h.app.is_pending(id),
            "flag write is pending with the row flipped",
        );
        wait(
            &mut h,
            "flag write lands (AppleScript, up to 120 s)",
            150,
            move |app| !app.is_pending(id),
        )
        .await;
        wait(&mut h, "flagged reads back", 30, move |app| {
            !app.loading && app.reminders.iter().any(|r| r.id == id && r.flagged)
        })
        .await;
        check(select_row(&mut h, &edited), "cursor on the flagged row");
        h.press("f");
        wait(&mut h, "unflag write lands", 150, move |app| {
            !app.is_pending(id)
        })
        .await;
        wait(&mut h, "unflagged reads back", 30, move |app| {
            !app.loading && app.reminders.iter().any(|r| r.id == id && !r.flagged)
        })
        .await;
    }

    // delete
    check(select_row(&mut h, &edited), "cursor on the row for delete");
    h.press("d");
    check(
        h.app.overlay.as_ref().map(|o| o.name()) == Some("confirm"),
        "delete asks first",
    );
    h.press("y");
    wait(&mut h, "deleted toast", 30, |app| {
        app.toasts
            .iter()
            .any(|t| t.message.starts_with("🗑 Deleted"))
    })
    .await;
    wait(&mut h, "row is gone", 30, move |app| {
        !app.loading && app.reminders.iter().all(|r| r.id != id)
    })
    .await;
    let gone = client
        .get_reminders(&list.title, true)
        .await
        .unwrap_or_default();
    check(gone.iter().all(|r| r.id != id), "remctl no longer lists it");
    check(
        h.app.focus == Pane::Reminders && h.app.running,
        "app still running with the reminders pane focused",
    );
    println!("GATE PASSED");
}

async fn bench(process_start: Instant) {
    let client = live_client();
    let mut h = Harness::new(Config::default(), client.clone(), false, SIZE);
    h.start();
    h.wait_until(
        |app| app.loaded && app.view_loaded && !app.loading,
        Duration::from_secs(60),
    )
    .await
    .expect("first load");
    let first_frame = process_start.elapsed();
    let epoch_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    println!("implementation=rust");
    println!("lists={}", h.app.lists.len());
    println!("first_frame_ms={}", first_frame.as_millis());
    println!("first_frame_epoch_ms={epoch_ms}");

    let t = Instant::now();
    h.press("r");
    h.wait_until(|app| !app.loading, Duration::from_secs(60))
        .await
        .expect("reload");
    println!("warm_reload_ms={}", t.elapsed().as_millis());

    let t = Instant::now();
    let _ = client.get_lists().await;
    println!("remctl_lists_ms={}", t.elapsed().as_millis());
    let t = Instant::now();
    let _ = client.today().await;
    println!("remctl_today_ms={}", t.elapsed().as_millis());

    // input to frame: median of 20 `j`/`k` presses on the reminders pane
    h.press("l");
    let mut samples: Vec<u128> = Vec::new();
    for i in 0..20 {
        let t = Instant::now();
        h.press(if i % 2 == 0 { "j" } else { "k" });
        samples.push(t.elapsed().as_micros());
    }
    samples.sort();
    println!("keypress_frame_us={}", samples[samples.len() / 2]);

    // resident memory of this process, from ps (KB)
    if let Ok(out) = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        && let Ok(kb) = String::from_utf8_lossy(&out.stdout).trim().parse::<u64>()
    {
        println!("rss_bytes={}", kb * 1024);
    }
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("remtui")));
    if let Some(size) = exe.and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()) {
        println!("binary_bytes={size}");
    }
}

#[tokio::main]
async fn main() {
    let process_start = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--bench") {
        bench(process_start).await;
        return;
    }
    let with_flag = args.iter().any(|a| a == "--with-flag");
    let list = args
        .iter()
        .position(|a| a == "--list")
        .and_then(|i| args.get(i + 1))
        .cloned();
    gate(list, with_flag).await;
}
