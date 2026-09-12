//! A local stand-in for remctl, used for demo mode and tests.
//!
//! Implements the subset of remctl's CLI that remtui drives, matching the real
//! tool's JSON contract: bare arrays on stdout for reads, compact status JSON
//! for mutations, `Error: #<id> not found` plain text on stderr (exit 1), and
//! structured `{"status": "error", ...}` JSON on stderr for invalid due dates
//! (exit 2).
//!
//! State lives in a JSON file at `$REMTUI_FAKE_STATE` (default
//! `~/.cache/remtui/demo.json`), seeded with sample data on first run. The
//! shapes are the Python fake's, so the two are interchangeable.

use std::path::PathBuf;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde_json::{Map, Value, json};

use super::{Args, write_atomic};
use crate::util::home_dir;

pub const ENV_STATE: &str = "REMTUI_FAKE_STATE";
pub const ENV_FLAG_FAILS: &str = "REMTUI_FAKE_FLAG_FAILS";
const ISO: &str = "%Y-%m-%dT%H:%M:%S";
const FLAG_ERROR: &str = "osascript: Automation permission denied (-1743)";

pub fn state_path() -> PathBuf {
    match std::env::var(ENV_STATE) {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => home_dir().join(".cache").join("remtui").join("demo.json"),
    }
}

fn list_color(name: &str) -> &'static str {
    match name {
        "red" => "#FF3B30",
        "orange" => "#FF9500",
        "yellow" => "#FFCC00",
        "green" => "#34C759",
        "blue" => "#007AFF",
        "purple" => "#AF52DE",
        "brown" => "#A2845E",
        _ => "#007AFF",
    }
}

// -- due date parsing --------------------------------------------------------

fn parse_time(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();
    if let Some((h, m)) = text.split_once(':') {
        let h: u32 = h.parse().ok()?;
        let m: u32 = m.parse().ok()?;
        if h < 24 && m < 60 && m.to_string().len() <= 2 && h.to_string().len() <= 2 {
            return Some((h, m));
        }
        return None;
    }
    let lower = text.to_lowercase();
    for (suffix, add) in [("am", 0), ("pm", 12)] {
        if let Some(num) = lower.strip_suffix(suffix) {
            let h: u32 = num.parse().ok()?;
            if num.len() <= 2 {
                return Some((h % 12 + add, 0));
            }
        }
    }
    None
}

fn weekday_index(word: &str) -> Option<u32> {
    Some(match word {
        "mon" | "monday" => 0,
        "tue" | "tues" | "tuesday" => 1,
        "wed" | "wednesday" => 2,
        "thu" | "thur" | "thurs" | "thursday" => 3,
        "fri" | "friday" => 4,
        "sat" | "saturday" => 5,
        "sun" | "sunday" => 6,
        _ => return None,
    })
}

fn iso_day(day: NaiveDate, h: u32, m: u32) -> String {
    format!("{}T{h:02}:{m:02}:00", day.format("%Y-%m-%d"))
}

/// Parse a user-supplied due string → (ISO datetime, all_day), or Err like the
/// real remctl (exit 2).
pub fn parse_due_input(text: &str, now: NaiveDateTime) -> Result<(String, bool), String> {
    let text = text.trim();
    let lowered = text.to_lowercase();
    if let Ok(day) = NaiveDate::parse_from_str(text, "%Y-%m-%d")
        && text.len() == 10
    {
        return Ok((iso_day(day, 0, 0), true));
    }
    if text.len() >= 15
        && let Some((day_s, time_s)) = text.split_at_checked(10)
        && let Ok(day) = NaiveDate::parse_from_str(day_s, "%Y-%m-%d")
        && (time_s.starts_with(' ') || time_s.starts_with('T'))
        && let Some((h, m)) = parse_time(&time_s[1..])
        && time_s[1..].contains(':')
    {
        return Ok((iso_day(day, h, m), false));
    }
    if let Some(n) = lowered.strip_prefix('+').and_then(|r| r.strip_suffix('d'))
        && let Ok(days) = n.parse::<i64>()
    {
        return Ok((iso_day(now.date() + Duration::days(days), 0, 0), true));
    }
    let joined = lowered.replace(" at ", " ");
    let words: Vec<&str> = joined.split_whitespace().collect();
    let base = words.first().and_then(|w| match *w {
        "today" | "tonight" => Some(0),
        "tomorrow" => Some(1),
        _ => None,
    });
    if let Some(base) = base {
        let day = now.date() + Duration::days(base);
        if words.len() == 1 {
            if words[0] == "tonight" {
                return Ok((iso_day(day, 21, 0), false));
            }
            return Ok((iso_day(day, 0, 0), true));
        }
        if let Some((h, m)) = parse_time(words[1]) {
            return Ok((iso_day(day, h, m), false));
        }
    }
    let mut words = &words[..];
    if words.first() == Some(&"next") {
        words = &words[1..];
    }
    if let Some(target) = words.first().and_then(|w| weekday_index(w)) {
        let today = now.weekday().num_days_from_monday();
        let mut delta = (target as i64 - today as i64).rem_euclid(7);
        if delta == 0 {
            delta = 7;
        }
        let day = now.date() + Duration::days(delta);
        if words.len() == 1 {
            return Ok((iso_day(day, 0, 0), true));
        }
        if let Some((h, m)) = parse_time(words[1]) {
            return Ok((iso_day(day, h, m), false));
        }
    }
    Err(text.to_string())
}

// -- state -------------------------------------------------------------------

pub fn seed_state(now: NaiveDateTime) -> Value {
    let iso_in = |days: i64, hour: u32, minute: u32| {
        iso_day(now.date() + Duration::days(days), hour, minute)
    };
    let lists = json!([
        {"id": 1, "title": "Personal", "color": "blue", "emoji": ""},
        {"id": 2, "title": "Work", "color": "red", "emoji": ""},
        {"id": 3, "title": "Groceries", "color": "green", "emoji": "🛒"},
        {"id": 4, "title": "Reading", "color": "purple", "emoji": "📚"},
        {"id": 5, "title": "Home", "color": "orange", "emoji": ""},
    ]);
    // title, list, due, all_day, priority, flagged, notes, tags, completed
    type Row<'a> = (
        &'a str,
        &'a str,
        String,
        bool,
        &'a str,
        bool,
        &'a str,
        Vec<&'a str>,
        bool,
    );
    let raw: Vec<Row> = vec![
        (
            "Renew passport",
            "Personal",
            iso_in(-3, 0, 0),
            true,
            "high",
            true,
            "Bring old passport and two photos",
            vec!["errands"],
            false,
        ),
        (
            "Call the dentist",
            "Personal",
            iso_in(0, 11, 30),
            false,
            "medium",
            false,
            "Reschedule the cleaning",
            vec![],
            false,
        ),
        (
            "Pay credit card bill",
            "Personal",
            iso_in(0, 0, 0),
            true,
            "high",
            false,
            "",
            vec!["finance"],
            false,
        ),
        (
            "Book flights for August",
            "Personal",
            iso_in(4, 0, 0),
            true,
            "none",
            false,
            "Check points balance first",
            vec!["travel"],
            false,
        ),
        (
            "Morning run",
            "Personal",
            iso_in(1, 7, 0),
            false,
            "none",
            false,
            "5k along the river",
            vec!["health"],
            true,
        ),
        (
            "Ship v2.3 release notes",
            "Work",
            iso_in(0, 16, 0),
            false,
            "high",
            true,
            "Waiting on changelog review from Sam",
            vec!["release"],
            false,
        ),
        (
            "Review Q3 hiring plan",
            "Work",
            iso_in(-1, 10, 0),
            false,
            "medium",
            false,
            "",
            vec![],
            false,
        ),
        (
            "1:1 prep — Patricia",
            "Work",
            iso_in(1, 9, 0),
            false,
            "medium",
            false,
            "Bring dashboard sync status",
            vec!["meetings"],
            false,
        ),
        (
            "File expense report",
            "Work",
            iso_in(6, 0, 0),
            true,
            "low",
            false,
            "Conference receipts in Downloads",
            vec![],
            false,
        ),
        (
            "Archive old sprint boards",
            "Work",
            String::new(),
            false,
            "none",
            false,
            "",
            vec![],
            true,
        ),
        (
            "Milk",
            "Groceries",
            String::new(),
            false,
            "none",
            false,
            "",
            vec![],
            false,
        ),
        (
            "Sourdough bread",
            "Groceries",
            String::new(),
            false,
            "none",
            false,
            "",
            vec![],
            false,
        ),
        (
            "Olive oil",
            "Groceries",
            String::new(),
            false,
            "none",
            false,
            "The good one",
            vec![],
            false,
        ),
        (
            "Coffee beans",
            "Groceries",
            String::new(),
            false,
            "high",
            false,
            "",
            vec![],
            true,
        ),
        (
            "Finish 'The Shallows'",
            "Reading",
            iso_in(9, 0, 0),
            true,
            "none",
            false,
            "Chapter 7 onward",
            vec!["books"],
            false,
        ),
        (
            "Start sci-fi book club pick",
            "Reading",
            String::new(),
            false,
            "low",
            false,
            "",
            vec!["books"],
            false,
        ),
        (
            "Replace furnace filter",
            "Home",
            iso_in(2, 0, 0),
            true,
            "medium",
            false,
            "16x25x1 — two spares in garage",
            vec![],
            false,
        ),
        (
            "Fix squeaky hinge",
            "Home",
            String::new(),
            false,
            "none",
            false,
            "",
            vec![],
            false,
        ),
    ];
    let mut reminders = Vec::new();
    for (i, row) in raw.into_iter().enumerate() {
        let (title, list, due, all_day, priority, flagged, notes, tags, completed) = row;
        let mut r = json!({
            "id": 101 + i as i64,
            "title": title,
            "list": list,
            "completed": completed,
            "flagged": flagged,
            "urgent": false,
            "priority": priority,
            "notes": notes,
            "tags": tags,
            "dueDate": due,
            "allDay": all_day,
        });
        if title == "Start sci-fi book club pick" {
            r["url"] = json!("https://bookclub.example.com/picks");
        }
        reminders.push(r);
    }
    json!({"next_id": 200, "lists": lists, "reminders": reminders})
}

fn load_state(now: NaiveDateTime) -> Result<Value, String> {
    let path = state_path();
    if !path.exists() {
        let state = seed_state(now);
        save_state(&state)?;
        return Ok(state);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn save_state(state: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    write_atomic(&state_path(), &text).map_err(|e| e.to_string())
}

// -- serialization (matches remctl's field surface) ---------------------------

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn serialize_reminder(row: &Value) -> Value {
    let mut out = Map::new();
    out.insert("id".into(), row["id"].clone());
    out.insert("title".into(), row["title"].clone());
    out.insert("list".into(), row["list"].clone());
    out.insert("completed".into(), json!(b(row, "completed")));
    out.insert("flagged".into(), json!(b(row, "flagged")));
    out.insert("urgent".into(), json!(b(row, "urgent")));
    let priority = s(row, "priority");
    out.insert(
        "priority".into(),
        json!(if priority.is_empty() {
            "none".to_string()
        } else {
            priority
        }),
    );
    out.insert("subtaskCount".into(), json!(0));
    out.insert("isSubtask".into(), json!(false));
    if !s(row, "notes").is_empty() {
        out.insert("notes".into(), row["notes"].clone());
    }
    if !s(row, "url").is_empty() {
        out.insert("url".into(), row["url"].clone());
    }
    if !s(row, "dueDate").is_empty() {
        out.insert("dueDate".into(), row["dueDate"].clone());
        out.insert("allDay".into(), json!(b(row, "allDay")));
    }
    if row
        .get("tags")
        .and_then(Value::as_array)
        .is_some_and(|t| !t.is_empty())
    {
        out.insert("tags".into(), row["tags"].clone());
    }
    if !s(row, "completionDate").is_empty() {
        out.insert("completionDate".into(), row["completionDate"].clone());
    }
    Value::Object(out)
}

fn serialize_list(lst: &Value, reminders: &[Value]) -> Value {
    let title = s(lst, "title");
    let mine: Vec<&Value> = reminders.iter().filter(|r| s(r, "list") == title).collect();
    let active = mine.iter().filter(|r| !b(r, "completed")).count();
    let color = s(lst, "color");
    let color = if color.is_empty() {
        "blue".to_string()
    } else {
        color
    };
    let mut out = json!({
        "id": lst["id"],
        "title": title,
        "listType": if title == "Groceries" { "groceries" } else { "standard" },
        "isGroup": false,
        "isGroceries": title == "Groceries",
        "color": {"name": color, "hex": list_color(&color)},
        "counts": {"active": active, "completed": mine.len() - active, "total": mine.len()},
    });
    let emoji = s(lst, "emoji");
    if !emoji.is_empty() {
        out["badge"] = json!({"raw": emoji, "emoji": emoji});
    }
    out
}

// -- helpers -----------------------------------------------------------------

/// What the process should do: print these streams and exit with this code.
pub struct Outcome {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

fn ok(stdout: String) -> Outcome {
    Outcome {
        stdout,
        stderr: String::new(),
        code: 0,
    }
}

fn fail(message: impl Into<String>) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{}\n", message.into()),
        code: 1,
    }
}

fn fail_not_found(id: i64) -> Outcome {
    fail(format!("Error: #{id} not found"))
}

fn flag_writes_fail() -> bool {
    std::env::var(ENV_FLAG_FAILS).is_ok_and(|v| v == "1")
}

fn fail_flag_write(id: i64) -> Outcome {
    let payload = json!({
        "status": "error",
        "code": "applescript_flag_failed",
        "id": id,
        "message": format!("Could not set the flag: {FLAG_ERROR}"),
    });
    Outcome {
        stdout: String::new(),
        stderr: format!("{payload}\n"),
        code: 1,
    }
}

fn fail_invalid_due(value: &str) -> Outcome {
    let payload = json!({
        "status": "error",
        "code": "invalid_due_date",
        "message": format!("Could not parse due date: {}", py_repr(value)),
        "field": "due",
        "input": value,
        "examples": ["2026-06-01", "tomorrow 09:30", "today at 3pm", "+3d"],
    });
    Outcome {
        stdout: String::new(),
        stderr: format!("{payload}\n"),
        code: 2,
    }
}

/// Python's `repr()` of a str, for byte-identical error text.
fn py_repr(text: &str) -> String {
    if text.contains('\'') && !text.contains('"') {
        format!("\"{}\"", text.replace('\\', "\\\\"))
    } else {
        format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

fn find_list_index(state: &Value, name: &str) -> Option<usize> {
    let lists = state["lists"].as_array()?;
    lists
        .iter()
        .position(|l| s(l, "title") == name)
        .or_else(|| {
            lists
                .iter()
                .position(|l| s(l, "title").to_lowercase() == name.to_lowercase())
        })
}

fn emit_array(rows: &[&Value]) -> Outcome {
    let out: Vec<Value> = rows.iter().map(|r| serialize_reminder(r)).collect();
    ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&out).unwrap_or_default()
    ))
}

fn emit_status(payload: Value) -> Outcome {
    ok(format!("{payload}\n"))
}

fn due_datetime(row: &Value) -> Option<NaiveDateTime> {
    let text = s(row, "dueDate");
    if text.is_empty() {
        return None;
    }
    NaiveDateTime::parse_from_str(&text, ISO).ok()
}

fn apply_due(row: &mut Value, value: &str, now: NaiveDateTime) -> Result<(), Outcome> {
    if value == "clear" {
        row["dueDate"] = json!("");
        row["allDay"] = json!(false);
        return Ok(());
    }
    match parse_due_input(value, now) {
        Ok((iso, all_day)) => {
            row["dueDate"] = json!(iso);
            row["allDay"] = json!(all_day);
            Ok(())
        }
        Err(_) => Err(fail_invalid_due(value)),
    }
}

fn priority_alias(text: &str) -> Option<&'static str> {
    Some(match text.to_lowercase().as_str() {
        "high" | "h" => "high",
        "medium" | "med" | "m" => "medium",
        "low" | "l" => "low",
        "none" => "none",
        _ => return None,
    })
}

fn parse_id(text: Option<&String>) -> Result<i64, Outcome> {
    match text.and_then(|t| t.parse::<i64>().ok()) {
        Some(id) => Ok(id),
        None => Err(Outcome {
            stdout: String::new(),
            stderr:
                "usage: remctl (fake) ...\nremctl (fake): error: argument id: invalid int value\n"
                    .into(),
            code: 2,
        }),
    }
}

// -- command handlers ---------------------------------------------------------

/// Run the fake with `argv` (without the program name) at time `now`.
pub fn run_at(argv: &[String], now: NaiveDateTime) -> Outcome {
    let Some((command, rest)) = argv.split_first() else {
        return Outcome {
            stdout: String::new(),
            stderr: "usage: remctl (fake) command ...\nremctl (fake): error: the following arguments are required: command\n".into(),
            code: 2,
        };
    };
    let aliases = [
        ("l", "list"),
        ("n", "notes"),
        ("d", "due"),
        ("p", "priority"),
        ("f", "flag"),
        ("t", "tags"),
    ];
    let takes_value = ["list", "notes", "due", "priority", "tags", "url", "title"];
    let args = match Args::parse(rest, &takes_value, &aliases) {
        Ok(a) => a,
        Err(e) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("usage: remctl (fake) ...\nremctl (fake): error: {e}\n"),
                code: 2,
            };
        }
    };
    let mut state = match load_state(now) {
        Ok(s) => s,
        Err(e) => return fail(format!("Error: {e}")),
    };
    let reminders: Vec<Value> = state["reminders"].as_array().cloned().unwrap_or_default();
    let active: Vec<&Value> = reminders.iter().filter(|r| !b(r, "completed")).collect();

    match command.as_str() {
        "lists" => {
            let lists: Vec<Value> = state["lists"]
                .as_array()
                .map(|ls| ls.iter().map(|l| serialize_list(l, &reminders)).collect())
                .unwrap_or_default();
            ok(format!(
                "{}\n",
                serde_json::to_string_pretty(&lists).unwrap_or_default()
            ))
        }
        "show" => {
            let Some(name) = args.positional.first() else {
                return fail("usage: remctl (fake) show [--json] [--completed] list");
            };
            let Some(idx) = find_list_index(&state, name) else {
                return fail(format!("Error: list '{name}' not found"));
            };
            let title = s(&state["lists"][idx], "title");
            let rows: Vec<&Value> = reminders
                .iter()
                .filter(|r| s(r, "list") == title)
                .filter(|r| args.flag("completed") || !b(r, "completed"))
                .collect();
            emit_array(&rows)
        }
        "today" => emit_array(
            &active
                .iter()
                .copied()
                .filter(|r| due_datetime(r).is_some_and(|d| d.date() == now.date()))
                .collect::<Vec<_>>(),
        ),
        "upcoming" => {
            let days: i64 = args
                .positional
                .first()
                .and_then(|d| d.parse().ok())
                .unwrap_or(7);
            let horizon = now.date() + Duration::days(days);
            emit_array(
                &active
                    .iter()
                    .copied()
                    .filter(|r| {
                        due_datetime(r)
                            .is_some_and(|d| now.date() <= d.date() && d.date() <= horizon)
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "overdue" => emit_array(
            &active
                .iter()
                .copied()
                .filter(|r| {
                    due_datetime(r).is_some_and(|d| {
                        if b(r, "allDay") {
                            d.date() < now.date()
                        } else {
                            d < now
                        }
                    })
                })
                .collect::<Vec<_>>(),
        ),
        "flagged" => emit_array(
            &active
                .iter()
                .copied()
                .filter(|r| b(r, "flagged"))
                .collect::<Vec<_>>(),
        ),
        "search" => {
            let Some(query) = args.positional.first() else {
                return fail("usage: remctl (fake) search [--json] [--completed] query");
            };
            let needle = query.to_lowercase();
            let pool: Vec<&Value> = if args.flag("completed") {
                reminders.iter().collect()
            } else {
                active.clone()
            };
            emit_array(
                &pool
                    .into_iter()
                    .filter(|r| {
                        s(r, "title").to_lowercase().contains(&needle)
                            || s(r, "notes").to_lowercase().contains(&needle)
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "info" => {
            let id = match parse_id(args.positional.first()) {
                Ok(id) => id,
                Err(o) => return o,
            };
            let Some(row) = reminders.iter().find(|r| r["id"].as_i64() == Some(id)) else {
                return fail_not_found(id);
            };
            let mut payload = serialize_reminder(row);
            payload["subtasks"] = json!([]);
            ok(format!(
                "{}\n",
                serde_json::to_string_pretty(&payload).unwrap_or_default()
            ))
        }
        "add" => {
            let Some(title) = args.positional.first().cloned() else {
                return fail("usage: remctl (fake) add [--json] title");
            };
            let list_name = args
                .opt("list")
                .map(str::to_string)
                .unwrap_or_else(|| s(&state["lists"][0], "title"));
            let Some(idx) = find_list_index(&state, &list_name) else {
                return fail(format!("Error: list '{list_name}' not found"));
            };
            let list_title = s(&state["lists"][idx], "title");
            let priority = match args.opt("priority") {
                Some(p) if !p.is_empty() => match priority_alias(p) {
                    Some(p) => p,
                    None => return fail(format!("Error: invalid priority '{p}'")),
                },
                _ => "none",
            };
            let id = state["next_id"].as_i64().unwrap_or(200);
            let tags: Vec<String> = args
                .opt("tags")
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .collect();
            let mut row = json!({
                "id": id,
                "title": title,
                "list": list_title,
                "completed": false,
                // A failed flag write does not undo the creation.
                "flagged": args.flag("flag") && !flag_writes_fail(),
                "urgent": false,
                "priority": priority,
                "notes": args.opt("notes").unwrap_or(""),
                "tags": tags,
                "dueDate": "",
                "allDay": false,
            });
            if let Some(url) = args.opt("url").filter(|u| !u.is_empty()) {
                row["url"] = json!(url);
            }
            if let Some(due) = args.opt("due").filter(|d| !d.is_empty())
                && let Err(outcome) = apply_due(&mut row, due, now)
            {
                return outcome;
            }
            state["next_id"] = json!(id + 1);
            state["reminders"].as_array_mut().unwrap().push(row.clone());
            if let Err(e) = save_state(&state) {
                return fail(format!("Error: {e}"));
            }
            let mut created = json!({
                "status": "created",
                "id": format!("FAKE-CK-{id}"),
                "title": row["title"],
                "numericId": id,
            });
            if args.flag("flag") && flag_writes_fail() {
                created["warnings"] = json!([format!("flag_not_set: {FLAG_ERROR}")]);
            }
            emit_status(created)
        }
        "edit" => {
            let id = match parse_id(args.positional.first()) {
                Ok(id) => id,
                Err(o) => return o,
            };
            if args.flag("flagged") || args.flag("no-flagged") {
                // Mirror real remctl: the flag is private metadata, so `edit
                // --flagged` is rejected before anything is written.
                return fail(
                    "Error: synced tag replacement/removal, grocery, section, assignment, subtask, image, urgent, early-reminder, flagged, and location writes require --private.",
                );
            }
            let Some(pos) = reminders.iter().position(|r| r["id"].as_i64() == Some(id)) else {
                return fail_not_found(id);
            };
            let mut row = reminders[pos].clone();
            if let Some(t) = args.opt("title") {
                row["title"] = json!(t);
            }
            if let Some(n) = args.opt("notes") {
                row["notes"] = json!(n);
            }
            if let Some(p) = args.opt("priority") {
                let lower = p.to_lowercase();
                if !["high", "medium", "low", "none"].contains(&lower.as_str()) {
                    return fail(format!("Error: invalid priority '{p}'"));
                }
                row["priority"] = json!(lower);
            }
            if let Some(l) = args.opt("list") {
                let Some(idx) = find_list_index(&state, l) else {
                    return fail(format!("Error: list '{l}' not found"));
                };
                row["list"] = state["lists"][idx]["title"].clone();
            }
            if let Some(d) = args.opt("due")
                && let Err(outcome) = apply_due(&mut row, d, now)
            {
                return outcome;
            }
            state["reminders"][pos] = row;
            if let Err(e) = save_state(&state) {
                return fail(format!("Error: {e}"));
            }
            emit_status(json!({"status": "updated", "id": id}))
        }
        "done" | "undone" => {
            let id = match parse_id(args.positional.first()) {
                Ok(id) => id,
                Err(o) => return o,
            };
            let Some(pos) = reminders.iter().position(|r| r["id"].as_i64() == Some(id)) else {
                return fail_not_found(id);
            };
            let row = &mut state["reminders"][pos];
            let status = if command == "done" {
                row["completed"] = json!(true);
                row["completionDate"] = json!(now.format(ISO).to_string());
                "completed"
            } else {
                row["completed"] = json!(false);
                row.as_object_mut().unwrap().remove("completionDate");
                "uncompleted"
            };
            let title = row["title"].clone();
            if let Err(e) = save_state(&state) {
                return fail(format!("Error: {e}"));
            }
            emit_status(json!({"status": status, "id": id, "title": title}))
        }
        "flag" | "unflag" => {
            let id = match parse_id(args.positional.first()) {
                Ok(id) => id,
                Err(o) => return o,
            };
            let Some(pos) = reminders.iter().position(|r| r["id"].as_i64() == Some(id)) else {
                return fail_not_found(id);
            };
            if flag_writes_fail() {
                return fail_flag_write(id);
            }
            let flagged = command == "flag";
            state["reminders"][pos]["flagged"] = json!(flagged);
            let title = state["reminders"][pos]["title"].clone();
            if let Err(e) = save_state(&state) {
                return fail(format!("Error: {e}"));
            }
            emit_status(json!({
                "status": if flagged { "flagged" } else { "unflagged" },
                "id": id,
                "title": title,
            }))
        }
        "delete" => {
            let id = match parse_id(args.positional.first()) {
                Ok(id) => id,
                Err(o) => return o,
            };
            let Some(pos) = reminders.iter().position(|r| r["id"].as_i64() == Some(id)) else {
                return fail_not_found(id);
            };
            if !args.flag("force") {
                return ok("Cancelled.\n".into());
            }
            let row = state["reminders"].as_array_mut().unwrap().remove(pos);
            if let Err(e) = save_state(&state) {
                return fail(format!("Error: {e}"));
            }
            emit_status(json!({"status": "deleted", "id": id, "title": row["title"]}))
        }
        other => Outcome {
            stdout: String::new(),
            stderr: format!(
                "usage: remctl (fake) command ...\nremctl (fake): error: argument command: invalid choice: '{other}'\n"
            ),
            code: 2,
        },
    }
}

/// Process entry point: run, print, and return the exit code.
pub fn run(argv: Vec<String>) -> i32 {
    let rest: Vec<String> = argv.into_iter().skip(1).collect();
    let outcome = run_at(&rest, chrono::Local::now().naive_local());
    use std::io::Write;
    let _ = std::io::stdout().write_all(outcome.stdout.as_bytes());
    let _ = std::io::stderr().write_all(outcome.stderr.as_bytes());
    let _ = std::io::stdout().flush();
    outcome.code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 8, 12)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap() // Wednesday
    }

    #[test]
    fn due_grammar() {
        let n = now();
        assert_eq!(
            parse_due_input("2026-08-20", n),
            Ok(("2026-08-20T00:00:00".into(), true))
        );
        assert_eq!(
            parse_due_input("2026-08-20 9:05", n),
            Ok(("2026-08-20T09:05:00".into(), false))
        );
        assert_eq!(
            parse_due_input("2026-08-20T14:30", n),
            Ok(("2026-08-20T14:30:00".into(), false))
        );
        assert_eq!(
            parse_due_input("+3d", n),
            Ok(("2026-08-15T00:00:00".into(), true))
        );
        assert_eq!(
            parse_due_input("today", n),
            Ok(("2026-08-12T00:00:00".into(), true))
        );
        assert_eq!(
            parse_due_input("tonight", n),
            Ok(("2026-08-12T21:00:00".into(), false))
        );
        assert_eq!(
            parse_due_input("tomorrow 09:30", n),
            Ok(("2026-08-13T09:30:00".into(), false))
        );
        assert_eq!(
            parse_due_input("today at 3pm", n),
            Ok(("2026-08-12T15:00:00".into(), false))
        );
        assert_eq!(
            parse_due_input("fri 15:00", n),
            Ok(("2026-08-14T15:00:00".into(), false))
        );
        assert_eq!(
            parse_due_input("next wed", n),
            Ok(("2026-08-19T00:00:00".into(), true))
        );
        assert_eq!(
            parse_due_input("Monday", n),
            Ok(("2026-08-17T00:00:00".into(), true))
        );
        assert!(parse_due_input("someday", n).is_err());
        assert!(parse_due_input("2026-13-40", n).is_err());
    }

    #[test]
    fn python_repr_quotes() {
        assert_eq!(py_repr("someday"), "'someday'");
        assert_eq!(py_repr("it's"), "\"it's\"");
    }

    #[test]
    fn seed_has_eighteen_reminders() {
        let state = seed_state(now());
        assert_eq!(state["reminders"].as_array().unwrap().len(), 18);
        assert_eq!(state["lists"].as_array().unwrap().len(), 5);
        assert_eq!(state["reminders"][0]["id"], 101);
        assert_eq!(state["reminders"][17]["id"], 118);
        assert_eq!(
            state["reminders"][15]["url"],
            "https://bookclub.example.com/picks"
        );
        let timed = &state["reminders"][1];
        assert_eq!(
            (
                timed["dueDate"].as_str().unwrap(),
                timed["allDay"].as_bool().unwrap()
            ),
            ("2026-08-12T11:30:00", false)
        );
    }
}
