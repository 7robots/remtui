//! A headless driver for `App`: injected keys and clicks, a test backend to
//! draw into, and a wait loop that runs the message and timer plumbing the
//! real main loop would. Tests read the drawn buffer as text.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::app::{App, Msg};
use crate::client::RemctlClient;
use crate::config::Config;

pub struct Harness {
    pub app: App,
    rx: UnboundedReceiver<Msg>,
    terminal: Terminal<TestBackend>,
}

impl Harness {
    pub fn new(config: Config, client: Arc<RemctlClient>, vim: bool, size: (u16, u16)) -> Harness {
        Self::with_env(config, client, vim, size, HashMap::new())
    }

    /// A harness whose app sees `environ` instead of the process environment.
    pub fn with_env(
        config: Config,
        client: Arc<RemctlClient>,
        vim: bool,
        size: (u16, u16),
        environ: HashMap<String, String>,
    ) -> Harness {
        let (app, rx) = App::new(config, client, vim, environ);
        let terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("test backend");
        Harness { app, rx, terminal }
    }

    /// Kick off the first loads and draw the first frame.
    pub fn start(&mut self) {
        self.app.start();
        self.draw();
    }

    pub fn draw(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| crate::ui::draw(frame, app))
            .expect("draw");
    }

    /// Deliver pending messages and timers, run any editor job inline, redraw.
    fn pump(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.app.handle_msg(msg);
        }
        self.run_editor_jobs();
        self.app.tick(Instant::now());
        self.draw();
    }

    fn run_editor_jobs(&mut self) {
        while let Some(job) = self.app.take_editor_job() {
            let outcome = crate::editor::run(&job, true);
            self.app.editor_done(job, outcome);
        }
    }

    /// Run the plumbing until `pred` holds or `timeout` passes.
    pub async fn wait_until(
        &mut self,
        pred: impl Fn(&App) -> bool,
        timeout: Duration,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            self.pump();
            if pred(&self.app) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "condition not met within {timeout:?}\n{}",
                    self.text()
                ));
            }
            let next = self
                .app
                .next_deadline()
                .unwrap_or(Instant::now() + Duration::from_millis(25));
            let step = next
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(25));
            tokio::select! {
                msg = self.rx.recv() => {
                    if let Some(msg) = msg { self.app.handle_msg(msg); }
                }
                _ = tokio::time::sleep(step) => {}
            }
        }
    }

    /// `wait_until` with five seconds, panicking on timeout.
    pub async fn until(&mut self, pred: impl Fn(&App) -> bool) {
        if let Err(err) = self.wait_until(pred, Duration::from_secs(5)).await {
            panic!("{err}");
        }
    }

    /// Let queued work drain for a moment.
    pub async fn settle(&mut self) {
        let _ = self.wait_until(|_| false, Duration::from_millis(60)).await;
    }

    /// Wait for the sidebar and the first view.
    pub async fn load(&mut self) {
        self.until(|app| app.loaded && !app.lists.is_empty() && app.view_loaded && !app.loading)
            .await;
    }

    /// Wait for a view reload that is in flight to land.
    pub async fn reloaded(&mut self) {
        self.until(|app| !app.loading).await;
    }

    pub fn key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app.handle_key(KeyEvent::new(code, modifiers));
        self.run_editor_jobs();
        self.draw();
    }

    /// Press a key by Textual's names: `j`, `down`, `enter`, `escape`,
    /// `tab`, `shift+tab`, `ctrl+d`, `question_mark`, `slash`, `G`.
    pub fn press(&mut self, name: &str) {
        let (code, modifiers) = match name {
            "enter" => (KeyCode::Enter, KeyModifiers::NONE),
            "escape" | "esc" => (KeyCode::Esc, KeyModifiers::NONE),
            "tab" => (KeyCode::Tab, KeyModifiers::NONE),
            "shift+tab" => (KeyCode::BackTab, KeyModifiers::SHIFT),
            "down" => (KeyCode::Down, KeyModifiers::NONE),
            "up" => (KeyCode::Up, KeyModifiers::NONE),
            "left" => (KeyCode::Left, KeyModifiers::NONE),
            "right" => (KeyCode::Right, KeyModifiers::NONE),
            "space" => (KeyCode::Char(' '), KeyModifiers::NONE),
            "backspace" => (KeyCode::Backspace, KeyModifiers::NONE),
            "delete" => (KeyCode::Delete, KeyModifiers::NONE),
            "end" => (KeyCode::End, KeyModifiers::NONE),
            "home" => (KeyCode::Home, KeyModifiers::NONE),
            "pagedown" => (KeyCode::PageDown, KeyModifiers::NONE),
            "pageup" => (KeyCode::PageUp, KeyModifiers::NONE),
            "question_mark" => (KeyCode::Char('?'), KeyModifiers::SHIFT),
            "slash" => (KeyCode::Char('/'), KeyModifiers::NONE),
            "colon" => (KeyCode::Char(':'), KeyModifiers::SHIFT),
            other => {
                if let Some(rest) = other.strip_prefix("ctrl+") {
                    let ch = rest.chars().next().expect("a key name");
                    (KeyCode::Char(ch), KeyModifiers::CONTROL)
                } else {
                    let ch = other.chars().next().expect("a key name");
                    let modifiers = if ch.is_uppercase() {
                        KeyModifiers::SHIFT
                    } else {
                        KeyModifiers::NONE
                    };
                    (KeyCode::Char(ch), modifiers)
                }
            }
        };
        self.key(code, modifiers);
    }

    pub fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.key(
                KeyCode::Char(ch),
                if ch.is_uppercase() {
                    KeyModifiers::SHIFT
                } else {
                    KeyModifiers::NONE
                },
            );
        }
    }

    pub fn click(&mut self, x: u16, y: u16) {
        self.app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        });
        self.draw();
    }

    pub fn scroll(&mut self, x: u16, y: u16, down: bool) {
        let kind = if down {
            MouseEventKind::ScrollDown
        } else {
            MouseEventKind::ScrollUp
        };
        self.app.handle_mouse(MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        });
        self.draw();
    }

    /// Is `(x, y)` the cell hidden under a wide glyph to its left? ratatui's
    /// diff never sends those cells, so the test backend keeps stale content
    /// there; a real terminal paints the glyph over both.
    fn is_continuation(&self, x: u16, y: u16) -> bool {
        if x == 0 {
            return false;
        }
        let left = self.terminal.backend().buffer()[(x - 1, y)].symbol();
        unicode_width::UnicodeWidthStr::width(left) == 2
    }

    /// The screen as text, one string per row, trailing spaces trimmed.
    pub fn lines(&self) -> Vec<String> {
        let buffer = self.terminal.backend().buffer();
        let area = buffer.area;
        (0..area.height)
            .map(|y| {
                let mut row = String::new();
                for x in 0..area.width {
                    if !self.is_continuation(x, y) {
                        row.push_str(buffer[(x, y)].symbol());
                    }
                }
                row.trim_end().to_string()
            })
            .collect()
    }

    pub fn text(&self) -> String {
        self.lines().join("\n")
    }

    pub fn row(&self, y: u16) -> String {
        let buffer = self.terminal.backend().buffer();
        (0..buffer.area.width)
            .filter(|x| !self.is_continuation(*x, y))
            .map(|x| buffer[(x, y)].symbol().to_string())
            .collect()
    }

    pub fn cell_bg(&self, x: u16, y: u16) -> Color {
        self.terminal.backend().buffer()[(x, y)].bg
    }

    pub fn cell_fg(&self, x: u16, y: u16) -> Color {
        self.terminal.backend().buffer()[(x, y)].fg
    }

    /// The screen row whose text contains `needle`, if any.
    pub fn find_row(&self, needle: &str) -> Option<u16> {
        self.lines()
            .iter()
            .position(|l| l.contains(needle))
            .map(|y| y as u16)
    }

    pub fn size(&self) -> (u16, u16) {
        let area = self.terminal.backend().buffer().area;
        (area.width, area.height)
    }

    /// The titles of the reminder rows as shown, in order.
    pub fn shown_titles(&self) -> Vec<String> {
        self.app
            .shown
            .iter()
            .map(|&i| self.app.reminders[i].title.clone())
            .collect()
    }
}
