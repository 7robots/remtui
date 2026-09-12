//! Async wrapper around the remctl CLI.
//!
//! Reads use `--json` (bare arrays on stdout); mutations emit compact status
//! JSON. Errors arrive on stderr as either structured
//! `{"status": "error", "code": ..., "message": ...}` JSON or plain
//! `Error: ...` text; both are normalized into `RemctlError`.
//!
//! Reads run concurrently (remctl opens the store read-only); mutations are
//! serialized through a lock because concurrent EventKit writes race. Every
//! call has a timeout, which the Python remtui lacks: 30 s for reads and field
//! edits, 120 s for `flag`/`unflag`, which go through AppleScript and were
//! measured at 25 s steady state and 76 s cold.

use std::time::Duration;

use serde_json::Value;

use crate::models::{Reminder, ReminderList, parse_lists, parse_reminders};
use crate::util::which;

pub const ENV_COMMAND: &str = "REMTUI_REMCTL";
pub const DEFAULT_COMMAND: &str = "remctl";
/// The notes line prefix that marks a reminder as made from a Bear todo; the
/// full contract is in `crate::triage`.
pub const LINK_SEARCH: &str = "bear-todo:";
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);
pub const FLAG_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct RemctlError {
    pub message: String,
    pub code: String,
    pub exit_code: i32,
}

impl RemctlError {
    pub fn new(message: impl Into<String>) -> RemctlError {
        RemctlError {
            message: message.into(),
            code: String::new(),
            exit_code: 1,
        }
    }

    pub fn with_code(message: impl Into<String>, code: impl Into<String>) -> RemctlError {
        RemctlError {
            message: message.into(),
            code: code.into(),
            exit_code: 1,
        }
    }
}

/// Partial-failure warnings from a mutation payload.
///
/// remctl reports a write that mostly succeeded with exit 0 plus a `warnings`
/// array rather than an error: `add --flag` creates the reminder and reports
/// `["flag_not_set: ..."]` when only the separate AppleScript flag write
/// failed. Callers must surface these or the failure is invisible.
pub fn warnings_of(result: Option<&Value>) -> Vec<String> {
    let Some(Value::Object(map)) = result else {
        return Vec::new();
    };
    let items: Vec<Value> = match map.get("warnings") {
        Some(Value::String(s)) => vec![Value::String(s.clone())],
        Some(Value::Array(a)) => a.clone(),
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| match item {
            Value::Null | Value::Bool(false) => None,
            Value::String(s) if s.is_empty() => None,
            Value::String(s) => Some(s.clone()),
            Value::Number(n) if n.as_f64() == Some(0.0) => None,
            other => Some(other.to_string()),
        })
        .collect()
}

/// (message, code) from remctl stderr: structured JSON if present, otherwise
/// the first non-empty plain-text line.
fn parse_stderr(stderr: &str) -> (String, String) {
    for line in stderr.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        if let Ok(Value::Object(data)) = serde_json::from_str::<Value>(line)
            && data.get("status").and_then(Value::as_str) == Some("error")
        {
            let message = data
                .get("message")
                .and_then(Value::as_str)
                .filter(|m| !m.is_empty())
                .unwrap_or("remctl error");
            let code = data.get("code").and_then(Value::as_str).unwrap_or("");
            return (message.to_string(), code.to_string());
        }
    }
    if let Some(line) = stderr.lines().map(str::trim).find(|l| !l.is_empty()) {
        return (line.to_string(), String::new());
    }
    (
        "remctl failed with no error output".to_string(),
        String::new(),
    )
}

/// The remctl binary to run: `$REMTUI_REMCTL`, else `remctl` on PATH.
pub fn resolve_remctl() -> String {
    std::env::var(ENV_COMMAND)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_COMMAND.to_string())
}

pub fn remctl_found(command: &str) -> bool {
    which(command).is_some()
}

/// The id a flag write after a field edit should address: the `id` remctl
/// returned when a list move clone-deleted the reminder, else the original.
pub fn flag_target(result: Option<&Value>, reminder_id: i64) -> i64 {
    result
        .and_then(|r| r.get("id"))
        .and_then(|id| match id {
            Value::Number(n) => n.as_i64(),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        })
        .unwrap_or(reminder_id)
}

/// Which fields an `edit` changes. `None` leaves a field alone; `Some("")`
/// clears it (due) or resets it (priority).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditFields {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub due: Option<String>,
    pub priority: Option<String>,
    pub list_title: Option<String>,
    pub flagged: Option<bool>,
}

impl EditFields {
    pub fn is_empty(&self) -> bool {
        *self == EditFields::default()
    }

    /// Whether anything other than the flag changes.
    pub fn has_field_edit(&self) -> bool {
        self.title.is_some()
            || self.notes.is_some()
            || self.due.is_some()
            || self.priority.is_some()
            || self.list_title.is_some()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddFields {
    pub title: String,
    pub list_title: String,
    pub notes: String,
    pub due: String,
    pub priority: String,
    pub flagged: bool,
    pub tags: String,
    pub url: String,
}

/// Shells out to remctl with `--json` and parses the results.
pub struct RemctlClient {
    command: Vec<String>,
    envs: Vec<(String, String)>,
    write_lock: tokio::sync::Mutex<()>,
}

impl RemctlClient {
    pub fn new(command: Vec<String>) -> RemctlClient {
        RemctlClient::with_env(command, Vec::new())
    }

    pub fn with_env(command: Vec<String>, envs: Vec<(String, String)>) -> RemctlClient {
        RemctlClient {
            command,
            envs,
            write_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub fn command(&self) -> &[String] {
        &self.command
    }

    /// Run `remctl <sub> --json <rest...>`; `--json` goes right after the
    /// subcommand because trailing positionals may follow a `--` guard.
    async fn run(&self, args: &[&str], timeout: Duration) -> Result<Option<Value>, RemctlError> {
        let (program, prefix) = self
            .command
            .split_first()
            .ok_or_else(|| RemctlError::new("no remctl command"))?;
        let (sub, rest) = args
            .split_first()
            .ok_or_else(|| RemctlError::new("no remctl subcommand"))?;
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(prefix)
            .arg(sub)
            .arg("--json")
            .args(rest)
            .env("NO_COLOR", "1")
            .env("REMCTL_SKIP_ONBOARD", "1")
            .envs(self.envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let child = cmd.spawn().map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                RemctlError::with_code(
                    format!(
                        "remctl not found ({program}). Install it from https://github.com/viticci/remctl or run with --demo."
                    ),
                    "not_found",
                )
            } else {
                RemctlError::new(format!("could not run {program}: {err}"))
            }
        })?;
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(err)) => return Err(RemctlError::new(format!("remctl failed to run: {err}"))),
            Err(_) => {
                return Err(RemctlError::with_code(
                    format!("remctl did not answer within {} s", timeout.as_secs()),
                    "timeout",
                ));
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let (mut message, mut code) = parse_stderr(&stderr);
            if stderr.trim().is_empty()
                && let Ok(Value::Object(payload)) = serde_json::from_str::<Value>(&stdout)
                && let Some(m) = payload.get("message").and_then(Value::as_str)
                && !m.is_empty()
            {
                message = m.to_string();
                code = payload
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
            return Err(RemctlError {
                message,
                code,
                exit_code: output.status.code().unwrap_or(1),
            });
        }
        if stdout.trim().is_empty() {
            return Ok(None);
        }
        serde_json::from_str(&stdout)
            .map(Some)
            .map_err(|e| RemctlError::new(format!("remctl returned invalid JSON: {e}")))
    }

    async fn read(&self, args: &[&str]) -> Result<Value, RemctlError> {
        Ok(self
            .run(args, READ_TIMEOUT)
            .await?
            .unwrap_or(Value::Array(Vec::new())))
    }

    // -- reads --------------------------------------------------------------

    pub async fn get_lists(&self) -> Result<Vec<ReminderList>, RemctlError> {
        Ok(parse_lists(&self.read(&["lists"]).await?))
    }

    pub async fn get_reminders(
        &self,
        list_title: &str,
        include_completed: bool,
    ) -> Result<Vec<Reminder>, RemctlError> {
        let mut args = vec!["show"];
        if include_completed {
            args.push("--completed");
        }
        args.extend(["--", list_title]);
        Ok(parse_reminders(&self.read(&args).await?))
    }

    pub async fn today(&self) -> Result<Vec<Reminder>, RemctlError> {
        Ok(parse_reminders(&self.read(&["today"]).await?))
    }

    pub async fn upcoming(&self, days: u32) -> Result<Vec<Reminder>, RemctlError> {
        let days = days.to_string();
        Ok(parse_reminders(&self.read(&["upcoming", &days]).await?))
    }

    pub async fn overdue(&self) -> Result<Vec<Reminder>, RemctlError> {
        Ok(parse_reminders(&self.read(&["overdue"]).await?))
    }

    pub async fn flagged(&self) -> Result<Vec<Reminder>, RemctlError> {
        Ok(parse_reminders(&self.read(&["flagged"]).await?))
    }

    pub async fn search(
        &self,
        query: &str,
        include_completed: bool,
    ) -> Result<Vec<Reminder>, RemctlError> {
        let mut args = vec!["search"];
        if include_completed {
            args.push("--completed");
        }
        args.extend(["--", query]);
        Ok(parse_reminders(&self.read(&args).await?))
    }

    /// Every reminder, active or completed, created from a Bear todo. remctl
    /// searches notes text, so this finds them even after a move off the
    /// triage list.
    pub async fn linked_reminders(&self) -> Result<Vec<Reminder>, RemctlError> {
        self.search(LINK_SEARCH, true).await
    }

    // -- mutations ----------------------------------------------------------

    async fn mutate(&self, args: &[&str], timeout: Duration) -> Result<Option<Value>, RemctlError> {
        let _guard = self.write_lock.lock().await;
        self.run(args, timeout).await
    }

    pub async fn add(&self, fields: &AddFields) -> Result<Option<Value>, RemctlError> {
        let mut owned: Vec<String> = vec!["add".into()];
        if !fields.list_title.is_empty() {
            owned.push(format!("--list={}", fields.list_title));
        }
        if !fields.notes.is_empty() {
            owned.push(format!("--notes={}", fields.notes));
        }
        if !fields.due.is_empty() {
            owned.push(format!("--due={}", fields.due));
        }
        if !fields.priority.is_empty() && fields.priority != "none" {
            owned.push(format!("--priority={}", fields.priority));
        }
        if fields.flagged {
            owned.push("--flag".into());
        }
        if !fields.tags.is_empty() {
            owned.push(format!("--tags={}", fields.tags));
        }
        if !fields.url.is_empty() {
            owned.push(format!("--url={}", fields.url));
        }
        owned.push("--".into());
        owned.push(fields.title.clone());
        let args: Vec<&str> = owned.iter().map(String::as_str).collect();
        self.mutate(&args, READ_TIMEOUT).await
    }

    /// Apply the changed fields, leaving `None` ones alone.
    ///
    /// `flagged` is written with a separate flag/unflag call, not an
    /// `edit --flagged` argument: remctl classes the flag as private metadata
    /// and rejects `edit --flagged` without `--private`, which would fail the
    /// whole update. The field edit runs first so a pure list move can report
    /// the new id its clone-delete fallback assigns; the flag is written
    /// against that id.
    pub async fn edit(
        &self,
        reminder_id: i64,
        fields: &EditFields,
    ) -> Result<Option<Value>, RemctlError> {
        let mut owned: Vec<String> = vec!["edit".into(), reminder_id.to_string()];
        if let Some(t) = &fields.title {
            owned.push(format!("--title={t}"));
        }
        if let Some(n) = &fields.notes {
            owned.push(format!("--notes={n}"));
        }
        if let Some(d) = &fields.due {
            owned.push(format!("--due={}", if d.is_empty() { "clear" } else { d }));
        }
        if let Some(p) = &fields.priority {
            owned.push(format!(
                "--priority={}",
                if p.is_empty() { "none" } else { p }
            ));
        }
        if let Some(l) = &fields.list_title {
            owned.push(format!("--list={l}"));
        }
        let result = if owned.len() > 2 {
            let args: Vec<&str> = owned.iter().map(String::as_str).collect();
            self.mutate(&args, READ_TIMEOUT).await?
        } else {
            None
        };
        let Some(flagged) = fields.flagged else {
            return Ok(result);
        };
        let target = flag_target(result.as_ref(), reminder_id);
        let flag_result = if flagged {
            self.flag(target).await?
        } else {
            self.unflag(target).await?
        };
        Ok(result.or(flag_result))
    }

    pub async fn done(&self, reminder_id: i64) -> Result<Option<Value>, RemctlError> {
        self.mutate(&["done", &reminder_id.to_string()], READ_TIMEOUT)
            .await
    }

    pub async fn undone(&self, reminder_id: i64) -> Result<Option<Value>, RemctlError> {
        self.mutate(&["undone", &reminder_id.to_string()], READ_TIMEOUT)
            .await
    }

    pub async fn delete(&self, reminder_id: i64) -> Result<Option<Value>, RemctlError> {
        self.mutate(
            &["delete", &reminder_id.to_string(), "--force"],
            READ_TIMEOUT,
        )
        .await
    }

    pub async fn flag(&self, reminder_id: i64) -> Result<Option<Value>, RemctlError> {
        self.mutate(&["flag", &reminder_id.to_string()], FLAG_TIMEOUT)
            .await
    }

    pub async fn unflag(&self, reminder_id: i64) -> Result<Option<Value>, RemctlError> {
        self.mutate(&["unflag", &reminder_id.to_string()], FLAG_TIMEOUT)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn warnings_edge_cases() {
        assert!(warnings_of(None).is_empty());
        assert!(warnings_of(Some(&json!([1]))).is_empty());
        assert_eq!(warnings_of(Some(&json!({"warnings": "one"}))), vec!["one"]);
        assert_eq!(
            warnings_of(Some(&json!({"warnings": ["a", "", null, "b"]}))),
            vec!["a", "b"]
        );
        assert!(warnings_of(Some(&json!({"warnings": 5}))).is_empty());
        assert!(warnings_of(Some(&json!({"status": "created"}))).is_empty());
    }

    #[test]
    fn stderr_shapes() {
        assert_eq!(
            parse_stderr("Error: #5 not found\n"),
            ("Error: #5 not found".to_string(), String::new())
        );
        assert_eq!(
            parse_stderr(
                "noise\n{\"status\": \"error\", \"code\": \"x\", \"message\": \"boom\"}\n"
            ),
            ("boom".to_string(), "x".to_string())
        );
        assert_eq!(
            parse_stderr("{\"status\": \"ok\"}\n"),
            ("{\"status\": \"ok\"}".to_string(), String::new())
        );
        assert_eq!(parse_stderr("  \n").0, "remctl failed with no error output");
    }

    #[test]
    fn flag_follows_the_new_id_after_a_clone_delete_move() {
        let moved = json!({"status": "updated", "id": 777, "oldId": 1, "method": "clone-delete"});
        assert_eq!(flag_target(Some(&moved), 1), 777);
        assert_eq!(
            flag_target(Some(&json!({"status": "updated", "id": "12"})), 1),
            12
        );
        assert_eq!(flag_target(Some(&json!({"status": "updated"})), 42), 42);
        assert_eq!(flag_target(None, 42), 42);
        assert_eq!(flag_target(Some(&json!({"id": "FAKE-CK-9"})), 3), 3);
    }

    #[tokio::test]
    async fn missing_remctl_is_not_found() {
        let err = RemctlClient::new(vec!["/nonexistent/remctl".into()])
            .get_lists()
            .await
            .unwrap_err();
        assert_eq!(err.code, "not_found");
        assert!(err.message.contains("--demo"));
    }
}
