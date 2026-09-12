//! Open todo items in Bear notes: the model, the parser, and the keys.
//!
//! The key a reminder carries (`bear-todo: <key>`) is identical to the Python
//! remtui's and to bjorn's, so existing reminders join up unchanged.

use std::sync::LazyLock;

use regex::Regex;
use sha1::Digest;

static TODO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<indent>\s*)(?P<bullet>[-*+]) \[ \]\s+(?P<text>\S.*?)\s*$").unwrap()
});
static HEADING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#{1,6} \S").unwrap());
static FENCE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(```|~~~)").unwrap());

/// One open `- [ ]` line in a Bear note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Todo {
    pub note_id: String,
    pub note_title: String,
    pub note_tags: Vec<String>,
    pub text: String,
    /// The full line as written, indentation included: what `edit --find` needs.
    pub line: String,
    /// The nearest heading line above the todo, `#` markers and all, or "" when
    /// the todo sits above every heading. Doubles as a bearcli section address.
    pub section: String,
}

impl Todo {
    pub fn new(
        note_id: &str,
        note_title: &str,
        note_tags: &[&str],
        text: &str,
        line: &str,
        section: &str,
    ) -> Todo {
        Todo {
            note_id: note_id.into(),
            note_title: note_title.into(),
            note_tags: note_tags.iter().map(|t| t.to_string()).collect(),
            text: text.into(),
            line: line.into(),
            section: section.into(),
        }
    }

    pub fn normalized(&self) -> String {
        self.text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    /// Stable id shared with the reminder created from this todo. Rewording
    /// the item in Bear yields a new key (and orphans the old reminder).
    pub fn key(&self) -> String {
        let digest =
            sha1::Sha1::digest(format!("{}\n{}", self.note_id, self.normalized()).as_bytes());
        digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()[..12]
            .to_string()
    }

    pub fn done_line(&self) -> String {
        self.line.replacen("[ ]", "[x]", 1)
    }

    /// The section heading without its `#` markers, for `app open --header`.
    pub fn header(&self) -> String {
        self.section.trim_start_matches('#').trim().to_string()
    }

    /// Leading whitespace of the line, for indenting the row.
    pub fn indent(&self) -> usize {
        self.line.len() - self.line.trim_start().len()
    }
}

pub fn is_fence(line: &str) -> bool {
    FENCE_RE.is_match(line)
}

/// Extract open todos from a note body, in document order.
///
/// Fenced code blocks are skipped. Nested todos count like top-level ones; a
/// checked parent does not hide its open children.
pub fn parse_todos(
    content: &str,
    note_id: &str,
    note_title: &str,
    note_tags: &[String],
) -> Vec<Todo> {
    let mut todos = Vec::new();
    let mut section = String::new();
    let mut in_fence = false;
    for raw in content.lines() {
        if is_fence(raw) {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if HEADING_RE.is_match(raw) {
            section = raw.trim().to_string();
            continue;
        }
        let Some(caps) = TODO_RE.captures(raw) else {
            continue;
        };
        todos.push(Todo {
            note_id: note_id.to_string(),
            note_title: note_title.to_string(),
            note_tags: note_tags.to_vec(),
            text: caps["text"].to_string(),
            line: raw.trim_end().to_string(),
            section: section.clone(),
        });
    }
    todos
}

/// What a triage read of Bear produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TodoScan {
    pub todos: Vec<Todo>,
    /// Notes whose content bearcli could not read (locked or encrypted).
    pub locked: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "# Sprint\n#work\n\n- [ ] top level, no section\n## Tasks\n- [x] done already\n- [ ] write the release notes\n  - [ ] nested child\n* [ ] star bullet\n-[ ] not a todo\n- [ ]\n1. not a todo\n```\n- [ ] inside a fence\n```\n### Sub\n- [ ]  double  spaced   text  \n";

    #[test]
    fn parse_todos_in_order_with_sections() {
        let todos = parse_todos(BODY, "N", "Sprint", &["work".to_string()]);
        let texts: Vec<&str> = todos.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "top level, no section",
                "write the release notes",
                "nested child",
                "star bullet",
                "double  spaced   text"
            ]
        );
        let sections: Vec<&str> = todos.iter().map(|t| t.section.as_str()).collect();
        assert_eq!(
            sections,
            vec!["# Sprint", "## Tasks", "## Tasks", "## Tasks", "### Sub"]
        );
        assert_eq!(todos[2].line, "  - [ ] nested child");
        assert_eq!(todos[2].done_line(), "  - [x] nested child");
        assert_eq!(todos[2].indent(), 2);
        assert_eq!(todos[1].header(), "Tasks");
    }

    #[test]
    fn key_matches_remtui_scheme_and_ignores_spacing() {
        let a = Todo::new(
            "N",
            "T",
            &[],
            "Write  the notes",
            "- [ ] Write  the notes",
            "",
        );
        let b = Todo::new(
            "N",
            "T",
            &[],
            "write the notes",
            "- [ ] write the notes",
            "",
        );
        let c = Todo::new(
            "M",
            "T",
            &[],
            "write the notes",
            "- [ ] write the notes",
            "",
        );
        assert_eq!(a.key(), b.key());
        assert_eq!(a.key().len(), 12);
        assert_ne!(a.key(), c.key());
        let digest = sha1::Sha1::digest(b"N\nwrite the notes");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(a.key(), hex[..12]);
    }
}
