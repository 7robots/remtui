//! Drawing: the header, sidebar, view header, reminder list, footer, toasts
//! and overlays, all from `App` state. The draw pass records where panes
//! landed for mouse hit testing and clamps scrolling to the viewport.

pub mod field;
pub mod form;
pub mod help;
pub mod rows;
pub mod theme;
pub mod triage;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, NavItem, Overlay, Pane, Rects, Severity};
use crate::keys::{Action, BINDINGS};
use crate::models::SmartView;
use field::Field;
use form::{BUTTONS, Focus, Form};
use triage::{Triage, TriageLine};

pub const SIDEBAR_WIDTH: u16 = 30;
/// The logo shows only when the terminal is at least this tall.
pub const LOGO_MIN_HEIGHT: u16 = 20;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);
    let mut rects = Rects::default();

    draw_header(frame, header);
    let [sidebar, main] =
        Layout::horizontal([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(20)]).areas(body);
    draw_sidebar(frame, app, sidebar, &mut rects);
    draw_main(frame, app, main, &mut rects);
    draw_footer(frame, app, footer);
    app.rects = rects;

    draw_toasts(frame, app, body);
    if let Some(overlay) = app.overlay.clone() {
        draw_overlay(frame, app, area, &overlay);
    }
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let clock = chrono::Local::now().format("%H:%M:%S").to_string();
    let title = " remtui";
    let sub = " — Apple Reminders";
    let pad = (area.width as usize).saturating_sub(title.len() + sub.len() + clock.len() + 1);
    let line = Line::from(vec![
        Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(sub, theme::muted()),
        Span::raw(" ".repeat(pad)),
        Span::styled(clock, theme::muted()),
        Span::raw(" "),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme::PANEL)),
        area,
    );
}

fn draw_sidebar(frame: &mut Frame, app: &mut App, area: Rect, rects: &mut Rects) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(theme::border());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.show_logo = frame.area().height >= LOGO_MIN_HEIGHT;
    let logo_rows = if app.show_logo { 4 } else { 0 };
    let [_, logo, nav] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(logo_rows),
        Constraint::Min(1),
    ])
    .areas(inner);
    if app.show_logo {
        let lines = rows::logo();
        let width = rows::line_width(&lines[0]);
        let pad = (logo.width as usize).saturating_sub(width) / 2;
        let centered: Vec<Line> = lines
            .into_iter()
            .map(|mut l| {
                l.spans.insert(0, Span::raw(" ".repeat(pad)));
                l
            })
            .collect();
        frame.render_widget(Paragraph::new(centered), logo);
    }
    let nav = Rect {
        x: nav.x + 1,
        width: nav.width.saturating_sub(2),
        ..nav
    };
    rects.nav_rows = nav;
    frame.render_widget(Clear, nav);
    // keep the highlight on screen
    let height = nav.height as usize;
    if app.nav_cursor < app.nav_scroll {
        app.nav_scroll = app.nav_cursor;
    }
    if height > 0 && app.nav_cursor >= app.nav_scroll + height {
        app.nav_scroll = app.nav_cursor + 1 - height;
    }
    let focused = app.focus == Pane::Nav;
    let width = nav.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (index, item) in app.nav.iter().enumerate().skip(app.nav_scroll).take(height) {
        let mut line = match item {
            NavItem::Header(label) => rows::nav_header(label),
            NavItem::Blank => Line::default(),
            NavItem::Smart(view) => rows::smart_row(*view, width),
            NavItem::List(i) => rows::list_row(&app.lists[*i], width),
        };
        if index == app.nav_cursor && item.selectable() {
            let used = rows::line_width(&line);
            line.spans
                .push(Span::raw(" ".repeat(width.saturating_sub(used))));
            line = line.style(theme::cursor(focused));
        }
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), nav);
}

fn draw_main(frame: &mut Frame, app: &mut App, area: Rect, rects: &mut Rects) {
    let filter_rows = if app.filter.is_some() { 3 } else { 0 };
    let bar_rows = if app
        .current_list()
        .is_some_and(|l| l.active + l.completed > 0)
    {
        1
    } else {
        0
    };
    let [_, title, stats, bar, _, filter, list] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(bar_rows),
        Constraint::Length(1),
        Constraint::Length(filter_rows),
        Constraint::Min(1),
    ])
    .areas(area);
    let indent = |r: Rect| Rect {
        x: r.x + 2,
        width: r.width.saturating_sub(4),
        ..r
    };
    draw_view_header(frame, app, indent(title), indent(stats), indent(bar));
    if let Some(field) = &app.filter {
        let focused = app.focus == Pane::Filter;
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if focused {
                theme::PRIMARY
            } else {
                theme::BORDER
            }));
        let inner = block.inner(indent(filter));
        frame.render_widget(block, indent(filter));
        rects.filter = indent(filter);
        draw_field(frame, field, inner, focused, "filter…");
    }
    draw_list(frame, app, indent(list), rects);
}

fn draw_view_header(frame: &mut Frame, app: &App, title: Rect, stats: Rect, bar: Rect) {
    let (icon, label, color) = match app.view {
        crate::app::ViewKind::Smart(v) => (
            v.icon().to_string(),
            v.label().to_string(),
            theme::hex(v.color_hex()),
        ),
        crate::app::ViewKind::List(_) => match app.current_list() {
            Some(l) => (
                if l.emoji.is_empty() {
                    "●".to_string()
                } else {
                    l.emoji.clone()
                },
                l.title.clone(),
                theme::hex(&l.color_hex),
            ),
            None => ("●".to_string(), String::new(), theme::PRIMARY),
        },
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{icon} "), theme::fg(color)),
            Span::styled(
                label,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ])),
        title,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(app.stats_line(), theme::muted()))),
        stats,
    );
    if bar.height > 0
        && let Some(list) = app.current_list()
    {
        let total = (list.active + list.completed).max(1) as f64;
        let ratio = list.completed as f64 / total;
        let width = 26usize.min(bar.width.saturating_sub(6) as usize);
        let filled = (ratio * width as f64).round() as usize;
        let line = Line::from(vec![
            Span::styled("━".repeat(filled), theme::fg(theme::SUCCESS)),
            Span::styled("━".repeat(width - filled), theme::fg(theme::PANEL)),
            Span::styled(
                format!(" {:>3}%", (ratio * 100.0).round() as i64),
                theme::muted(),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), bar);
    }
}

impl App {
    /// The counts line under the view title.
    pub fn stats_line(&self) -> String {
        let shown = self.shown.len();
        let mut parts: Vec<String> = Vec::new();
        match self.current_list() {
            Some(list) => {
                parts.push(format!("{} active", list.active));
                if list.completed > 0 {
                    parts.push(format!("{} done", list.completed));
                }
            }
            None => parts.push(format!(
                "{shown} reminder{}",
                if shown == 1 { "" } else { "s" }
            )),
        }
        if !self.filter_text.is_empty() {
            parts.push(format!(
                "filter \"{}\" → {shown} match{}",
                self.filter_text,
                if shown == 1 { "" } else { "es" }
            ));
        }
        if self.loading {
            parts.push("loading…".into());
        }
        parts.join(" · ")
    }

    /// The message shown when the list is empty.
    pub fn empty_message(&self) -> String {
        if !self.filter_text.is_empty() {
            return format!("○  nothing matches \"{}\"", self.filter_text);
        }
        match self.view {
            crate::app::ViewKind::List(_) => "○  no reminders here — press a to add one".into(),
            crate::app::ViewKind::Smart(v) => format!("✓  {}", v.empty_message()),
        }
    }
}

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect, rects: &mut Rects) {
    rects.list_rows = area;
    frame.render_widget(Clear, area);
    let now = crate::dates::now();
    let width = area.width as usize;
    if app.shown.is_empty() {
        if app.view_loaded && !app.loading {
            let message = app.empty_message();
            let y = area.y + area.height / 3;
            let x = area.x
                + (area.width as usize).saturating_sub(UnicodeWidthStr::width(message.as_str()))
                    as u16
                    / 2;
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    message,
                    Style::default()
                        .fg(theme::MUTED)
                        .add_modifier(Modifier::ITALIC),
                ))),
                Rect {
                    x,
                    y,
                    width: area.width.saturating_sub(x - area.x),
                    height: 1,
                },
            );
        }
        app.ensure_visible(&[], area.height);
        return;
    }
    let rendered: Vec<Vec<Line<'static>>> = app
        .shown
        .iter()
        .map(|&i| {
            let r = &app.reminders[i];
            rows::reminder_lines(r, now, width, app.is_pending(r.id))
        })
        .collect();
    let heights: Vec<u16> = rendered.iter().map(|ls| ls.len() as u16).collect();
    app.ensure_visible(&heights, area.height);
    let focused = app.focus == Pane::Reminders;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut y = area.y;
    rects.row_spans.clear();
    for (index, block) in rendered.into_iter().enumerate().skip(app.scroll) {
        let h = block.len() as u16;
        if y + h > area.y + area.height {
            break;
        }
        rects.row_spans.push((y, h));
        y += h;
        let is_cursor = index == app.cursor;
        for mut line in block {
            if is_cursor {
                let used = rows::line_width(&line);
                line.spans
                    .push(Span::raw(" ".repeat(width.saturating_sub(used))));
                line = line.style(theme::cursor(focused));
            }
            lines.push(line);
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Draw a text field's value with the cursor placed when focused.
pub fn draw_field(frame: &mut Frame, field: &Field, area: Rect, focused: bool, placeholder: &str) {
    let text = if field.value.is_empty() && !focused {
        Span::styled(placeholder.to_string(), theme::muted())
    } else if field.value.is_empty() {
        Span::styled(placeholder.to_string(), theme::dim())
    } else {
        Span::raw(field.value.clone())
    };
    frame.render_widget(Paragraph::new(Line::from(vec![Span::raw(" "), text])), area);
    if focused {
        let offset: u16 = field
            .value
            .chars()
            .take(field.cursor)
            .map(|c| UnicodeWidthStr::width(c.to_string().as_str()) as u16)
            .sum();
        frame.set_cursor_position((
            (area.x + 1 + offset).min(area.x + area.width.saturating_sub(1)),
            area.y,
        ));
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let overlay_hints: Option<Vec<(&str, &str)>> = match &app.overlay {
        Some(Overlay::Form(_)) => Some(vec![("esc", "Cancel"), ("^s", "Save"), ("^e", "Editor")]),
        Some(Overlay::Confirm { confirm_label, .. }) => {
            Some(vec![("esc", "Cancel"), ("y", confirm_label.as_str())])
        }
        Some(Overlay::Triage(_)) => Some(vec![
            ("esc", "Close"),
            ("space", "Mark"),
            ("enter", "Add marked"),
            ("o", "Open in Bear"),
            ("x", "Tick in Bear"),
            ("/", "Filter"),
        ]),
        Some(Overlay::Help { .. }) => Some(vec![("esc", "Close")]),
        None => None,
    };
    if let Some(hints) = overlay_hints {
        for (key, label) in hints {
            spans.push(Span::styled(
                format!(" {key} "),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(format!("{label} "), theme::muted()));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::PANEL)),
            area,
        );
        return;
    }
    let selection_actions = [
        Action::Edit,
        Action::ToggleDone,
        Action::Delete,
        Action::ToggleFlag,
        Action::CyclePriority,
    ];
    for binding in BINDINGS.iter().filter(|b| b.shown) {
        if binding.action == Action::BearTriage && !app.bear_enabled() {
            continue;
        }
        let key = app.keymap.display_key(binding.id);
        if key.is_empty() {
            continue;
        }
        let grayed = selection_actions.contains(&binding.action) && !app.has_selection();
        let key_style = if grayed {
            Style::default()
                .fg(theme::MUTED)
                .add_modifier(Modifier::DIM)
        } else {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        };
        let label_style = if grayed { theme::dim() } else { theme::muted() };
        spans.push(Span::styled(format!(" {key} "), key_style));
        spans.push(Span::styled(format!("{} ", binding.label), label_style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::PANEL)),
        area,
    );
}

fn draw_toasts(frame: &mut Frame, app: &App, area: Rect) {
    let width = 50u16.min(area.width.saturating_sub(4));
    let mut bottom = area.y + area.height;
    for toast in app.toasts.iter().rev() {
        let color = match toast.severity {
            Severity::Information => theme::PRIMARY,
            Severity::Warning => theme::WARNING,
            Severity::Error => theme::ERROR,
        };
        let mut lines: Vec<Line<'static>> = Vec::new();
        if !toast.title.is_empty() {
            lines.push(Line::from(Span::styled(
                toast.title.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            )));
        }
        lines.push(Line::from(toast.message.clone()));
        let text_width = width.saturating_sub(4).max(1) as usize;
        let text_rows: usize = lines
            .iter()
            .map(|l| {
                UnicodeWidthStr::width(l.to_string().as_str())
                    .div_ceil(text_width)
                    .max(1)
            })
            .sum();
        let height = (text_rows + 2) as u16;
        if bottom < area.y + height {
            break;
        }
        let rect = Rect {
            x: area.x + area.width - width - 1,
            y: bottom - height,
            width,
            height,
        };
        bottom = rect.y;
        frame.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        frame.render_widget(
            Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }),
            inner,
        );
    }
}

/// A centered rect of at most `w` × `h` inside `area`.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Clear and frame a dialog; returns the inner rect.
pub fn dialog(
    frame: &mut Frame,
    area: Rect,
    w: u16,
    h: u16,
    title: &str,
    color: ratatui::style::Color,
) -> Rect {
    let rect = centered(area, w, h);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    Rect {
        x: inner.x + 2,
        width: inner.width.saturating_sub(4),
        ..inner
    }
}

fn draw_overlay(frame: &mut Frame, app: &mut App, area: Rect, overlay: &Overlay) {
    match overlay {
        Overlay::Form(form) => draw_form(frame, area, form),
        Overlay::Triage(triage) => {
            let mut triage = (**triage).clone();
            draw_triage(frame, area, &mut triage);
            app.overlay = Some(Overlay::Triage(Box::new(triage)));
        }
        Overlay::Confirm {
            title,
            message,
            confirm_label,
            danger,
            focus_confirm,
            ..
        } => {
            let color = if *danger {
                theme::ERROR
            } else {
                theme::SUCCESS
            };
            let lines: Vec<&str> = message.lines().collect();
            let inner = dialog(frame, area, 58, lines.len() as u16 + 5, title, color);
            let [text, _, buttons] = Layout::vertical([
                Constraint::Length(lines.len() as u16),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            frame.render_widget(
                Paragraph::new(
                    lines
                        .iter()
                        .map(|l| Line::from(l.to_string()))
                        .collect::<Vec<_>>(),
                ),
                text,
            );
            draw_buttons(
                frame,
                buttons,
                &["Cancel", confirm_label],
                if *focus_confirm { 1 } else { 0 },
                Some(color),
            );
        }
        Overlay::Help { scroll } => {
            let body = help::lines(app.bear_enabled());
            let height = (body.len() as u16 + 4).min(area.height.saturating_sub(2));
            let inner = dialog(
                frame,
                area,
                60,
                height,
                "⌨  Keyboard Reference",
                theme::ACCENT,
            );
            let rows = inner.height.saturating_sub(2) as usize;
            let max_scroll = body.len().saturating_sub(rows);
            let wanted = *scroll;
            let scroll = wanted.min(max_scroll);
            if scroll != wanted {
                app.overlay = Some(Overlay::Help { scroll });
            }
            let shown: Vec<Line> = body.into_iter().skip(scroll).take(rows).collect();
            let [text, _, foot] = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            frame.render_widget(Paragraph::new(shown), text);
            let hint = "esc to close";
            let x = foot.x + (foot.width as usize).saturating_sub(hint.len()) as u16 / 2;
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(hint, theme::dim()))),
                Rect {
                    x,
                    width: foot.width.saturating_sub(x - foot.x),
                    ..foot
                },
            );
        }
    }
}

impl SmartView {
    /// The sidebar row index of a smart view (after the header).
    pub fn nav_index(self) -> usize {
        1 + SmartView::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
}

/// Right-aligned buttons; the focused one is reversed, the last is the
/// primary and takes `primary` as its color.
fn draw_buttons(
    frame: &mut Frame,
    area: Rect,
    labels: &[&str],
    focused: usize,
    primary: Option<ratatui::style::Color>,
) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut width = 0;
    for (index, label) in labels.iter().enumerate() {
        let text = format!("[ {label:^8} ]");
        width += UnicodeWidthStr::width(text.as_str()) + 2;
        let mut style = Style::default();
        if index == labels.len() - 1
            && let Some(color) = primary
        {
            style = style.fg(color).add_modifier(Modifier::BOLD);
        }
        if index == focused {
            style = style.add_modifier(Modifier::REVERSED);
        }
        spans.push(Span::raw("  "));
        spans.push(Span::styled(text, style));
    }
    let pad = (area.width as usize).saturating_sub(width);
    spans.insert(0, Span::raw(" ".repeat(pad)));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn labeled_box(frame: &mut Frame, area: Rect, title: &str, focused: bool) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused {
            theme::PRIMARY
        } else {
            theme::BORDER
        }))
        .title(Span::styled(format!(" {title} "), theme::muted()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn draw_form(frame: &mut Frame, area: Rect, form: &Form) {
    let height = 3 + 6 + 3 + 3 + 1 + 1 + 2;
    let inner = dialog(frame, area, 62, height, form.heading(), theme::PRIMARY);
    let [title, notes, row1, row2, error, _, buttons] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(6),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    let box_inner = labeled_box(frame, title, "Title", form.focus == Focus::Title);
    draw_field(
        frame,
        &form.title,
        box_inner,
        form.focus == Focus::Title,
        form::TITLE_PLACEHOLDER,
    );

    let notes_inner = labeled_box(frame, notes, "Notes", form.focus == Focus::Notes);
    let lines = form.notes.lines();
    let (cursor_line, cursor_col) = form.notes.position();
    let rows = notes_inner.height as usize;
    let scroll = cursor_line.saturating_sub(rows.saturating_sub(1));
    let shown: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(rows)
        .map(|l| Line::from(format!(" {l}")))
        .collect();
    frame.render_widget(Paragraph::new(shown), notes_inner);
    if form.focus == Focus::Notes {
        let col: u16 = lines
            .get(cursor_line)
            .map(|l| {
                l.chars()
                    .take(cursor_col)
                    .map(|c| UnicodeWidthStr::width(c.to_string().as_str()) as u16)
                    .sum()
            })
            .unwrap_or(0);
        frame.set_cursor_position((
            (notes_inner.x + 1 + col).min(notes_inner.x + notes_inner.width.saturating_sub(1)),
            notes_inner.y + (cursor_line - scroll) as u16,
        ));
    }

    let [due, priority] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(row1);
    let due_inner = labeled_box(frame, due, "Due", form.focus == Focus::Due);
    draw_field(
        frame,
        &form.due,
        due_inner,
        form.focus == Focus::Due,
        form::DUE_PLACEHOLDER,
    );
    let priority_inner = labeled_box(frame, priority, "Priority", form.focus == Focus::Priority);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::raw(form.priority.label().to_string()),
            Span::styled(" ▾", theme::muted()),
        ])),
        priority_inner,
    );

    let [list, flag] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(row2);
    let list_inner = labeled_box(frame, list, "List", form.focus == Focus::List);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::raw(form.list_title()),
            Span::styled(" ▾", theme::muted()),
        ])),
        list_inner,
    );
    let flag_inner = labeled_box(frame, flag, "", form.focus == Focus::Flag);
    let mark = if form.flagged { "▣" } else { "▢" };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(format!(" {mark} ")),
            Span::raw("Flagged "),
            Span::styled("⚑", theme::fg(theme::FLAG)),
        ])),
        flag_inner,
    );

    if !form.error.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                form.error.clone(),
                theme::fg(theme::ERROR),
            ))),
            error,
        );
    } else if form.saving {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("saving…", theme::muted()))),
            error,
        );
    }
    let labels = [BUTTONS[0], BUTTONS[1], form.save_label()];
    let focused = if form.focus == Focus::Buttons {
        form.button
    } else {
        usize::MAX
    };
    draw_buttons(frame, buttons, &labels, focused, Some(theme::PRIMARY));
}

fn draw_triage(frame: &mut Frame, area: Rect, triage: &mut Triage) {
    let height = (area.height as u32 * 9 / 10) as u16;
    let inner = dialog(
        frame,
        area,
        100,
        height.max(10),
        "🐻 Bear Todos",
        theme::SECONDARY,
    );
    let filter_rows = if triage.filter.is_some() { 3 } else { 0 };
    let [stats, _, filter, list, error, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(filter_rows),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(triage.stats(), theme::muted()))),
        stats,
    );
    if let Some(field) = &triage.filter {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if triage.filter_focused {
                theme::PRIMARY
            } else {
                theme::BORDER
            }));
        let field_inner = block.inner(filter);
        frame.render_widget(block, filter);
        draw_field(
            frame,
            field,
            field_inner,
            triage.filter_focused,
            "filter todos…",
        );
    }
    let width = list.width as usize;
    let now = crate::dates::now();
    if triage.lines.is_empty() {
        if triage.loaded && !triage.loading {
            let message = triage.empty_message();
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    message,
                    Style::default()
                        .fg(theme::MUTED)
                        .add_modifier(Modifier::ITALIC),
                ))),
                Rect { height: 1, ..list },
            );
        } else if triage.loading {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled("loading…", theme::muted()))),
                Rect { height: 1, ..list },
            );
        }
    } else {
        let rows = list.height as usize;
        if triage.cursor < triage.scroll {
            triage.scroll = triage.cursor;
        }
        if rows > 0 && triage.cursor >= triage.scroll + rows {
            triage.scroll = triage.cursor + 1 - rows;
        }
        let mut rendered: Vec<Line<'static>> = Vec::new();
        for (index, line) in triage
            .lines
            .iter()
            .enumerate()
            .skip(triage.scroll)
            .take(rows)
        {
            let mut row = match line {
                TriageLine::Blank => Line::default(),
                TriageLine::Header { title, tags } => Triage::render_header(title, tags, width),
                TriageLine::Todo(i) => triage.render_row(&triage.rows[*i], width, now),
            };
            if index == triage.cursor && matches!(line, TriageLine::Todo(_)) {
                let used = rows::line_width(&row);
                row.spans
                    .push(Span::raw(" ".repeat(width.saturating_sub(used))));
                row = row.style(theme::cursor(!triage.filter_focused));
            }
            rendered.push(row);
        }
        frame.render_widget(Clear, list);
        frame.render_widget(Paragraph::new(rendered), list);
    }
    if !triage.error.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                triage.error.clone(),
                theme::fg(theme::ERROR),
            ))),
            error,
        );
    } else if triage.adding {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("adding…", theme::muted()))),
            error,
        );
    }
    draw_buttons(
        frame,
        buttons,
        &["Close", "Add marked"],
        usize::MAX,
        Some(theme::PRIMARY),
    );
}
