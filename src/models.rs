//! Data models mirroring remctl's `--json` output schemas.

use chrono::NaiveDateTime;
use serde_json::Value;

use crate::dates::{parse_due, sort_key};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Priority {
    High,
    Medium,
    Low,
    #[default]
    None,
}

impl Priority {
    pub fn parse(text: &str) -> Priority {
        match text {
            "high" => Priority::High,
            "medium" => Priority::Medium,
            "low" => Priority::Low,
            _ => Priority::None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
            Priority::None => "none",
        }
    }

    /// `none → low → medium → high → none`, the `p` key's cycle.
    pub fn next(self) -> Priority {
        match self {
            Priority::None => Priority::Low,
            Priority::Low => Priority::Medium,
            Priority::Medium => Priority::High,
            Priority::High => Priority::None,
        }
    }

    /// The `!!!` / `!!` / `!` mark shown before a title, or "".
    pub fn mark(self) -> &'static str {
        match self {
            Priority::High => "!!!",
            Priority::Medium => "!!",
            Priority::Low => "!",
            Priority::None => "",
        }
    }

    /// The form's option label.
    pub fn label(self) -> &'static str {
        match self {
            Priority::High => "!!! High",
            Priority::Medium => "!! Medium",
            Priority::Low => "! Low",
            Priority::None => "None",
        }
    }

    pub const ALL: [Priority; 4] = [
        Priority::None,
        Priority::Low,
        Priority::Medium,
        Priority::High,
    ];
}

/// One reminder, as serialized by remctl.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reminder {
    pub id: i64,
    pub title: String,
    pub list_name: String,
    pub completed: bool,
    pub flagged: bool,
    pub urgent: bool,
    pub priority: Priority,
    pub notes: String,
    pub url: String,
    pub section: String,
    pub tags: Vec<String>,
    pub due_raw: String,
    pub all_day: bool,
    pub subtask_count: i64,
    pub is_subtask: bool,
    pub recurring: bool,
}

fn str_of(data: &Value, key: &str) -> String {
    match data.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
    }
}

fn truthy(data: &Value, key: &str) -> bool {
    match data.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn int_of(data: &Value, key: &str) -> i64 {
    match data.get(key) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

impl Reminder {
    pub fn from_json(data: &Value) -> Reminder {
        // remctl's `dueDate` for an all-day reminder is UTC midnight rendered in
        // local time (20:00 the evening before, in the eastern US), so the date
        // is wrong; `displayDate` is the local wall-clock moment Reminders
        // shows. Prefer it whenever remctl provides it.
        let mut due_raw = str_of(data, "displayDate");
        if due_raw.is_empty() {
            due_raw = str_of(data, "dueDate");
        }
        Reminder {
            id: int_of(data, "id"),
            title: str_of(data, "title"),
            list_name: str_of(data, "list"),
            completed: truthy(data, "completed"),
            flagged: truthy(data, "flagged"),
            urgent: truthy(data, "urgent"),
            priority: Priority::parse(&str_of(data, "priority")),
            notes: str_of(data, "notes"),
            url: str_of(data, "url"),
            section: str_of(data, "section"),
            tags: data
                .get("tags")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|t| match t {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            due_raw,
            all_day: truthy(data, "allDay"),
            subtask_count: int_of(data, "subtaskCount"),
            is_subtask: truthy(data, "isSubtask"),
            recurring: truthy(data, "recurrence"),
        }
    }

    pub fn due(&self) -> Option<NaiveDateTime> {
        parse_due(&self.due_raw)
    }

    /// Sort: active before completed, then due date, priority, title.
    pub fn display_key(&self) -> (bool, (u8, NaiveDateTime), Priority, String) {
        (
            self.completed,
            sort_key(self.due()),
            self.priority,
            self.title.to_lowercase(),
        )
    }

    /// Case-insensitive substring match on title, notes, and tags.
    pub fn matches(&self, query: &str) -> bool {
        let needle = query.to_lowercase();
        self.title.to_lowercase().contains(&needle)
            || self.notes.to_lowercase().contains(&needle)
            || self.tags.join(" ").to_lowercase().contains(&needle)
    }
}

/// One reminder list, as serialized by remctl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderList {
    pub id: i64,
    pub title: String,
    pub color_name: String,
    pub color_hex: String,
    pub emoji: String,
    pub is_group: bool,
    pub is_groceries: bool,
    pub pinned: bool,
    pub active: i64,
    pub completed: i64,
    pub total: i64,
}

impl Default for ReminderList {
    fn default() -> Self {
        ReminderList {
            id: 0,
            title: String::new(),
            color_name: "blue".into(),
            color_hex: "#007AFF".into(),
            emoji: String::new(),
            is_group: false,
            is_groceries: false,
            pinned: false,
            active: 0,
            completed: 0,
            total: 0,
        }
    }
}

impl ReminderList {
    pub fn from_json(data: &Value) -> ReminderList {
        let empty = Value::Object(Default::default());
        let color = data
            .get("color")
            .filter(|v| v.is_object())
            .unwrap_or(&empty);
        let badge = data
            .get("badge")
            .filter(|v| v.is_object())
            .unwrap_or(&empty);
        let counts = data
            .get("counts")
            .filter(|v| v.is_object())
            .unwrap_or(&empty);
        let or = |v: String, d: &str| if v.is_empty() { d.to_string() } else { v };
        ReminderList {
            id: int_of(data, "id"),
            title: str_of(data, "title"),
            color_name: or(str_of(color, "name"), "blue"),
            color_hex: or(str_of(color, "hex"), "#007AFF"),
            emoji: str_of(badge, "emoji"),
            is_group: truthy(data, "isGroup"),
            is_groceries: truthy(data, "isGroceries"),
            pinned: truthy(data, "pinned"),
            active: int_of(counts, "active"),
            completed: int_of(counts, "completed"),
            total: int_of(counts, "total"),
        }
    }
}

fn rows(payload: &Value) -> Vec<&Value> {
    match payload {
        Value::Array(rows) => rows.iter().filter(|r| r.is_object()).collect(),
        Value::Object(map) => map
            .get("reminders")
            .or_else(|| map.get("results"))
            .or_else(|| map.get("lists"))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter(|r| r.is_object()).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Parse `lists --json`: drop group containers, dedupe the flattened child
/// duplicates by id, keep the original order.
pub fn parse_lists(payload: &Value) -> Vec<ReminderList> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in rows(payload) {
        let list = ReminderList::from_json(item);
        if list.is_group || !seen.insert(list.id) {
            continue;
        }
        out.push(list);
    }
    out
}

pub fn parse_reminders(payload: &Value) -> Vec<Reminder> {
    let mut out: Vec<Reminder> = rows(payload).into_iter().map(Reminder::from_json).collect();
    out.sort_by_cached_key(Reminder::display_key);
    out
}

/// The four smart views, in sidebar order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SmartView {
    Today,
    Upcoming,
    Overdue,
    Flagged,
}

impl SmartView {
    pub const ALL: [SmartView; 4] = [
        SmartView::Today,
        SmartView::Upcoming,
        SmartView::Overdue,
        SmartView::Flagged,
    ];

    pub fn key(self) -> &'static str {
        match self {
            SmartView::Today => "today",
            SmartView::Upcoming => "upcoming",
            SmartView::Overdue => "overdue",
            SmartView::Flagged => "flagged",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SmartView::Today => "Today",
            SmartView::Upcoming => "Upcoming",
            SmartView::Overdue => "Overdue",
            SmartView::Flagged => "Flagged",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            SmartView::Today => "◉",
            SmartView::Upcoming => "◷",
            SmartView::Overdue => "⚠",
            SmartView::Flagged => "⚑",
        }
    }

    pub fn color_hex(self) -> &'static str {
        match self {
            SmartView::Today => "#0A84FF",
            SmartView::Upcoming => "#BF5AF2",
            SmartView::Overdue => "#FF453A",
            SmartView::Flagged => "#FF9F0A",
        }
    }

    pub fn empty_message(self) -> &'static str {
        match self {
            SmartView::Today => "Nothing due today — enjoy it",
            SmartView::Upcoming => "Nothing scheduled in the next 7 days",
            SmartView::Overdue => "Nothing overdue — inbox zero energy",
            SmartView::Flagged => "No flagged reminders",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reminder_from_json_prefers_display_date() {
        let r = Reminder::from_json(&json!({
            "id": 5, "title": "x", "list": "Work", "completed": false, "flagged": true,
            "priority": "high", "dueDate": "2026-08-11T20:00:00", "displayDate": "2026-08-12T00:00:00",
            "allDay": true, "tags": ["a", "b"], "recurrence": {"frequency": "daily"}, "subtaskCount": 2
        }));
        assert_eq!(r.due_raw, "2026-08-12T00:00:00");
        assert!(r.all_day && r.flagged && r.recurring);
        assert_eq!(r.priority, Priority::High);
        assert_eq!(r.tags, vec!["a", "b"]);
        assert_eq!(r.subtask_count, 2);
        assert!(r.matches("B"));
        assert!(!r.matches("zzz"));
        let plain = Reminder::from_json(&json!({"id": "7", "title": "y", "priority": "weird"}));
        assert_eq!((plain.id, plain.priority), (7, Priority::None));
    }

    #[test]
    fn lists_drop_groups_and_duplicates_in_order() {
        let lists = parse_lists(&json!([
            {"id": 1, "title": "Personal", "color": {"name": "blue", "hex": "#007AFF"}},
            {"id": 9, "title": "Group", "isGroup": true},
            {"id": 2, "title": "Work", "color": {"name": "red", "hex": "#FF3B30"}, "badge": {"emoji": "💼"},
             "counts": {"active": 3, "completed": 1, "total": 4}},
            {"id": 1, "title": "Personal"}
        ]));
        let titles: Vec<&str> = lists.iter().map(|l| l.title.as_str()).collect();
        assert_eq!(titles, vec!["Personal", "Work"]);
        assert_eq!(lists[1].emoji, "💼");
        assert_eq!(
            (lists[1].active, lists[1].completed, lists[1].total),
            (3, 1, 4)
        );
        assert_eq!(lists[0].color_hex, "#007AFF");
    }

    #[test]
    fn reminders_sort_active_due_priority_title() {
        let rows = parse_reminders(&json!([
            {"id": 1, "title": "b undated", "priority": "none"},
            {"id": 2, "title": "done", "completed": true, "dueDate": "2020-01-01T00:00:00"},
            {"id": 3, "title": "a undated high", "priority": "high"},
            {"id": 4, "title": "dated", "dueDate": "2030-01-01T09:00:00"},
        ]));
        let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![4, 3, 1, 2]);
    }

    #[test]
    fn priority_cycle_and_marks() {
        assert_eq!(Priority::None.next(), Priority::Low);
        assert_eq!(Priority::High.next(), Priority::None);
        assert_eq!(Priority::Medium.mark(), "!!");
        assert_eq!(Priority::parse("low").as_str(), "low");
    }
}
