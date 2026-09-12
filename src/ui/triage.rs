//! The Bear triage screen: rows grouped under their notes, marks, filter,
//! cursor. Every action (load, add, open, tick) is executed by `App`; this is
//! display state.

use std::collections::HashSet;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::field::Field;
use super::rows::fit;
use super::theme;
use crate::dates::humanize_due;
use crate::triage::{Status, TriageRow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageLine {
    Blank,
    Header {
        title: String,
        tags: Vec<String>,
    },
    /// Index into `Triage::rows`.
    Todo(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Triage {
    pub rows: Vec<TriageRow>,
    pub locked: usize,
    /// Todo keys marked for adding.
    pub marked: HashSet<String>,
    pub filter_text: String,
    pub filter: Option<Field>,
    pub filter_focused: bool,
    pub lines: Vec<TriageLine>,
    /// Index into `lines`, always on a `Todo` line when there is one.
    pub cursor: usize,
    pub scroll: usize,
    pub error: String,
    pub added_any: bool,
    pub adding: bool,
    pub loading: bool,
    pub loaded: bool,
    pub target_list: String,
    pub due: String,
}

impl Triage {
    pub fn new(target_list: &str, due: &str) -> Triage {
        Triage {
            rows: Vec::new(),
            locked: 0,
            marked: HashSet::new(),
            filter_text: String::new(),
            filter: None,
            filter_focused: false,
            lines: Vec::new(),
            cursor: 0,
            scroll: 0,
            error: String::new(),
            added_any: false,
            adding: false,
            loading: false,
            loaded: false,
            target_list: target_list.to_string(),
            due: due.to_string(),
        }
    }

    /// Replace the rows after a load or a re-join, keeping marks only on
    /// rows that are still new, and the cursor on the same todo.
    pub fn show(&mut self, rows: Vec<TriageRow>, locked: usize) {
        let keep = self
            .current_row()
            .map(|r| (r.todo.key(), r.todo.line.clone()));
        self.rows = rows;
        self.locked = locked;
        let new_keys: HashSet<String> = self
            .rows
            .iter()
            .filter(|r| r.status() == Status::New)
            .map(|r| r.todo.key())
            .collect();
        self.marked.retain(|k| new_keys.contains(k));
        self.rebuild(keep);
    }

    fn visible(&self) -> Vec<usize> {
        let needle = self.filter_text.to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                needle.is_empty()
                    || row.todo.text.to_lowercase().contains(&needle)
                    || row.todo.note_title.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Regroup the visible rows under note headers.
    pub fn rebuild(&mut self, keep: Option<(String, String)>) {
        let mut lines = Vec::new();
        let mut note_id: Option<&str> = None;
        let mut cursor = None;
        for index in self.visible() {
            let row = &self.rows[index];
            if note_id != Some(row.todo.note_id.as_str()) {
                if note_id.is_some() {
                    lines.push(TriageLine::Blank);
                }
                note_id = Some(row.todo.note_id.as_str());
                lines.push(TriageLine::Header {
                    title: if row.todo.note_title.is_empty() {
                        "(untitled)".into()
                    } else {
                        row.todo.note_title.clone()
                    },
                    tags: row.todo.note_tags.clone(),
                });
            }
            if cursor.is_none()
                && let Some((key, line)) = &keep
                && row.todo.key() == *key
                && row.todo.line == *line
            {
                cursor = Some(lines.len());
            }
            lines.push(TriageLine::Todo(index));
        }
        self.lines = lines;
        self.cursor = cursor.unwrap_or(1).min(self.lines.len().saturating_sub(1));
        if !self.lines.is_empty() && !matches!(self.lines[self.cursor], TriageLine::Todo(_)) {
            self.step(1);
        }
    }

    pub fn current_row(&self) -> Option<&TriageRow> {
        match self.lines.get(self.cursor) {
            Some(TriageLine::Todo(i)) => self.rows.get(*i),
            _ => None,
        }
    }

    /// Move the cursor to the next todo line in `direction`.
    pub fn step(&mut self, direction: i64) {
        let len = self.lines.len() as i64;
        let mut index = self.cursor as i64;
        loop {
            index += direction;
            if index < 0 || index >= len {
                return;
            }
            if matches!(self.lines[index as usize], TriageLine::Todo(_)) {
                self.cursor = index as usize;
                return;
            }
        }
    }

    pub fn toggle_mark(&mut self, key: &str) {
        if !self.marked.remove(key) {
            self.marked.insert(key.to_string());
        }
    }

    pub fn stats(&self) -> String {
        let notes: HashSet<&str> = self.rows.iter().map(|r| r.todo.note_id.as_str()).collect();
        let added = self
            .rows
            .iter()
            .filter(|r| r.status() == Status::Added)
            .count();
        let done = self
            .rows
            .iter()
            .filter(|r| r.status() == Status::Done)
            .count();
        let mut parts = vec![
            format!(
                "{} todo{} in {} note{}",
                self.rows.len(),
                if self.rows.len() == 1 { "" } else { "s" },
                notes.len(),
                if notes.len() == 1 { "" } else { "s" }
            ),
            format!("{added} added"),
            format!("{done} done"),
        ];
        if !self.marked.is_empty() {
            parts.push(format!("{} marked", self.marked.len()));
        }
        if !self.filter_text.is_empty() {
            parts.push(format!(
                "filter \"{}\" → {}",
                self.filter_text,
                self.visible().len()
            ));
        }
        if self.locked > 0 {
            parts.push(format!("{} locked skipped", self.locked));
        }
        let target = if self.target_list.is_empty() {
            "current list"
        } else {
            &self.target_list
        };
        parts.push(format!("add → {target}, due {}", self.due));
        parts.join(" · ")
    }

    pub fn empty_message(&self) -> String {
        if !self.filter_text.is_empty() {
            format!("○  nothing matches \"{}\"", self.filter_text)
        } else if !self.rows.is_empty() {
            "○  nothing to show".into()
        } else {
            "✓  no open todos in Bear".into()
        }
    }

    /// Todo keys to add: the marked ones, else the current new row's.
    pub fn add_targets(&self) -> Result<HashSet<String>, &'static str> {
        if !self.marked.is_empty() {
            return Ok(self.marked.clone());
        }
        match self.current_row() {
            Some(row) if row.status() == Status::New => Ok(HashSet::from([row.todo.key()])),
            _ => Err("Mark a todo first (space)"),
        }
    }

    pub fn render_header(title: &str, tags: &[String], width: usize) -> Line<'static> {
        let mut spans = vec![Span::styled(
            title.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        )];
        if !tags.is_empty() {
            let used = UnicodeWidthStr::width(title) + 2;
            spans.push(Span::styled(
                format!("  {}", fit(&tags.join(" "), width.saturating_sub(used))),
                Style::default().fg(theme::TAG).add_modifier(Modifier::DIM),
            ));
        }
        Line::from(spans)
    }

    pub fn render_row(
        &self,
        row: &TriageRow,
        width: usize,
        now: chrono::NaiveDateTime,
    ) -> Line<'static> {
        let marked = self.marked.contains(&row.todo.key());
        let mut left: Vec<Span<'static>> = Vec::new();
        let indent = row.todo.indent();
        if indent > 0 {
            left.push(Span::raw(" ".repeat(indent)));
        }
        match row.status() {
            Status::New => left.push(if marked {
                Span::styled("◆ ", theme::fg(theme::FLAG))
            } else {
                Span::styled("◯ ", theme::dim())
            }),
            Status::Done => left.push(Span::styled("⬤ ", theme::fg(theme::DONE))),
            Status::Added => left.push(Span::styled("⬤ ", theme::fg(theme::TODAY))),
        }
        let right: Option<Span<'static>> = match (row.status(), &row.reminder) {
            (Status::Added, Some(reminder)) => {
                let due = match reminder.due() {
                    Some(_) => humanize_due(reminder.due(), reminder.all_day, now),
                    None => "no date".into(),
                };
                Some(Span::styled(format!("⏰ {due}"), theme::fg(theme::TODAY)))
            }
            (Status::Done, _) => Some(Span::styled("✓ done", theme::fg(theme::DONE))),
            _ => None,
        };
        let right_width = right
            .as_ref()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()) + 2)
            .unwrap_or(0);
        let used: usize = left
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        let text = fit(&row.todo.text, width.saturating_sub(used + right_width));
        let text_width = UnicodeWidthStr::width(text.as_str());
        left.push(Span::styled(
            text,
            if marked {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ));
        if let Some(right) = right {
            let pad = width.saturating_sub(used + text_width + right_width - 2);
            left.push(Span::raw(" ".repeat(pad)));
            left.push(right);
        }
        Line::from(left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todos::Todo;

    fn row(note: &str, text: &str) -> TriageRow {
        TriageRow {
            todo: Todo::new(
                note,
                &format!("Note {note}"),
                &["#t"],
                text,
                &format!("- [ ] {text}"),
                "",
            ),
            reminder: None,
        }
    }

    #[test]
    fn groups_rows_and_steps_over_headers() {
        let mut t = Triage::new("Work", "today");
        t.show(vec![row("A", "one"), row("A", "two"), row("B", "three")], 1);
        assert_eq!(t.lines.len(), 6);
        assert_eq!(t.cursor, 1);
        assert_eq!(t.current_row().unwrap().todo.text, "one");
        t.step(1);
        t.step(1);
        assert_eq!(t.current_row().unwrap().todo.text, "three");
        t.step(1);
        assert_eq!(t.current_row().unwrap().todo.text, "three");
        assert_eq!(
            t.stats(),
            "3 todos in 2 notes · 0 added · 0 done · 1 locked skipped · add → Work, due today"
        );
        t.toggle_mark(&row("A", "two").todo.key());
        assert!(t.stats().contains("1 marked"));
        assert_eq!(t.add_targets().unwrap().len(), 1);
        t.filter_text = "thr".into();
        t.rebuild(None);
        assert_eq!(t.lines.len(), 2);
        assert!(t.stats().contains("filter \"thr\" → 1"));
        t.filter_text = "zzz".into();
        t.rebuild(None);
        assert_eq!(t.empty_message(), "○  nothing matches \"zzz\"");
    }
}
