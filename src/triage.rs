//! Linking Bear todos to the reminders made from them.
//!
//! The link lives entirely on the Reminders side: a reminder created from a
//! todo carries, in its notes, the note's `bear://` link and a
//! `bear-todo: <key>` line. Nothing is written into Bear on add. Later triage
//! runs read the reminders back and join them to the current todos on that key.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::models::Reminder;
use crate::todos::Todo;

pub const KEY_PREFIX: &str = "bear-todo:";

static KEY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^bear-todo:\s*([0-9a-f]{6,40})\s*$").unwrap());

pub fn note_url(note_id: &str) -> String {
    format!("bear://x-callback-url/open-note?id={note_id}")
}

/// The notes block written into a reminder created from `todo`: a line for a
/// human, a clickable link, and the machine key `link_key` reads back.
pub fn link_notes(todo: &Todo) -> String {
    let first = if todo.note_title.is_empty() {
        "From Bear".to_string()
    } else {
        format!("From Bear: {}", todo.note_title)
    };
    format!(
        "{first}\n{}\n{KEY_PREFIX} {}",
        note_url(&todo.note_id),
        todo.key()
    )
}

/// The `bear-todo:` key in a reminder's notes, or "" when there is none.
pub fn link_key(notes: &str) -> String {
    KEY_RE
        .captures(notes)
        .map(|c| c[1].to_string())
        .unwrap_or_default()
}

/// What became of a todo in Reminders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    New,
    Added,
    Done,
}

/// One todo on the triage screen, with what became of it in Reminders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageRow {
    pub todo: Todo,
    pub reminder: Option<Reminder>,
}

impl TriageRow {
    pub fn status(&self) -> Status {
        match &self.reminder {
            None => Status::New,
            Some(r) if r.completed => Status::Done,
            Some(_) => Status::Added,
        }
    }
}

/// Pair each todo with the reminder carrying its key, keeping todo order.
///
/// When several reminders share a key (the same todo added twice, say), an
/// active one wins over a completed one so the row does not read as finished
/// while work is still open.
pub fn join(todos: &[Todo], reminders: &[Reminder]) -> Vec<TriageRow> {
    let mut by_key: HashMap<String, &Reminder> = HashMap::new();
    for reminder in reminders {
        let key = link_key(&reminder.notes);
        if key.is_empty() {
            continue;
        }
        match by_key.get(&key) {
            Some(current) if !(current.completed && !reminder.completed) => {}
            _ => {
                by_key.insert(key, reminder);
            }
        }
    }
    todos
        .iter()
        .map(|todo| TriageRow {
            todo: todo.clone(),
            reminder: by_key.get(&todo.key()).map(|r| (*r).clone()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t1() -> Todo {
        Todo::new(
            "N1",
            "Sprint",
            &["work"],
            "write the notes",
            "- [ ] write the notes",
            "## Tasks",
        )
    }

    fn t2() -> Todo {
        Todo::new(
            "N1",
            "Sprint",
            &["work"],
            "ship it",
            "- [ ] ship it",
            "## Tasks",
        )
    }

    fn reminder(id: i64, completed: bool, notes: &str) -> Reminder {
        Reminder {
            id,
            title: "x".into(),
            completed,
            notes: notes.into(),
            ..Reminder::default()
        }
    }

    #[test]
    fn link_notes_and_key_round_trip() {
        let notes = link_notes(&t1());
        assert_eq!(
            notes.lines().collect::<Vec<_>>(),
            vec![
                "From Bear: Sprint",
                &note_url("N1"),
                &format!("{KEY_PREFIX} {}", t1().key())
            ]
        );
        assert_eq!(link_key(&notes), t1().key());
        assert_eq!(link_key("no key here"), "");
        assert_eq!(link_key("bear-todo: not-hex"), "");
        assert_eq!(link_key("bear-todo: 9afbe7c1dc95"), "9afbe7c1dc95");
        let untitled = Todo::new("N1", "", &[], "x", "- [ ] x", "");
        assert!(link_notes(&untitled).starts_with("From Bear\n"));
    }

    #[test]
    fn join_prefers_an_active_duplicate() {
        let notes = link_notes(&t1());
        let active = reminder(1, false, &notes);
        let done = reminder(2, true, &notes);
        let todos = vec![t1(), t2()];
        let rows = join(&todos, std::slice::from_ref(&done));
        assert_eq!(rows[0].status(), Status::Done);
        assert_eq!(rows[1].status(), Status::New);
        let rows = join(&todos, &[done.clone(), active.clone()]);
        assert_eq!(
            (rows[0].status(), rows[0].reminder.as_ref().unwrap().id),
            (Status::Added, 1)
        );
        let rows = join(&todos, &[active, done]);
        assert_eq!(rows[0].reminder.as_ref().unwrap().id, 1);
        assert!(
            join(&todos, &[reminder(3, false, "plain")])
                .iter()
                .all(|r| r.reminder.is_none())
        );
    }
}
