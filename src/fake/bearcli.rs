//! A local stand-in for bearcli, used for demo mode and tests.
//!
//! Implements the subset of bearcli that remtui drives — `search`, `cat`,
//! `edit`, and `app open` — with the real tool's contract: `--format json`
//! puts one JSON document on stdout for reads and `{"error": {...}}` for their
//! failures; writes print nothing on success and plain text on stderr (exit 1)
//! when they fail.
//!
//! State lives in a JSON file at `$REMTUI_FAKE_BEAR_STATE` (default
//! `~/.cache/remtui/demo-bear.json`), seeded with sample notes on first run.

use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value, json};

use super::{Args, write_atomic};
use crate::util::home_dir;

pub const ENV_STATE: &str = "REMTUI_FAKE_BEAR_STATE";
const ALL_FIELDS: [&str; 7] = [
    "id", "title", "locked", "tags", "length", "created", "modified",
];

static OPEN_TODO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*[-*+] \[ \] \S").unwrap());

pub fn state_path() -> PathBuf {
    match std::env::var(ENV_STATE) {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => home_dir()
            .join(".cache")
            .join("remtui")
            .join("demo-bear.json"),
    }
}

pub fn seed_state() -> Value {
    json!({"notes": [
        {
            "id": "NOTE-PLANNING",
            "title": "Sprint Planning",
            "tags": ["#work", "#work/sprint"],
            "locked": false,
            "created": "2026-08-01T09:00:00Z",
            "modified": "2026-09-01T15:30:00Z",
            "content": "# Sprint Planning\n#work/sprint\n\n## Tasks\n- [x] book the retro room\n- [ ] write the release notes\n- [ ] ask Priya about the API deprecation\n  - [ ] confirm the sunset date\n\n## Notes\nVelocity is holding steady.\n",
        },
        {
            "id": "NOTE-GARDEN",
            "title": "Garden Plan",
            "tags": ["#home", "#home/garden"],
            "locked": false,
            "created": "2026-07-12T09:00:00Z",
            "modified": "2026-08-28T10:00:00Z",
            "content": "# Garden Plan\n#home/garden\n\n- [ ] order bulbs for the front bed\n- [x] mulch the roses\n\n## Next spring\n- [ ] move the hydrangea\n```\n- [ ] this is inside a code block\n```\n",
        },
        {
            "id": "NOTE-READING",
            "title": "Reading Queue",
            "tags": ["#home"],
            "locked": false,
            "created": "2026-06-01T09:00:00Z",
            "modified": "2026-08-15T12:00:00Z",
            "content": "# Reading Queue\n#home\n\nNo tasks here, just titles.\n",
        },
        {
            "id": "NOTE-LOCKED",
            "title": "Private Journal",
            "tags": ["#home"],
            "locked": true,
            "created": "2026-05-01T09:00:00Z",
            "modified": "2026-08-01T12:00:00Z",
            "content": "# Private Journal\n#home\n\n- [ ] hidden behind the lock\n",
        },
    ]})
}

fn load_state() -> Result<Value, String> {
    let path = state_path();
    if path.exists() {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        return serde_json::from_str(&text).map_err(|e| e.to_string());
    }
    let state = seed_state();
    save_state(&state)?;
    Ok(state)
}

fn save_state(state: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    write_atomic(&state_path(), &text).map_err(|e| e.to_string())
}

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

fn fail_json(code: &str, message: &str) -> Outcome {
    Outcome {
        stdout: format!("{}\n", json!({"error": {"code": code, "message": message}})),
        stderr: String::new(),
        code: 1,
    }
}

fn fail_text(message: impl Into<String>) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{}\n", message.into()),
        code: 1,
    }
}

fn usage(message: &str) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("usage: bearcli (fake) ...\nbearcli (fake): error: {message}\n"),
        code: 2,
    }
}

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn locked(note: &Value) -> bool {
    note.get("locked").and_then(Value::as_bool).unwrap_or(false)
}

fn tags(note: &Value) -> Vec<String> {
    note.get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn project(note: &Value, fields: &str) -> Value {
    let requested: Vec<&str> = fields.split(',').collect();
    let mut wanted: Vec<&str> = if requested.first() == Some(&"all") {
        ALL_FIELDS.to_vec()
    } else {
        requested.clone()
    };
    if requested.contains(&"content") && !wanted.contains(&"content") {
        wanted.push("content");
    }
    let content = s(note, "content");
    let mut out = Map::new();
    for field in wanted {
        let value = match field {
            "length" => json!(content.chars().count()),
            // Locked notes return metadata only; the body is inaccessible.
            "content" => {
                if locked(note) {
                    Value::Null
                } else {
                    json!(content)
                }
            }
            // strings, as bearcli does
            "locked" => json!(if locked(note) { "yes" } else { "no" }),
            other => note.get(other).cloned().unwrap_or(Value::Null),
        };
        out.insert(field.to_string(), value);
    }
    Value::Object(out)
}

fn has_open_todo(note: &Value) -> bool {
    OPEN_TODO_RE.is_match(&s(note, "content"))
}

fn matches(note: &Value, query: &str) -> bool {
    for term in query.split_whitespace() {
        if term == "@todo" {
            if !has_open_todo(note) {
                return false;
            }
        } else if let Some(_tag) = term.strip_prefix('#') {
            let tag = term.to_lowercase();
            let hit = tags(note).iter().any(|t| {
                t.to_lowercase() == tag || t.to_lowercase().starts_with(&format!("{tag}/"))
            });
            if !hit {
                return false;
            }
        } else if term.starts_with('@') {
            continue; // other directives are not modeled
        } else {
            let haystack = format!("{}\n{}", s(note, "title"), s(note, "content")).to_lowercase();
            if !haystack.contains(&term.to_lowercase()) {
                return false;
            }
        }
    }
    true
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('\\') => {
                    chars.next();
                    out.push('\\');
                }
                Some('n') => {
                    chars.next();
                    out.push('\n');
                }
                Some('t') => {
                    chars.next();
                    out.push('\t');
                }
                Some('r') => {
                    chars.next();
                    out.push('\r');
                }
                _ => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Byte span of the section headed by `address` (heading line included).
fn section_span(content: &str, address: &str) -> Option<(usize, usize)> {
    let heading = squash(address);
    let level = heading.len() - heading.trim_start_matches('#').len();
    let mut start = None;
    let mut offset = 0;
    for line in content.split('\n') {
        match start {
            None => {
                if squash(line) == heading {
                    start = Some(offset);
                }
            }
            Some(begin) => {
                let stripped = line.trim_start();
                if stripped.starts_with('#') {
                    let this_level = stripped.len() - stripped.trim_start_matches('#').len();
                    if this_level <= level && stripped[this_level..].starts_with(' ') {
                        return Some((begin, offset));
                    }
                }
            }
        }
        offset += line.len() + 1;
    }
    start.map(|begin| (begin, content.len()))
}

pub fn run_args(argv: &[String]) -> Outcome {
    let Some((command, rest)) = argv.split_first() else {
        return usage("the following arguments are required: command");
    };
    let args = match Args::parse(
        rest,
        &[
            "query", "format", "fields", "section", "find", "replace", "header",
        ],
        &[],
    ) {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let mut state = match load_state() {
        Ok(s) => s,
        Err(e) => return fail_text(e),
    };
    let notes: Vec<Value> = state["notes"].as_array().cloned().unwrap_or_default();
    let find_note = |id: &str| notes.iter().position(|n| s(n, "id") == id);

    match command.as_str() {
        "search" => {
            let query = args
                .opt("query")
                .map(str::to_string)
                .or_else(|| args.positional.first().cloned())
                .unwrap_or_default();
            let rows: Vec<&Value> = notes.iter().filter(|n| matches(n, &query)).collect();
            if args.flag("count") {
                return ok(format!("{}\n", rows.len()));
            }
            if args.opt("format") != Some("json") {
                let lines: Vec<String> = rows
                    .iter()
                    .map(|n| format!("{}\t{}\t{}", s(n, "id"), s(n, "title"), tags(n).join(",")))
                    .collect();
                return ok(lines.into_iter().map(|l| format!("{l}\n")).collect());
            }
            let fields = args.opt("fields").unwrap_or("id,title,tags,length,matches");
            let out: Vec<Value> = rows.iter().map(|n| project(n, fields)).collect();
            ok(format!("{}\n", Value::Array(out)))
        }
        "cat" => {
            let Some(id) = args.positional.first() else {
                return usage("the following arguments are required: note_id");
            };
            let json_out = args.opt("format") == Some("json");
            let Some(pos) = find_note(id) else {
                return if json_out {
                    fail_json("not_found", &format!("Note {id} not found"))
                } else {
                    fail_text(format!("Note {id} not found"))
                };
            };
            let note = &notes[pos];
            if locked(note) {
                return if json_out {
                    fail_json("locked", "Note is locked; content is not accessible")
                } else {
                    fail_text("Note is locked; content is not accessible")
                };
            }
            let content = s(note, "content");
            if json_out {
                let hash: String = sha1::Sha1::digest(content.as_bytes())
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                ok(format!("{}\n", json!({"content": content, "hash": hash})))
            } else {
                ok(content)
            }
        }
        "edit" => {
            let Some(id) = args.positional.first() else {
                return usage("the following arguments are required: note_id");
            };
            let (Some(find), Some(replace)) = (args.opt("find"), args.opt("replace")) else {
                return usage("the following arguments are required: --find, --replace");
            };
            let Some(pos) = find_note(id) else {
                return fail_text(format!("Note {id} not found"));
            };
            if locked(&notes[pos]) {
                return fail_text("Note is locked; content is not accessible");
            }
            let content = s(&notes[pos], "content");
            let (lo, hi) = match args.opt("section") {
                Some(section) => match section_span(&content, &unescape(section)) {
                    Some(span) => span,
                    None => return fail_text(format!("Section not found: {section}")),
                },
                None => (0, content.len()),
            };
            let find_text = unescape(find);
            let replace_text = unescape(replace);
            let region = &content[lo..hi];
            let count = region.matches(find_text.as_str()).count();
            if count == 0 {
                return fail_text(format!("Text not found: {find}"));
            }
            if count > 1 {
                return fail_text(format!(
                    "Text matches {count} times; pass --all or a longer --find"
                ));
            }
            let new_content = format!(
                "{}{}{}",
                &content[..lo],
                region.replacen(&find_text, &replace_text, 1),
                &content[hi..]
            );
            state["notes"][pos]["content"] = json!(new_content);
            state["notes"][pos]["modified"] = json!("2026-09-03T12:00:00Z");
            match save_state(&state) {
                Ok(()) => ok(String::new()),
                Err(e) => fail_text(e),
            }
        }
        "app" => {
            if args.positional.first().map(String::as_str) != Some("open") {
                return usage("argument app_command: invalid choice");
            }
            let Some(id) = args.positional.get(1) else {
                return usage("the following arguments are required: note_id");
            };
            if find_note(id).is_none() {
                return fail_text(format!("Note {id} not found"));
            }
            // The real command foregrounds Bear; the fake records the request
            // so tests can see it happened.
            let record = json!({"id": id, "header": args.opt("header").unwrap_or("")});
            match state.get_mut("opened").and_then(Value::as_array_mut) {
                Some(opened) => opened.push(record),
                None => {
                    state["opened"] = json!([record]);
                }
            }
            match save_state(&state) {
                Ok(()) => ok(String::new()),
                Err(e) => fail_text(e),
            }
        }
        other => usage(&format!("argument command: invalid choice: '{other}'")),
    }
}

pub fn run(argv: Vec<String>) -> i32 {
    let rest: Vec<String> = argv.into_iter().skip(1).collect();
    let outcome = run_args(&rest);
    use std::io::Write;
    let _ = std::io::stdout().write_all(outcome.stdout.as_bytes());
    let _ = std::io::stderr().write_all(outcome.stderr.as_bytes());
    let _ = std::io::stdout().flush();
    outcome.code
}

use sha1::Digest;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_terms_are_anded() {
        let state = seed_state();
        let notes = state["notes"].as_array().unwrap();
        let planning = &notes[0];
        assert!(matches(planning, "@todo #work"));
        assert!(matches(planning, "@todo #WORK/sprint"));
        assert!(!matches(planning, "@todo #home"));
        assert!(!matches(&notes[2], "@todo"));
        assert!(matches(&notes[3], "@todo")); // locked note still matches; search projects content as null
        assert!(matches(planning, "velocity"));
        assert!(matches(planning, "@pinned velocity"));
    }

    #[test]
    fn projection_shapes() {
        let state = seed_state();
        let locked_note = &state["notes"][3];
        let p = project(locked_note, "id,title,tags,locked,content");
        assert_eq!(p["locked"], "yes");
        assert!(p["content"].is_null());
        let p = project(&state["notes"][0], "all");
        assert_eq!(p["locked"], "no");
        assert!(p.get("content").is_none());
        assert!(p["length"].as_u64().unwrap() > 10);
    }

    #[test]
    fn section_spans_and_unescape() {
        let content = "# T\n\n## A\nx\n### A1\ny\n## B\nz\n";
        let (lo, hi) = section_span(content, "## A").unwrap();
        assert_eq!(&content[lo..hi], "## A\nx\n### A1\ny\n");
        let (lo, hi) = section_span(content, "## B").unwrap();
        assert_eq!(&content[lo..hi], "## B\nz\n");
        assert!(section_span(content, "## C").is_none());
        assert_eq!(unescape("a\\\\b\\nc"), "a\\b\nc");
    }
}
