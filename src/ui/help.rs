//! The keyboard reference, as the Python remtui prints it.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme;

/// (heading, qualifier, rows)
type Section = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const SECTIONS: &[Section] = &[
    (
        "Navigate",
        "",
        &[
            ("j / ↓, k / ↑", "move down / up"),
            ("← / h, → / l", "focus sidebar / reminders"),
            ("tab", "switch pane"),
            ("g, G", "jump to top / bottom"),
        ],
    ),
    (
        "Reminders",
        "",
        &[
            ("a", "add a reminder"),
            ("e / enter", "edit selected"),
            ("space", "toggle done"),
            ("d / ⌫", "delete (asks first)"),
            ("f", "toggle flag ⚑"),
            ("p", "cycle priority"),
        ],
    ),
    (
        "Views",
        "",
        &[
            ("/", "filter current view"),
            ("esc", "clear filter"),
            ("c", "show/hide completed (list views)"),
            ("r", "refresh"),
        ],
    ),
    (
        "Form (add / edit)",
        "",
        &[
            ("ctrl+s", "save"),
            ("ctrl+e", "edit notes in $EDITOR"),
            ("esc", "cancel"),
        ],
    ),
    (
        "App",
        "",
        &[
            ("ctrl+p", "this help (the Python remtui's command palette)"),
            ("?", "this help"),
            ("q", "quit"),
        ],
    ),
    (
        "Vim profile",
        "(--vim, REMTUI_KEYS=vim, or config)",
        &[
            ("gg / G", "jump to top / bottom"),
            ("ctrl+d / ctrl+u", "half page down / up"),
            ("ctrl+f / ctrl+b", "full page down / up"),
            (":", "this help"),
            ("o", "add a reminder"),
        ],
    ),
];

const BEAR_SECTION: Section = (
    "Bear triage",
    "([bear] enabled = true)",
    &[
        ("b", "review open todos from Bear notes"),
        ("space", "mark / unmark a todo"),
        ("enter / ctrl+s", "add marked todos to Reminders"),
        ("o", "open the note in Bear"),
        ("x", "tick the todo in Bear (done rows only, asks first)"),
        ("/", "filter todos"),
        ("r", "reload from Bear and Reminders"),
    ],
);

pub fn lines(bear: bool) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut sections: Vec<&Section> = SECTIONS.iter().collect();
    if bear {
        sections.push(&BEAR_SECTION);
    }
    for (index, (heading, qualifier, rows)) in sections.into_iter().enumerate() {
        if index > 0 {
            out.push(Line::default());
        }
        let mut spans = vec![Span::styled(
            heading.to_string(),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )];
        if !qualifier.is_empty() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(qualifier.to_string(), theme::dim()));
        }
        out.push(Line::from(spans));
        for (key, what) in rows.iter() {
            out.push(Line::from(vec![
                Span::raw(format!("  {key:<20}")),
                Span::raw(what.to_string()),
            ]));
        }
    }
    out
}

/// Plain text of the reference, for tests.
pub fn text(bear: bool) -> String {
    lines(bear)
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
