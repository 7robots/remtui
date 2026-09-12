//! Rendering of one reminder row and one sidebar row.

use chrono::NaiveDateTime;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::theme;
use crate::dates::{humanize_due, is_due_today, is_overdue};
use crate::models::{Priority, Reminder, ReminderList, SmartView};

fn priority_color(priority: Priority) -> ratatui::style::Color {
    match priority {
        Priority::High => theme::ERROR,
        Priority::Medium => theme::ACCENT,
        _ => theme::WARNING,
    }
}

/// The width of a line's text in cells.
pub fn line_width(line: &Line) -> usize {
    line.spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum()
}

/// A reminder row as one or two lines: check | title + badges … meta, then an
/// optional details line. `pending` marks a flag write in flight.
pub fn reminder_lines(
    r: &Reminder,
    now: NaiveDateTime,
    width: usize,
    pending: bool,
) -> Vec<Line<'static>> {
    let due = r.due();
    let overdue = !r.completed && is_overdue(due, r.all_day, now);
    let check = if pending {
        Span::styled("◌", theme::fg(theme::ACCENT))
    } else if r.completed {
        Span::styled("⬤", theme::fg(theme::DONE))
    } else if overdue {
        Span::styled("◯", theme::fg(theme::OVERDUE))
    } else {
        Span::styled("◯", theme::dim())
    };

    let mut meta: Vec<Span<'static>> = Vec::new();
    if r.urgent {
        meta.push(Span::styled("⏰ ", theme::fg(theme::FLAG)));
    }
    if r.flagged {
        meta.push(Span::styled("⚑ ", theme::fg(theme::FLAG)));
    }
    if due.is_some() {
        let label = humanize_due(due, r.all_day, now);
        let style = if r.completed {
            theme::dim()
        } else if overdue {
            Style::default()
                .fg(theme::OVERDUE)
                .add_modifier(Modifier::BOLD)
        } else if is_due_today(due, now) {
            theme::fg(theme::TODAY)
        } else {
            theme::dim()
        };
        meta.push(Span::styled(label, style));
    }
    let meta_width: usize = meta
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();

    let mut body: Vec<Span<'static>> = Vec::new();
    if !r.completed && r.priority != Priority::None {
        body.push(Span::styled(
            format!("{} ", r.priority.mark()),
            Style::default()
                .fg(priority_color(r.priority))
                .add_modifier(Modifier::BOLD),
        ));
    }
    let title_style = if r.completed {
        Style::default().add_modifier(Modifier::DIM | Modifier::CROSSED_OUT)
    } else {
        Style::default()
    };
    // check (1) + two spaces, a gap of two before meta
    let budget = width.saturating_sub(3 + if meta_width > 0 { meta_width + 2 } else { 0 });
    let fixed: usize = body
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum::<usize>()
        + if r.recurring { 2 } else { 0 }
        + if r.subtask_count > 0 {
            2 + r.subtask_count.to_string().len()
        } else {
            0
        };
    let title = fit(&r.title, budget.saturating_sub(fixed));
    body.push(Span::styled(title, title_style));
    if r.recurring {
        body.push(Span::styled(
            " ↻",
            Style::default().fg(theme::TAG).add_modifier(Modifier::DIM),
        ));
    }
    if r.subtask_count > 0 {
        body.push(Span::styled(format!(" ⤷{}", r.subtask_count), theme::dim()));
    }
    let body_width: usize = body
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    let pad = width.saturating_sub(3 + body_width + meta_width);

    let mut first = vec![check, Span::raw("  ")];
    first.extend(body);
    first.push(Span::raw(" ".repeat(pad)));
    first.extend(meta);
    let mut lines = vec![Line::from(first)];

    let mut details: Vec<Span<'static>> = Vec::new();
    let sep = || Span::styled(" · ", theme::dim());
    if !r.section.is_empty() {
        details.push(Span::styled(
            format!("§ {}", r.section),
            Style::default().add_modifier(Modifier::DIM | Modifier::ITALIC),
        ));
    }
    if !r.notes.is_empty() {
        let first_line = r.notes.lines().next().unwrap_or("");
        let shown = if first_line.chars().count() > 72 {
            format!("{}…", first_line.chars().take(71).collect::<String>())
        } else {
            first_line.to_string()
        };
        if !details.is_empty() {
            details.push(sep());
        }
        details.push(Span::styled(shown, theme::dim()));
    }
    if !r.tags.is_empty() {
        if !details.is_empty() {
            details.push(sep());
        }
        details.push(Span::styled(
            r.tags
                .iter()
                .map(|t| format!("#{t}"))
                .collect::<Vec<_>>()
                .join(" "),
            Style::default().fg(theme::TAG).add_modifier(Modifier::DIM),
        ));
    }
    if !r.url.is_empty() {
        if !details.is_empty() {
            details.push(sep());
        }
        details.push(Span::styled("🔗", theme::dim()));
    }
    if !details.is_empty() {
        let mut second = vec![Span::raw("   ")];
        let mut used = 3;
        for span in details {
            let w = UnicodeWidthStr::width(span.content.as_ref());
            if used + w > width {
                let room = width.saturating_sub(used);
                if room > 1 {
                    second.push(Span::styled(fit(&span.content, room), span.style));
                }
                break;
            }
            used += w;
            second.push(span);
        }
        lines.push(Line::from(second));
    }
    lines
}

/// Cut text to `width` cells with an ellipsis.
pub fn fit(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = UnicodeWidthStr::width(c.to_string().as_str());
        if used + w > width - 1 {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push('…');
    out
}

pub fn nav_header(label: &str) -> Line<'static> {
    Line::from(Span::styled(
        label.to_uppercase(),
        Style::default()
            .fg(theme::NAV_HEADER)
            .add_modifier(Modifier::BOLD),
    ))
}

pub fn smart_row(view: SmartView, width: usize) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{} ", view.icon()),
            theme::fg(theme::hex(view.color_hex())),
        ),
        Span::raw(fit(view.label(), width.saturating_sub(2))),
    ])
}

pub fn list_row(list: &ReminderList, width: usize) -> Line<'static> {
    let count = if list.active > 0 {
        list.active.to_string()
    } else {
        String::new()
    };
    let mut left = vec![Span::styled("● ", theme::fg(theme::hex(&list.color_hex)))];
    let mut used = 2;
    if !list.emoji.is_empty() {
        left.push(Span::raw(format!("{} ", list.emoji)));
        used += UnicodeWidthStr::width(list.emoji.as_str()) + 1;
    }
    let room = width.saturating_sub(used + if count.is_empty() { 0 } else { count.len() + 1 });
    let title = fit(&list.title, room);
    used += UnicodeWidthStr::width(title.as_str());
    left.push(Span::raw(title));
    let pad = width.saturating_sub(used + count.len());
    left.push(Span::raw(" ".repeat(pad)));
    left.push(Span::styled(count, theme::dim()));
    Line::from(left)
}

/// The sidebar wordmark: "RemTUI" in box glyphs, one color per letter.
pub fn logo() -> Vec<Line<'static>> {
    const WORDMARK: [[&str; 3]; 6] = [
        ["╦═╗", "╠╦╝", "╩╚═"],
        ["┌─┐", "├┤ ", "└─┘"],
        ["┌┬┐", "│││", "┴ ┴"],
        ["╔╦╗", " ║ ", " ╩ "],
        ["╦ ╦", "║ ║", "╚═╝"],
        ["╦", "║", "╩"],
    ];
    (0..3)
        .map(|row| {
            let mut spans = Vec::new();
            for (index, (glyph, color)) in
                WORDMARK.iter().zip(theme::APPLE_RAINBOW.iter()).enumerate()
            {
                if index > 0 {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(
                    glyph[row].to_string(),
                    Style::default().fg(*color).add_modifier(Modifier::BOLD),
                ));
            }
            Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 8, 12)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap()
    }

    #[test]
    fn row_shows_priority_flag_due_and_details() {
        let r = Reminder {
            id: 1,
            title: "Renew passport".into(),
            priority: Priority::High,
            flagged: true,
            due_raw: "2026-08-09T00:00:00".into(),
            all_day: true,
            notes: "Bring old passport\nsecond line".into(),
            tags: vec!["errands".into()],
            ..Reminder::default()
        };
        let lines = reminder_lines(&r, now(), 60, false);
        assert_eq!(lines.len(), 2);
        let first = lines[0].to_string();
        assert!(first.starts_with("◯  !!! Renew passport"));
        assert!(first.ends_with("⚑ Aug 9"));
        assert_eq!(line_width(&lines[0]), 60);
        assert_eq!(lines[1].to_string(), "   Bring old passport · #errands");
    }

    #[test]
    fn completed_row_and_pending_glyph() {
        let r = Reminder {
            id: 1,
            title: "Done thing".into(),
            completed: true,
            recurring: true,
            subtask_count: 2,
            ..Reminder::default()
        };
        let lines = reminder_lines(&r, now(), 40, false);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].to_string().starts_with("⬤  Done thing ↻ ⤷2"));
        let pending = reminder_lines(&r, now(), 40, true);
        assert!(pending[0].to_string().starts_with("◌"));
    }

    #[test]
    fn fit_and_logo() {
        assert_eq!(fit("abcdef", 4), "abc…");
        assert_eq!(fit("ab", 4), "ab");
        assert_eq!(logo().len(), 3);
        assert_eq!(logo()[0].to_string(), "╦═╗ ┌─┐ ┌┬┐ ╔╦╗ ╦ ╦ ╦");
    }
}
