//! The add/edit form: fields, focus order, key handling. Saving and the
//! editor round trip are driven by `App`; this is display state plus editing.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::field::Field;
use crate::client::{AddFields, EditFields};
use crate::models::{Priority, Reminder};

pub const DUE_PLACEHOLDER: &str = "tomorrow 09:30 · +3d · none";
pub const TITLE_PLACEHOLDER: &str = "What needs doing?";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Title,
    Notes,
    Due,
    Priority,
    List,
    Flag,
    Buttons,
}

const ORDER: [Focus; 7] = [
    Focus::Title,
    Focus::Notes,
    Focus::Due,
    Focus::Priority,
    Focus::List,
    Focus::Flag,
    Focus::Buttons,
];

pub const BUTTONS: [&str; 3] = ["Editor", "Cancel", "Save"];

/// A small multi-line editor for the notes field.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Notes {
    pub text: String,
    /// Char index into `text`.
    pub cursor: usize,
}

impl Notes {
    pub fn new(text: &str) -> Notes {
        Notes {
            text: text.to_string(),
            cursor: text.chars().count(),
        }
    }

    fn byte_index(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    pub fn set(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.chars().count();
    }

    pub fn insert(&mut self, ch: char) {
        let at = self.byte_index(self.cursor);
        self.text.insert(at, ch);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let at = self.byte_index(self.cursor - 1);
            self.text.remove(at);
            self.cursor -= 1;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.text.chars().count() {
            let at = self.byte_index(self.cursor);
            self.text.remove(at);
        }
    }

    /// (line, column) of the cursor, in chars.
    pub fn position(&self) -> (usize, usize) {
        let before: String = self.text.chars().take(self.cursor).collect();
        let line = before.matches('\n').count();
        let col = before
            .rsplit('\n')
            .next()
            .map(|s| s.chars().count())
            .unwrap_or(0);
        (line, col)
    }

    pub fn lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self.text.split('\n').map(str::to_string).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }

    fn move_to(&mut self, line: usize, col: usize) {
        let lines = self.lines();
        let line = line.min(lines.len() - 1);
        let col = col.min(lines[line].chars().count());
        let mut index = 0;
        for l in lines.iter().take(line) {
            index += l.chars().count() + 1;
        }
        self.cursor = index + col;
    }

    pub fn up(&mut self) {
        let (line, col) = self.position();
        if line > 0 {
            self.move_to(line - 1, col);
        }
    }

    pub fn down(&mut self) {
        let (line, col) = self.position();
        self.move_to(line + 1, col);
    }

    pub fn handle(&mut self, key: &KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char(c) if !ctrl => self.insert(c),
            KeyCode::Enter => self.insert('\n'),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.text.chars().count()),
            KeyCode::Up => self.up(),
            KeyCode::Down => self.down(),
            KeyCode::Home => {
                let (line, _) = self.position();
                self.move_to(line, 0);
            }
            KeyCode::End => {
                let (line, _) = self.position();
                self.move_to(line, usize::MAX);
            }
            _ => return false,
        }
        true
    }
}

/// What the form does when a key is pressed that is not plain editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormEvent {
    None,
    Save,
    Cancel,
    Editor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    /// The reminder being edited, or None for a new one.
    pub reminder: Option<Reminder>,
    pub title: Field,
    pub notes: Notes,
    pub due: Field,
    pub priority: Priority,
    pub lists: Vec<String>,
    /// Index into `lists`; None is the blank option (only when no default).
    pub list: Option<usize>,
    pub flagged: bool,
    pub focus: Focus,
    pub button: usize,
    pub error: String,
    pub saving: bool,
}

/// The current due date in a form remctl can parse back.
pub fn prefill_due(reminder: &Reminder) -> String {
    match reminder.due() {
        None => String::new(),
        Some(due) => {
            use chrono::Timelike;
            if reminder.all_day || (due.hour(), due.minute()) == (0, 0) {
                due.format("%Y-%m-%d").to_string()
            } else {
                due.format("%Y-%m-%d %H:%M").to_string()
            }
        }
    }
}

impl Form {
    pub fn new(reminder: Option<&Reminder>, list_titles: &[String], default_list: &str) -> Form {
        let mut lists: Vec<String> = list_titles.to_vec();
        let current = reminder
            .map(|r| r.list_name.clone())
            .unwrap_or_else(|| default_list.to_string());
        if !current.is_empty() && !lists.contains(&current) {
            lists.insert(0, current.clone());
        }
        let list = if current.is_empty() {
            None
        } else {
            lists.iter().position(|l| *l == current)
        };
        Form {
            reminder: reminder.cloned(),
            title: Field::new(reminder.map(|r| r.title.as_str()).unwrap_or("")),
            notes: Notes::new(reminder.map(|r| r.notes.as_str()).unwrap_or("")),
            due: Field::new(&reminder.map(prefill_due).unwrap_or_default()),
            priority: reminder.map(|r| r.priority).unwrap_or_default(),
            lists,
            list,
            flagged: reminder.is_some_and(|r| r.flagged),
            focus: Focus::Title,
            button: 2,
            error: String::new(),
            saving: false,
        }
    }

    pub fn is_edit(&self) -> bool {
        self.reminder.is_some()
    }

    pub fn heading(&self) -> &'static str {
        if self.is_edit() {
            "✎ Edit Reminder"
        } else {
            "＋ New Reminder"
        }
    }

    pub fn save_label(&self) -> &'static str {
        if self.is_edit() { "Save" } else { "Add" }
    }

    pub fn list_title(&self) -> String {
        self.list
            .and_then(|i| self.lists.get(i).cloned())
            .unwrap_or_default()
    }

    fn step_focus(&mut self, forward: bool) {
        let index = ORDER.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = if forward {
            (index + 1) % ORDER.len()
        } else {
            (index + ORDER.len() - 1) % ORDER.len()
        };
        self.focus = ORDER[next];
    }

    fn cycle_list(&mut self, forward: bool) {
        if self.lists.is_empty() {
            return;
        }
        let allow_blank = self.reminder.is_none() && self.list.is_none();
        self.list = match (self.list, forward) {
            (None, _) => Some(0),
            (Some(i), true) if i + 1 < self.lists.len() => Some(i + 1),
            (Some(_), true) => {
                if allow_blank {
                    None
                } else {
                    Some(0)
                }
            }
            (Some(0), false) => Some(self.lists.len() - 1),
            (Some(i), false) => Some(i - 1),
        };
    }

    fn cycle_priority(&mut self, forward: bool) {
        let all = Priority::ALL;
        let index = all.iter().position(|p| *p == self.priority).unwrap_or(0);
        let next = if forward {
            (index + 1) % all.len()
        } else {
            (index + all.len() - 1) % all.len()
        };
        self.priority = all[next];
    }

    /// Apply a key; the dialog shortcuts win over field editing, as the
    /// Python form's priority bindings did.
    pub fn handle(&mut self, key: &KeyEvent) -> FormEvent {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (key.code, ctrl) {
            (KeyCode::Esc, _) => return FormEvent::Cancel,
            (KeyCode::Char('s'), true) => return FormEvent::Save,
            (KeyCode::Char('e'), true) => return FormEvent::Editor,
            (KeyCode::Tab, _) => {
                self.step_focus(true);
                return FormEvent::None;
            }
            (KeyCode::BackTab, _) => {
                self.step_focus(false);
                return FormEvent::None;
            }
            _ => {}
        }
        if self.saving {
            return FormEvent::None;
        }
        match self.focus {
            Focus::Title => match key.code {
                KeyCode::Enter => return FormEvent::Save,
                KeyCode::Down => self.step_focus(true),
                KeyCode::Up => self.step_focus(false),
                _ => {
                    self.title.handle(key);
                }
            },
            Focus::Due => match key.code {
                KeyCode::Enter => return FormEvent::Save,
                KeyCode::Down => self.step_focus(true),
                KeyCode::Up => self.step_focus(false),
                _ => {
                    self.due.handle(key);
                }
            },
            Focus::Notes => {
                self.notes.handle(key);
            }
            Focus::Priority => match key.code {
                KeyCode::Right | KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Down => {
                    self.cycle_priority(true)
                }
                KeyCode::Left | KeyCode::Up => self.cycle_priority(false),
                _ => {}
            },
            Focus::List => match key.code {
                KeyCode::Right | KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Down => {
                    self.cycle_list(true)
                }
                KeyCode::Left | KeyCode::Up => self.cycle_list(false),
                _ => {}
            },
            Focus::Flag => match key.code {
                KeyCode::Char(' ') | KeyCode::Enter => self.flagged = !self.flagged,
                KeyCode::Down => self.step_focus(true),
                KeyCode::Up => self.step_focus(false),
                _ => {}
            },
            Focus::Buttons => match key.code {
                KeyCode::Left => self.button = self.button.saturating_sub(1),
                KeyCode::Right => self.button = (self.button + 1).min(BUTTONS.len() - 1),
                KeyCode::Up => self.step_focus(false),
                KeyCode::Enter | KeyCode::Char(' ') => {
                    return match self.button {
                        0 => FormEvent::Editor,
                        1 => FormEvent::Cancel,
                        _ => FormEvent::Save,
                    };
                }
                _ => {}
            },
        }
        FormEvent::None
    }

    /// Validate; on failure the error is set and the title focused.
    pub fn validate(&mut self) -> bool {
        if self.title.value.trim().is_empty() {
            self.error = "⚠ A title is required.".into();
            self.focus = Focus::Title;
            return false;
        }
        true
    }

    pub fn add_fields(&self) -> AddFields {
        AddFields {
            title: self.title.value.trim().to_string(),
            list_title: self.list_title(),
            notes: self.notes.text.trim_end().to_string(),
            due: self.due.value.trim().to_string(),
            priority: self.priority.as_str().to_string(),
            flagged: self.flagged,
            tags: String::new(),
            url: String::new(),
        }
    }

    /// Only the fields that changed, for an edit.
    pub fn edit_fields(&self) -> EditFields {
        let Some(r) = &self.reminder else {
            return EditFields::default();
        };
        let mut fields = EditFields::default();
        let title = self.title.value.trim().to_string();
        if title != r.title {
            fields.title = Some(title);
        }
        let notes = self.notes.text.trim_end().to_string();
        if notes != r.notes.trim_end() {
            fields.notes = Some(notes);
        }
        let due = self.due.value.trim().to_string();
        if due != prefill_due(r) {
            fields.due = Some(due);
        }
        if self.priority != r.priority {
            fields.priority = Some(self.priority.as_str().to_string());
        }
        let list = self.list_title();
        if !list.is_empty() && list != r.list_name {
            fields.list_title = Some(list);
        }
        if self.flagged != r.flagged {
            fields.flagged = Some(self.flagged);
        }
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn notes_editor_moves_by_line() {
        let mut n = Notes::new("ab\ncde");
        assert_eq!(n.position(), (1, 3));
        n.up();
        assert_eq!(n.position(), (0, 2));
        n.insert('X');
        assert_eq!(n.text, "abX\ncde");
        n.down();
        assert_eq!(n.position(), (1, 3));
        n.handle(&key(KeyCode::Enter));
        assert_eq!(n.text, "abX\ncde\n");
        assert_eq!(n.lines().len(), 3);
    }

    #[test]
    fn edit_sends_only_changes_and_prefills_due() {
        let r = Reminder {
            id: 1,
            title: "T".into(),
            notes: "n\n".into(),
            due_raw: "2026-08-12T00:00:00".into(),
            all_day: true,
            list_name: "Work".into(),
            priority: Priority::Low,
            ..Reminder::default()
        };
        let lists = vec!["Personal".to_string(), "Work".to_string()];
        let mut f = Form::new(Some(&r), &lists, "");
        assert_eq!(f.due.value, "2026-08-12");
        assert_eq!(f.list, Some(1));
        assert!(f.edit_fields().is_empty());
        f.title = Field::new("T2");
        f.due.clear();
        f.flagged = true;
        let fields = f.edit_fields();
        assert_eq!(fields.title.as_deref(), Some("T2"));
        assert_eq!(fields.due.as_deref(), Some(""));
        assert_eq!(fields.flagged, Some(true));
        assert!(fields.notes.is_none() && fields.priority.is_none() && fields.list_title.is_none());
        let timed = Reminder {
            due_raw: "2026-08-12T09:30:00".into(),
            all_day: false,
            ..r.clone()
        };
        assert_eq!(prefill_due(&timed), "2026-08-12 09:30");
    }

    #[test]
    fn new_form_focus_order_and_cycles() {
        let lists = vec!["Personal".to_string(), "Work".to_string()];
        let mut f = Form::new(None, &lists, "Work");
        assert_eq!(f.list, Some(1));
        assert_eq!(f.focus, Focus::Title);
        f.handle(&key(KeyCode::Tab));
        assert_eq!(f.focus, Focus::Notes);
        for _ in 0..5 {
            f.handle(&key(KeyCode::Tab));
        }
        assert_eq!(f.focus, Focus::Buttons);
        f.handle(&key(KeyCode::Tab));
        assert_eq!(f.focus, Focus::Title);
        f.focus = Focus::Priority;
        f.handle(&key(KeyCode::Char(' ')));
        assert_eq!(f.priority, Priority::Low);
        f.focus = Focus::List;
        f.handle(&key(KeyCode::Right));
        assert_eq!(
            f.list,
            Some(0),
            "wraps without a blank when there was a default"
        );
        f.focus = Focus::Flag;
        f.handle(&key(KeyCode::Char(' ')));
        assert!(f.flagged);
        assert_eq!(
            f.handle(&KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            FormEvent::Save
        );
        assert_eq!(f.handle(&key(KeyCode::Esc)), FormEvent::Cancel);
        assert!(!f.validate());
        assert_eq!(f.focus, Focus::Title);
        let unknown = Form::new(None, &lists, "Elsewhere");
        assert_eq!(unknown.lists[0], "Elsewhere");
        let blank = Form::new(None, &lists, "");
        assert_eq!(blank.list, None);
    }
}
