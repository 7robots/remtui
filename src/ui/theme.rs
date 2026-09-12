//! The palette: remtui's Apple colors as RGB where the Python used them, the
//! terminal's own foreground and background everywhere else.

use ratatui::style::{Color, Modifier, Style};

pub const PRIMARY: Color = Color::Rgb(0x0A, 0x84, 0xFF);
pub const ACCENT: Color = Color::Rgb(0xFF, 0x9F, 0x0A);
pub const WARNING: Color = Color::Rgb(0xFF, 0xD6, 0x0A);
pub const ERROR: Color = Color::Rgb(0xFF, 0x45, 0x3A);
pub const SUCCESS: Color = Color::Rgb(0x30, 0xD1, 0x58);
pub const SECONDARY: Color = Color::Rgb(0x5E, 0x5C, 0xE6);
pub const TAG: Color = Color::Rgb(0x64, 0xD2, 0xFF);
pub const TODAY: Color = PRIMARY;
pub const OVERDUE: Color = ERROR;
pub const FLAG: Color = ACCENT;
pub const DONE: Color = SUCCESS;
pub const NAV_HEADER: Color = Color::Rgb(0x6E, 0x6E, 0x73);
/// `$panel`: the unfocused cursor bar.
pub const PANEL: Color = Color::Rgb(0x3A, 0x3A, 0x3C);
/// `$primary 35%` over the dark background: the focused cursor bar.
pub const CURSOR: Color = Color::Rgb(0x1B, 0x40, 0x6F);
pub const BORDER: Color = Color::DarkGray;
pub const MUTED: Color = Color::DarkGray;

/// Classic Apple logo rainbow, left to right across the wordmark.
pub const APPLE_RAINBOW: [Color; 6] = [
    Color::Rgb(0x61, 0xBB, 0x46),
    Color::Rgb(0xFD, 0xB8, 0x27),
    Color::Rgb(0xF5, 0x82, 0x1F),
    Color::Rgb(0xE0, 0x3A, 0x3E),
    Color::Rgb(0x96, 0x3D, 0x97),
    Color::Rgb(0x00, 0x9D, 0xDC),
];

/// `#RRGGBB` → a color; anything else falls back to the primary blue.
pub fn hex(text: &str) -> Color {
    let t = text.trim().trim_start_matches('#');
    if t.len() == 6
        && let Ok(v) = u32::from_str_radix(t, 16)
    {
        return Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
    }
    PRIMARY
}

pub fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

pub fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

pub fn fg(color: Color) -> Style {
    Style::default().fg(color)
}

pub fn cursor(focused: bool) -> Style {
    Style::default().bg(if focused { CURSOR } else { PANEL })
}

pub fn border() -> Style {
    Style::default().fg(BORDER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parses_or_falls_back() {
        assert_eq!(hex("#FF3B30"), Color::Rgb(0xFF, 0x3B, 0x30));
        assert_eq!(hex("34C759"), Color::Rgb(0x34, 0xC7, 0x59));
        assert_eq!(hex("nope"), PRIMARY);
    }
}
