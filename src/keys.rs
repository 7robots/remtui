//! The binding table: ids, default keys, the vim profile, and `[keys]`
//! overrides. Keys are written in Textual's names (`ctrl+d`, `question_mark`,
//! `G`) so the shared config file means the same thing to both remtuis.

use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Everything a key can do on the main screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Add,
    Edit,
    ToggleDone,
    Delete,
    ToggleFlag,
    CyclePriority,
    Filter,
    ToggleCompleted,
    Refresh,
    DismissFilter,
    Down,
    Up,
    FocusNav,
    FocusReminders,
    SwitchPane,
    Top,
    Bottom,
    BearTriage,
    Help,
    Quit,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    Palette,
    VimNew,
}

/// One row of the table: id, default keys, action, footer label, shown.
pub struct Binding {
    pub id: &'static str,
    pub keys: &'static str,
    pub action: Action,
    pub label: &'static str,
    pub shown: bool,
    pub vim_only: bool,
}

pub const BINDINGS: &[Binding] = &[
    Binding {
        id: "reminder.add",
        keys: "a,n",
        action: Action::Add,
        label: "Add",
        shown: true,
        vim_only: false,
    },
    Binding {
        id: "reminder.edit",
        keys: "e",
        action: Action::Edit,
        label: "Edit",
        shown: true,
        vim_only: false,
    },
    Binding {
        id: "reminder.done",
        keys: "space",
        action: Action::ToggleDone,
        label: "Done",
        shown: true,
        vim_only: false,
    },
    Binding {
        id: "reminder.delete",
        keys: "d,delete,backspace",
        action: Action::Delete,
        label: "Delete",
        shown: true,
        vim_only: false,
    },
    Binding {
        id: "reminder.flag",
        keys: "f",
        action: Action::ToggleFlag,
        label: "Flag",
        shown: true,
        vim_only: false,
    },
    Binding {
        id: "reminder.priority",
        keys: "p",
        action: Action::CyclePriority,
        label: "Priority",
        shown: false,
        vim_only: false,
    },
    Binding {
        id: "view.filter",
        keys: "slash",
        action: Action::Filter,
        label: "Filter",
        shown: true,
        vim_only: false,
    },
    Binding {
        id: "view.show-completed",
        keys: "c",
        action: Action::ToggleCompleted,
        label: "Show done",
        shown: false,
        vim_only: false,
    },
    Binding {
        id: "view.refresh",
        keys: "r",
        action: Action::Refresh,
        label: "Refresh",
        shown: false,
        vim_only: false,
    },
    Binding {
        id: "view.dismiss-filter",
        keys: "escape",
        action: Action::DismissFilter,
        label: "",
        shown: false,
        vim_only: false,
    },
    Binding {
        id: "nav.down",
        keys: "j",
        action: Action::Down,
        label: "",
        shown: false,
        vim_only: false,
    },
    Binding {
        id: "nav.up",
        keys: "k",
        action: Action::Up,
        label: "",
        shown: false,
        vim_only: false,
    },
    Binding {
        id: "nav.left",
        keys: "left,h",
        action: Action::FocusNav,
        label: "Lists",
        shown: false,
        vim_only: false,
    },
    Binding {
        id: "nav.right",
        keys: "right,l",
        action: Action::FocusReminders,
        label: "Reminders",
        shown: false,
        vim_only: false,
    },
    Binding {
        id: "nav.switch-pane",
        keys: "tab",
        action: Action::SwitchPane,
        label: "Switch pane",
        shown: true,
        vim_only: false,
    },
    Binding {
        id: "nav.top",
        keys: "g",
        action: Action::Top,
        label: "",
        shown: false,
        vim_only: false,
    },
    Binding {
        id: "nav.bottom",
        keys: "G",
        action: Action::Bottom,
        label: "",
        shown: false,
        vim_only: false,
    },
    Binding {
        id: "bear.triage",
        keys: "b",
        action: Action::BearTriage,
        label: "Bear",
        shown: true,
        vim_only: false,
    },
    Binding {
        id: "app.help",
        keys: "question_mark",
        action: Action::Help,
        label: "Help",
        shown: true,
        vim_only: false,
    },
    Binding {
        id: "app.quit",
        keys: "q",
        action: Action::Quit,
        label: "Quit",
        shown: true,
        vim_only: false,
    },
    Binding {
        id: "vim.half-down",
        keys: "ctrl+d",
        action: Action::HalfPageDown,
        label: "½ page down",
        shown: false,
        vim_only: true,
    },
    Binding {
        id: "vim.half-up",
        keys: "ctrl+u",
        action: Action::HalfPageUp,
        label: "½ page up",
        shown: false,
        vim_only: true,
    },
    Binding {
        id: "vim.page-down",
        keys: "ctrl+f",
        action: Action::PageDown,
        label: "Page down",
        shown: false,
        vim_only: true,
    },
    Binding {
        id: "vim.page-up",
        keys: "ctrl+b",
        action: Action::PageUp,
        label: "Page up",
        shown: false,
        vim_only: true,
    },
    Binding {
        id: "vim.palette",
        keys: "colon",
        action: Action::Palette,
        label: "Palette",
        shown: false,
        vim_only: true,
    },
    Binding {
        id: "vim.new",
        keys: "o",
        action: Action::VimNew,
        label: "New",
        shown: false,
        vim_only: true,
    },
];

/// A key as the table names it, normalized for matching.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

/// Parse one Textual key name (`a`, `G`, `ctrl+d`, `question_mark`, `shift+tab`).
pub fn parse_key(name: &str) -> Option<Key> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut modifiers = KeyModifiers::NONE;
    let mut parts: Vec<&str> = name.split('+').collect();
    if parts.len() > 1 && name.ends_with('+') {
        // "ctrl++" is the plus key
        parts = vec!["ctrl", "plus"];
    }
    let last = parts.pop()?;
    for part in parts {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            "alt" | "meta" | "option" => modifiers |= KeyModifiers::ALT,
            _ => return None,
        }
    }
    let code = match last {
        "space" => KeyCode::Char(' '),
        "enter" | "return" => KeyCode::Enter,
        "escape" | "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "slash" => KeyCode::Char('/'),
        "backslash" => KeyCode::Char('\\'),
        "colon" => KeyCode::Char(':'),
        "semicolon" => KeyCode::Char(';'),
        "comma" => KeyCode::Char(','),
        "full_stop" | "period" => KeyCode::Char('.'),
        "minus" => KeyCode::Char('-'),
        "plus" => KeyCode::Char('+'),
        "equals_sign" | "equals" => KeyCode::Char('='),
        "question_mark" => KeyCode::Char('?'),
        "exclamation_mark" => KeyCode::Char('!'),
        "asterisk" => KeyCode::Char('*'),
        "number_sign" => KeyCode::Char('#'),
        "dollar_sign" => KeyCode::Char('$'),
        "percent_sign" => KeyCode::Char('%'),
        "ampersand" => KeyCode::Char('&'),
        "at" => KeyCode::Char('@'),
        "circumflex_accent" => KeyCode::Char('^'),
        "underscore" => KeyCode::Char('_'),
        "grave_accent" => KeyCode::Char('`'),
        "tilde" => KeyCode::Char('~'),
        "apostrophe" => KeyCode::Char('\''),
        "quotation_mark" => KeyCode::Char('"'),
        "left_square_bracket" => KeyCode::Char('['),
        "right_square_bracket" => KeyCode::Char(']'),
        "left_curly_bracket" => KeyCode::Char('{'),
        "right_curly_bracket" => KeyCode::Char('}'),
        "vertical_line" => KeyCode::Char('|'),
        "less_than_sign" => KeyCode::Char('<'),
        "greater_than_sign" => KeyCode::Char('>'),
        other => {
            let mut chars = other.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                KeyCode::F(other.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())?)
            } else {
                KeyCode::Char(c)
            }
        }
    };
    Some(Key { code, modifiers })
}

fn normalize(event: &KeyEvent) -> Key {
    // Shift is part of the character for printable keys; ignore it there so
    // `G` matches whether the terminal reports SHIFT or not.
    let mut modifiers =
        event.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
    if matches!(event.code, KeyCode::Char(_)) {
        modifiers.remove(KeyModifiers::SHIFT);
    }
    if event.code == KeyCode::BackTab {
        return Key {
            code: KeyCode::Tab,
            modifiers: modifiers | KeyModifiers::SHIFT,
        };
    }
    Key {
        code: event.code,
        modifiers,
    }
}

/// The resolved table: which keys fire which action.
#[derive(Debug, Clone)]
pub struct Keymap {
    pub vim: bool,
    entries: Vec<(Key, Action)>,
    keys_by_action: BTreeMap<&'static str, Vec<String>>,
}

impl Keymap {
    pub fn new(vim: bool, overrides: &BTreeMap<String, String>) -> Keymap {
        let mut entries = Vec::new();
        let mut keys_by_action = BTreeMap::new();
        for binding in BINDINGS {
            let spec = overrides
                .get(binding.id)
                .map(String::as_str)
                .unwrap_or(binding.keys);
            let names: Vec<String> = spec
                .split(',')
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .collect();
            for name in &names {
                if let Some(key) = parse_key(name) {
                    entries.push((key, binding.action));
                }
            }
            keys_by_action.insert(binding.id, names);
        }
        Keymap {
            vim,
            entries,
            keys_by_action,
        }
    }

    /// The action for a key press, if the table has one. Vim-only actions
    /// answer only in the vim profile; the Bear action is gated by the caller.
    pub fn action(&self, event: &KeyEvent) -> Option<Action> {
        let key = normalize(event);
        self.entries
            .iter()
            .filter(|(k, _)| *k == key)
            .map(|(_, a)| *a)
            .find(|action| self.vim || !is_vim_action(*action))
    }

    /// The key names bound to a binding id, as configured.
    pub fn keys_for(&self, id: &str) -> Vec<String> {
        self.keys_by_action.get(id).cloned().unwrap_or_default()
    }

    /// A short display form of the first key for a binding (`?`, `space`, `^d`).
    pub fn display_key(&self, id: &str) -> String {
        self.keys_for(id)
            .first()
            .map(|k| display_name(k))
            .unwrap_or_default()
    }
}

pub fn is_vim_action(action: Action) -> bool {
    matches!(
        action,
        Action::HalfPageDown
            | Action::HalfPageUp
            | Action::PageDown
            | Action::PageUp
            | Action::Palette
            | Action::VimNew
    )
}

/// How the footer prints a Textual key name.
pub fn display_name(name: &str) -> String {
    let name = name.trim();
    if let Some(rest) = name.strip_prefix("ctrl+") {
        return format!("^{}", display_name(rest));
    }
    match name {
        "question_mark" => "?".into(),
        "slash" => "/".into(),
        "colon" => ":".into(),
        "escape" => "esc".into(),
        "backspace" => "⌫".into(),
        "delete" => "del".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "space" => "space".into(),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn parses_textual_names() {
        assert_eq!(parse_key("a").unwrap().code, KeyCode::Char('a'));
        assert_eq!(parse_key("question_mark").unwrap().code, KeyCode::Char('?'));
        let ctrl_d = parse_key("ctrl+d").unwrap();
        assert_eq!(
            (ctrl_d.code, ctrl_d.modifiers),
            (KeyCode::Char('d'), KeyModifiers::CONTROL)
        );
        assert_eq!(parse_key("space").unwrap().code, KeyCode::Char(' '));
        assert!(parse_key("nonsense_key").is_none());
        assert_eq!(parse_key("f5").unwrap().code, KeyCode::F(5));
    }

    #[test]
    fn default_table_matches_events() {
        let km = Keymap::new(false, &BTreeMap::new());
        assert_eq!(
            km.action(&ev(KeyCode::Char('a'), KeyModifiers::NONE)),
            Some(Action::Add)
        );
        assert_eq!(
            km.action(&ev(KeyCode::Char('n'), KeyModifiers::NONE)),
            Some(Action::Add)
        );
        assert_eq!(
            km.action(&ev(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Some(Action::Bottom)
        );
        assert_eq!(
            km.action(&ev(KeyCode::Char('g'), KeyModifiers::NONE)),
            Some(Action::Top)
        );
        assert_eq!(
            km.action(&ev(KeyCode::Char('?'), KeyModifiers::SHIFT)),
            Some(Action::Help)
        );
        assert_eq!(
            km.action(&ev(KeyCode::Backspace, KeyModifiers::NONE)),
            Some(Action::Delete)
        );
        assert_eq!(
            km.action(&ev(KeyCode::Tab, KeyModifiers::NONE)),
            Some(Action::SwitchPane)
        );
        // vim-only keys are silent in the default profile
        assert_eq!(
            km.action(&ev(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            None
        );
        assert_eq!(km.action(&ev(KeyCode::Char('o'), KeyModifiers::NONE)), None);
        let vim = Keymap::new(true, &BTreeMap::new());
        assert_eq!(
            vim.action(&ev(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(Action::HalfPageDown)
        );
        assert_eq!(
            vim.action(&ev(KeyCode::Char(':'), KeyModifiers::SHIFT)),
            Some(Action::Palette)
        );
        assert_eq!(
            vim.action(&ev(KeyCode::Char('o'), KeyModifiers::NONE)),
            Some(Action::VimNew)
        );
    }

    #[test]
    fn overrides_replace_keys_entirely() {
        let mut o = BTreeMap::new();
        o.insert("reminder.add".to_string(), "x".to_string());
        o.insert("app.quit".to_string(), "ctrl+q, Q".to_string());
        let km = Keymap::new(false, &o);
        assert_eq!(
            km.action(&ev(KeyCode::Char('x'), KeyModifiers::NONE)),
            Some(Action::Add)
        );
        assert_eq!(km.action(&ev(KeyCode::Char('a'), KeyModifiers::NONE)), None);
        assert_eq!(km.action(&ev(KeyCode::Char('q'), KeyModifiers::NONE)), None);
        assert_eq!(
            km.action(&ev(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            Some(Action::Quit)
        );
        assert_eq!(
            km.action(&ev(KeyCode::Char('Q'), KeyModifiers::SHIFT)),
            Some(Action::Quit)
        );
        assert_eq!(km.keys_for("app.quit"), vec!["ctrl+q", "Q"]);
        assert_eq!(km.display_key("app.quit"), "^q");
        assert_eq!(display_name("question_mark"), "?");
    }
}
