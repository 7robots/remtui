//! Date parsing and humanization for reminder due dates.
//!
//! Every function takes an explicit `now` so the rules are testable; callers
//! pass `Local::now().naive_local()`.

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

/// Parse a due date string from remctl JSON output.
///
/// Accepts ISO 8601 forms, with or without a time component or timezone, and
/// returns a naive local datetime; None when empty or unparseable.
pub fn parse_due(value: &str) -> Option<NaiveDateTime> {
    let text = value.trim();
    if text.is_empty() {
        return None;
    }
    let text = if let Some(stripped) = text.strip_suffix('Z') {
        format!("{stripped}+00:00")
    } else {
        text.to_string()
    };
    if let Ok(aware) = DateTime::parse_from_rfc3339(&text) {
        return Some(aware.with_timezone(&Local).naive_local());
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f%:z",
        "%Y-%m-%d %H:%M:%S%.f%:z",
        "%Y-%m-%dT%H:%M%:z",
        "%Y-%m-%d %H:%M%:z",
    ] {
        if let Ok(aware) = DateTime::parse_from_str(&text, fmt) {
            return Some(aware.with_timezone(&Local).naive_local());
        }
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&text, fmt) {
            return Some(naive);
        }
    }
    NaiveDate::parse_from_str(&text, "%Y-%m-%d")
        .ok()
        .map(|d| d.and_time(NaiveTime::MIN))
}

/// A compact human label: `Today 14:00`, `Tomorrow`, `Yesterday`, `Fri 21:30`,
/// `Jun 12`, `Jan 2, 2027`. All-day (or exactly midnight) drops the time.
pub fn humanize_due(due: Option<NaiveDateTime>, all_day: bool, now: NaiveDateTime) -> String {
    let Some(due) = due else {
        return String::new();
    };
    let today = now.date();
    let day = due.date();
    let delta_days = (day - today).num_days();
    let label = match delta_days {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        2..=6 => WEEKDAYS[day.weekday().num_days_from_monday() as usize].to_string(),
        _ if day.year() == today.year() => format!("{} {}", month_abbr(day), day.day()),
        _ => format!("{} {}, {}", month_abbr(day), day.day(), day.year()),
    };
    if all_day || (due.hour(), due.minute(), due.second()) == (0, 0, 0) {
        return label;
    }
    format!("{label} {}", due.format("%H:%M"))
}

fn month_abbr(day: NaiveDate) -> &'static str {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    MONTHS[day.month0() as usize]
}

/// Overdue once the due moment has passed; an all-day reminder only after its
/// day ends (the allDay flag is authoritative: a timed reminder can be due 00:00).
pub fn is_overdue(due: Option<NaiveDateTime>, all_day: bool, now: NaiveDateTime) -> bool {
    match due {
        None => false,
        Some(due) if all_day => due.date() < now.date(),
        Some(due) => due < now,
    }
}

pub fn is_due_today(due: Option<NaiveDateTime>, now: NaiveDateTime) -> bool {
    due.is_some_and(|d| d.date() == now.date())
}

/// Dated first (ascending), undated last.
pub fn sort_key(due: Option<NaiveDateTime>) -> (u8, NaiveDateTime) {
    match due {
        Some(d) => (0, d),
        None => (1, NaiveDateTime::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    #[test]
    fn parse_due_accepts_iso_shapes() {
        assert_eq!(parse_due("2026-07-03"), Some(at(2026, 7, 3, 0, 0)));
        assert_eq!(parse_due("2026-07-03 14:30"), Some(at(2026, 7, 3, 14, 30)));
        assert_eq!(
            parse_due("2026-07-03T14:30:00"),
            Some(at(2026, 7, 3, 14, 30))
        );
        assert_eq!(parse_due(""), None);
        assert_eq!(parse_due("nonsense"), None);
        // Aware stamps come back as local wall-clock time.
        let z = parse_due("2026-07-03T14:30:00Z").unwrap();
        let expected = DateTime::parse_from_rfc3339("2026-07-03T14:30:00+00:00")
            .unwrap()
            .with_timezone(&Local)
            .naive_local();
        assert_eq!(z, expected);
    }

    #[test]
    fn humanize_labels() {
        let now = at(2026, 8, 12, 9, 0); // a Wednesday
        assert_eq!(
            humanize_due(Some(at(2026, 8, 12, 11, 30)), false, now),
            "Today 11:30"
        );
        assert_eq!(
            humanize_due(Some(at(2026, 8, 12, 0, 0)), true, now),
            "Today"
        );
        assert_eq!(
            humanize_due(Some(at(2026, 8, 13, 0, 0)), false, now),
            "Tomorrow"
        );
        assert_eq!(
            humanize_due(Some(at(2026, 8, 11, 10, 0)), false, now),
            "Yesterday 10:00"
        );
        assert_eq!(
            humanize_due(Some(at(2026, 8, 17, 9, 30)), false, now),
            "Mon 09:30"
        );
        assert_eq!(humanize_due(Some(at(2026, 8, 15, 0, 0)), true, now), "Sat");
        assert_eq!(
            humanize_due(Some(at(2026, 8, 19, 0, 0)), true, now),
            "Aug 19"
        );
        assert_eq!(humanize_due(Some(at(2026, 8, 9, 0, 0)), true, now), "Aug 9");
        assert_eq!(
            humanize_due(Some(at(2027, 1, 2, 0, 0)), true, now),
            "Jan 2, 2027"
        );
        assert_eq!(humanize_due(None, false, now), "");
    }

    #[test]
    fn overdue_rules() {
        let now = at(2026, 8, 12, 12, 0);
        assert!(!is_overdue(None, false, now));
        assert!(!is_overdue(Some(at(2026, 8, 12, 0, 0)), true, now));
        assert!(is_overdue(Some(at(2026, 8, 12, 0, 0)), false, now));
        assert!(is_overdue(Some(at(2026, 8, 11, 0, 0)), true, now));
        assert!(!is_overdue(Some(at(2026, 8, 12, 13, 0)), false, now));
        assert!(is_due_today(Some(at(2026, 8, 12, 23, 0)), now));
        assert!(!is_due_today(Some(at(2026, 8, 13, 0, 0)), now));
    }

    #[test]
    fn undated_sorts_last() {
        assert!(sort_key(Some(at(2030, 1, 1, 0, 0))) < sort_key(None));
    }
}
