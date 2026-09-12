//! A one-line text field with a cursor, shared by the filter box and the form.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Field {
    pub value: String,
    pub cursor: usize,
}

impl Field {
    pub fn new(value: &str) -> Field {
        Field {
            value: value.to_string(),
            cursor: value.chars().count(),
        }
    }

    fn byte_index(&self, chars: usize) -> usize {
        self.value
            .char_indices()
            .nth(chars)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }

    pub fn insert(&mut self, ch: char) {
        let at = self.byte_index(self.cursor);
        self.value.insert(at, ch);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let at = self.byte_index(self.cursor - 1);
            self.value.remove(at);
            self.cursor -= 1;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.value.chars().count() {
            let at = self.byte_index(self.cursor);
            self.value.remove(at);
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.value.chars().count());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.value.chars().count();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    /// Delete from the cursor to the end (`ctrl+k`).
    pub fn kill_to_end(&mut self) {
        let at = self.byte_index(self.cursor);
        self.value.truncate(at);
    }

    /// Delete the word before the cursor (`ctrl+w`, `alt+backspace`).
    pub fn delete_word(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        let mut i = self.cursor;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }
        let rest: String = chars[..i]
            .iter()
            .chain(chars[self.cursor..].iter())
            .collect();
        self.value = rest;
        self.cursor = i;
    }

    /// Apply an editing key; false when the key is not an editing key.
    pub fn handle(&mut self, key: &KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char(c) if !ctrl && !alt => self.insert(c),
            KeyCode::Char('a') if ctrl => self.home(),
            KeyCode::Char('e') if ctrl => self.end(),
            KeyCode::Char('u') if ctrl => {
                self.clear();
            }
            KeyCode::Char('k') if ctrl => self.kill_to_end(),
            KeyCode::Char('w') if ctrl => self.delete_word(),
            KeyCode::Backspace if alt => self.delete_word(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.left(),
            KeyCode::Right => self.right(),
            KeyCode::Home => self.home(),
            KeyCode::End => self.end(),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_is_char_safe() {
        let mut f = Field::new("héllo");
        f.left();
        f.left();
        f.insert('X');
        assert_eq!(f.value, "hélXlo");
        f.backspace();
        f.delete();
        assert_eq!(f.value, "hélo");
        f.end();
        f.delete_word();
        assert_eq!(f.value, "");
        let mut f = Field::new("two words here");
        f.delete_word();
        assert_eq!(f.value, "two words ");
        f.home();
        f.kill_to_end();
        assert_eq!(f.value, "");
    }
}
