//! Async wrapper around the bearcli CLI.
//!
//! bearcli reads and writes Bear's local database directly, so everything here
//! works with Bear closed except `open_note`, which asks the running app to
//! show a note. Reads use `--format json`, where every command (errors
//! included) emits one JSON document on stdout; writes print nothing on success
//! and plain text on stderr when they fail. Both shapes become `BearError`.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

use crate::todos::{Todo, TodoScan, parse_todos};
use crate::util::which;

pub const ENV_COMMAND: &str = "REMTUI_BEARCLI";
pub const DEFAULT_COMMAND: &str = "bearcli";
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const FIELDS: &str = "id,title,tags,locked,content";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct BearError {
    pub message: String,
    pub code: String,
    pub exit_code: i32,
}

impl BearError {
    pub fn new(message: impl Into<String>) -> BearError {
        BearError {
            message: message.into(),
            code: String::new(),
            exit_code: 1,
        }
    }
}

/// The bearcli binary to run: `$REMTUI_BEARCLI`, else the config value, else
/// `bearcli` on PATH. The environment beats the config here.
pub fn resolve_bearcli(configured: &str) -> String {
    std::env::var(ENV_COMMAND)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| {
            let c = configured.trim();
            (!c.is_empty()).then(|| c.to_string())
        })
        .unwrap_or_else(|| DEFAULT_COMMAND.to_string())
}

pub fn bearcli_found(configured: &str) -> bool {
    which(&resolve_bearcli(configured)).is_some()
}

/// Encode text for a bearcli flag that interprets `\n`, `\t`, `\r`, `\\`.
/// Literal backslashes in a note line (Bear writes `1\.` for a non-list "1.")
/// must be doubled or the CLI unescapes them and the find misses.
pub fn escape_flag(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

/// bearcli's JSON writes `locked` as the strings "yes"/"no", not booleans.
fn is_locked(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(s)) => matches!(s.trim().to_lowercase().as_str(), "yes" | "true" | "1"),
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        _ => false,
    }
}

fn value_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// Shells out to bearcli.
pub struct BearClient {
    command: Vec<String>,
    envs: Vec<(String, String)>,
    write_lock: tokio::sync::Mutex<()>,
}

impl BearClient {
    pub fn new(command: Vec<String>) -> BearClient {
        BearClient::with_env(command, Vec::new())
    }

    pub fn with_env(command: Vec<String>, envs: Vec<(String, String)>) -> BearClient {
        BearClient {
            command,
            envs,
            write_lock: tokio::sync::Mutex::new(()),
        }
    }

    async fn run(&self, args: &[&str], parse: bool) -> Result<Option<Value>, BearError> {
        let (program, prefix) = self
            .command
            .split_first()
            .ok_or_else(|| BearError::new("no bearcli command"))?;
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(prefix)
            .args(args)
            .envs(self.envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let child = cmd.spawn().map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                BearError {
                    message: format!(
                        "bearcli not found ({program}). It ships inside Bear.app; see the README."
                    ),
                    code: "not_found".into(),
                    exit_code: 1,
                }
            } else {
                BearError::new(format!("could not run {program}: {err}"))
            }
        })?;
        let output = match tokio::time::timeout(COMMAND_TIMEOUT, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(err)) => return Err(BearError::new(format!("bearcli failed to run: {err}"))),
            Err(_) => {
                return Err(BearError {
                    message: format!(
                        "bearcli did not answer within {} s",
                        COMMAND_TIMEOUT.as_secs()
                    ),
                    code: "timeout".into(),
                    exit_code: 1,
                });
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let exit_code = output.status.code().unwrap_or(1);
        let mut payload = None;
        if parse && !stdout.trim().is_empty() {
            let parsed: Value = serde_json::from_str(&stdout)
                .map_err(|e| BearError::new(format!("bearcli returned invalid JSON: {e}")))?;
            if let Value::Object(map) = &parsed
                && let Some(err) = map.get("error")
            {
                let message = err
                    .get("message")
                    .and_then(Value::as_str)
                    .filter(|m| !m.is_empty())
                    .unwrap_or("bearcli error");
                return Err(BearError {
                    message: message.to_string(),
                    code: err
                        .get("code")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    exit_code: if exit_code == 0 { 1 } else { exit_code },
                });
            }
            payload = Some(parsed);
        }
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let message = crate::util::first_line(&stderr);
            return Err(BearError {
                message: if message.is_empty() {
                    "bearcli failed with no error output".into()
                } else {
                    message
                },
                code: String::new(),
                exit_code,
            });
        }
        Ok(payload)
    }

    /// Every open todo in active notes, optionally only under some tags.
    ///
    /// Bear ANDs multiple `#tag` terms, so each tag is its own query and the
    /// results are merged by note id.
    pub async fn search_todo_notes(&self, tags: &[String]) -> Result<TodoScan, BearError> {
        let mut queries: Vec<String> = tags
            .iter()
            .map(|t| t.trim_matches(|c| c == '#' || c == ' '))
            .filter(|t| !t.is_empty())
            .map(|t| format!("@todo #{t}"))
            .collect();
        if queries.is_empty() {
            queries.push("@todo".into());
        }
        let mut notes: BTreeMap<String, Value> = BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        for query in &queries {
            let rows = self
                .run(
                    &["search", query, "--format", "json", "--fields", FIELDS],
                    true,
                )
                .await?;
            if let Some(Value::Array(rows)) = rows {
                for row in rows {
                    let id = value_text(row.get("id"));
                    if let std::collections::btree_map::Entry::Vacant(slot) =
                        notes.entry(id.clone())
                    {
                        order.push(id);
                        slot.insert(row);
                    }
                }
            }
        }
        let mut scan = TodoScan::default();
        for id in order {
            let row = &notes[&id];
            let content = row.get("content");
            if is_locked(row.get("locked")) || content.is_none_or(Value::is_null) {
                scan.locked += 1;
                continue;
            }
            let tags: Vec<String> = row
                .get("tags")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|t| value_text(Some(t))).collect())
                .unwrap_or_default();
            scan.todos.extend(parse_todos(
                &value_text(content),
                &id,
                &value_text(row.get("title")),
                &tags,
            ));
        }
        Ok(scan)
    }

    /// Flip the todo's `[ ]` to `[x]` in its note, scoped to its section.
    pub async fn complete_todo(&self, todo: &Todo) -> Result<(), BearError> {
        let section = escape_flag(&todo.section);
        let find = escape_flag(&todo.line);
        let replace = escape_flag(&todo.done_line());
        let mut args = vec!["edit", todo.note_id.as_str()];
        if !todo.section.is_empty() {
            args.extend(["--section", section.as_str()]);
        }
        args.extend(["--find", find.as_str(), "--replace", replace.as_str()]);
        let _guard = self.write_lock.lock().await;
        self.run(&args, false).await.map(|_| ())
    }

    /// Show the note in Bear, scrolled to the todo's section.
    pub async fn open_note(&self, todo: &Todo) -> Result<(), BearError> {
        let header = todo.header();
        let mut args = vec!["app", "open", todo.note_id.as_str()];
        if !header.is_empty() {
            args.extend(["--header", header.as_str()]);
        }
        self.run(&args, false).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_flag_doubles_backslashes() {
        assert_eq!(escape_flag("a\\b\nc\td\r"), "a\\\\b\\nc\\td\\r");
    }

    #[test]
    fn locked_strings() {
        assert!(is_locked(Some(&Value::String("yes".into()))));
        assert!(is_locked(Some(&Value::String(" TRUE ".into()))));
        assert!(!is_locked(Some(&Value::String("no".into()))));
        assert!(!is_locked(None));
    }

    #[tokio::test]
    async fn missing_bearcli_is_not_found() {
        let err = BearClient::new(vec!["/nonexistent/bearcli".into()])
            .search_todo_notes(&[])
            .await
            .unwrap_err();
        assert_eq!(err.code, "not_found");
    }
}
