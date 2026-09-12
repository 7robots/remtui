//! Application state and the update loop.
//!
//! One task owns `App` and draws each frame from it. remctl work runs in tokio
//! tasks and reports back on one channel as `Msg` values tagged with a
//! generation number; a stale result is dropped, never cancelled mid-flight.
//! Modals are an `Overlay` value on the state, and the terminal is never
//! touched from here.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::bear::BearClient;
use crate::client::{RemctlClient, RemctlError, flag_target, warnings_of};
use crate::config::{BearConfig, Config};
use crate::editor::EditorJob;
use crate::keys::{Action, Keymap};
use crate::models::{Reminder, ReminderList, SmartView};
use crate::ui::field::Field;
use crate::ui::form::{Form, FormEvent};

/// Two `g` presses this close together are `gg` in the vim profile.
pub const GG_CHORD: Duration = Duration::from_millis(750);
pub const UPCOMING_DAYS: u32 = 7;
/// Two clicks this close together on the same row open the editor.
pub const DOUBLE_CLICK: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Nav,
    Reminders,
    Filter,
}

/// What the main list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Smart(SmartView),
    /// A list, by id.
    List(i64),
}

/// One row of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavItem {
    Header(&'static str),
    Blank,
    Smart(SmartView),
    /// Index into `App::lists`.
    List(usize),
}

impl NavItem {
    pub fn selectable(&self) -> bool {
        matches!(self, NavItem::Smart(_) | NavItem::List(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Information,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub title: String,
    pub message: String,
    pub severity: Severity,
    pub expires: Instant,
}

/// What a confirmed dialog goes on to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    Delete(Reminder),
}

/// A modal over the main screen.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Overlay {
    Help {
        scroll: usize,
    },
    Form(Box<Form>),
    Confirm {
        title: String,
        message: String,
        confirm_label: String,
        danger: bool,
        /// The confirm button has focus (Cancel is focused first, so a stray
        /// Enter right after `d` cannot delete).
        focus_confirm: bool,
        action: Pending,
    },
}

impl Overlay {
    pub fn name(&self) -> &'static str {
        match self {
            Overlay::Help { .. } => "help",
            Overlay::Form(_) => "form",
            Overlay::Confirm { .. } => "confirm",
        }
    }
}

/// Results of background work.
#[derive(Debug)]
pub enum Msg {
    Lists {
        generation: u64,
        result: Result<Vec<ReminderList>, RemctlError>,
    },
    View {
        generation: u64,
        result: Result<Vec<Reminder>, RemctlError>,
    },
    /// A done/undone/delete/priority write finished.
    Mutated {
        message: String,
        result: Result<Option<serde_json::Value>, RemctlError>,
    },
    /// A flag/unflag write finished.
    Flagged {
        id: i64,
        wanted: bool,
        result: Result<Option<serde_json::Value>, RemctlError>,
    },
    /// The form's add or field edit finished.
    Saved {
        is_edit: bool,
        /// A flag change to write once the field edit has landed.
        follow_flag: Option<(i64, bool)>,
        result: Result<Option<serde_json::Value>, RemctlError>,
    },
}

/// Where the draw pass put things, for mouse hit testing.
#[derive(Debug, Clone, Default)]
pub struct Rects {
    pub nav_rows: Rect,
    pub list_rows: Rect,
    /// (first screen row, height) of each visible reminder row, in `shown` order from `scroll`.
    pub row_spans: Vec<(u16, u16)>,
    pub filter: Rect,
}

pub struct App {
    pub config: Config,
    pub keymap: Keymap,
    pub client: Arc<RemctlClient>,
    pub bear: Option<Arc<BearClient>>,
    pub bear_config: BearConfig,
    pub environ: HashMap<String, String>,
    pub running: bool,
    pub focus: Pane,

    pub lists: Vec<ReminderList>,
    pub nav: Vec<NavItem>,
    pub nav_cursor: usize,
    pub nav_scroll: usize,

    pub view: ViewKind,
    pub reminders: Vec<Reminder>,
    /// Indices into `reminders` that pass the filter, in display order.
    pub shown: Vec<usize>,
    /// Index into `shown`.
    pub cursor: usize,
    pub scroll: usize,
    pub filter_text: String,
    pub filter: Option<Field>,
    pub show_completed: bool,
    pub loading: bool,
    /// The first lists reply has arrived.
    pub loaded: bool,
    pub view_loaded: bool,

    pub overlay: Option<Overlay>,
    pub toasts: Vec<Toast>,
    pub rects: Rects,
    pub show_logo: bool,

    tx: UnboundedSender<Msg>,
    lists_gen: u64,
    view_gen: u64,
    last_g: Option<Instant>,
    last_click: Option<(Instant, usize)>,
    /// Row heights of `shown` as last drawn, for paging by viewport share.
    pub row_heights: Vec<u16>,
    pub list_height: u16,
    /// Reminder ids with a flag write in flight.
    pub pending_flags: std::collections::HashSet<i64>,
    editor_job: Option<EditorJob>,
}

impl App {
    pub fn new(
        config: Config,
        client: Arc<RemctlClient>,
        vim: bool,
        environ: HashMap<String, String>,
    ) -> (App, UnboundedReceiver<Msg>) {
        let (tx, rx) = unbounded_channel();
        let keymap = Keymap::new(vim, &config.overrides);
        let bear_config = config.bear.clone();
        let app = App {
            config,
            keymap,
            client,
            bear: None,
            bear_config,
            environ,
            running: true,
            focus: Pane::Nav,
            lists: Vec::new(),
            nav: Vec::new(),
            nav_cursor: 1,
            nav_scroll: 0,
            view: ViewKind::Smart(SmartView::Today),
            reminders: Vec::new(),
            shown: Vec::new(),
            cursor: 0,
            scroll: 0,
            filter_text: String::new(),
            filter: None,
            show_completed: false,
            loading: false,
            loaded: false,
            view_loaded: false,
            overlay: None,
            toasts: Vec::new(),
            rects: Rects::default(),
            show_logo: true,
            tx,
            lists_gen: 0,
            view_gen: 0,
            last_g: None,
            last_click: None,
            row_heights: Vec::new(),
            list_height: 0,
            pending_flags: Default::default(),
            editor_job: None,
        };
        (app, rx)
    }

    /// First loads: the sidebar and the initial view.
    pub fn start(&mut self) {
        self.build_nav();
        self.refresh_lists();
        self.load_view();
    }

    pub fn env(&self, name: &str) -> Option<String> {
        self.environ.get(name).cloned().filter(|v| !v.is_empty())
    }

    pub fn bear_enabled(&self) -> bool {
        self.bear_config.enabled
    }

    // -- toasts ---------------------------------------------------------------

    pub fn notify(&mut self, message: impl Into<String>, seconds: u64) {
        self.notify_titled("", message, Severity::Information, seconds);
    }

    pub fn notify_titled(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        severity: Severity,
        seconds: u64,
    ) {
        self.toasts.push(Toast {
            title: title.into(),
            message: message.into(),
            severity,
            expires: Instant::now() + Duration::from_secs(seconds),
        });
    }

    pub fn error(&mut self, err: &RemctlError) {
        self.notify_titled("remctl", err.message.clone(), Severity::Error, 8);
    }

    pub fn toast_messages(&self) -> Vec<String> {
        self.toasts.iter().map(|t| t.message.clone()).collect()
    }

    // -- timers ---------------------------------------------------------------

    pub fn next_deadline(&self) -> Option<Instant> {
        let mut next: Option<Instant> = self.toasts.iter().map(|t| t.expires).min();
        if let Some(at) = self.last_g {
            let chord = at + GG_CHORD;
            next = Some(next.map_or(chord, |n| n.min(chord)));
        }
        next
    }

    pub fn tick(&mut self, now: Instant) {
        self.toasts.retain(|t| t.expires > now);
        if self.last_g.is_some_and(|at| now >= at + GG_CHORD) {
            self.last_g = None;
        }
    }

    // -- loading --------------------------------------------------------------

    pub fn refresh_lists(&mut self) {
        self.lists_gen += 1;
        let generation = self.lists_gen;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.get_lists().await;
            let _ = tx.send(Msg::Lists { generation, result });
        });
    }

    pub fn load_view(&mut self) {
        self.view_gen += 1;
        let generation = self.view_gen;
        self.loading = true;
        let client = self.client.clone();
        let tx = self.tx.clone();
        let view = self.view;
        let list_title = self.current_list().map(|l| l.title.clone());
        let show_completed = self.show_completed;
        tokio::spawn(async move {
            let result = match view {
                ViewKind::Smart(SmartView::Today) => client.today().await,
                ViewKind::Smart(SmartView::Upcoming) => client.upcoming(UPCOMING_DAYS).await,
                ViewKind::Smart(SmartView::Overdue) => client.overdue().await,
                ViewKind::Smart(SmartView::Flagged) => client.flagged().await,
                ViewKind::List(_) => match list_title {
                    Some(title) => client.get_reminders(&title, show_completed).await,
                    None => Ok(Vec::new()),
                },
            };
            let _ = tx.send(Msg::View { generation, result });
        });
    }

    /// Reload both the view and the sidebar (`r`, and after every write).
    pub fn reload(&mut self) {
        self.load_view();
        self.refresh_lists();
    }

    pub fn handle_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Lists { generation, result } => {
                if generation != self.lists_gen {
                    return;
                }
                self.loaded = true;
                match result {
                    Ok(lists) => {
                        self.lists = lists;
                        self.build_nav();
                    }
                    Err(err) => self.error(&err),
                }
            }
            Msg::View { generation, result } => {
                if generation != self.view_gen {
                    return;
                }
                self.loading = false;
                self.view_loaded = true;
                // Remember the selection before the rows change under it.
                let previous_id = self.selected().map(|r| r.id);
                match result {
                    Ok(items) => self.reminders = items,
                    Err(err) => {
                        self.error(&err);
                        self.reminders.clear();
                    }
                }
                self.populate_keeping(previous_id);
            }
            Msg::Mutated { message, result } => match result {
                Ok(_) => {
                    if !message.is_empty() {
                        self.notify(message, 3);
                    }
                    self.reload();
                }
                Err(err) => self.error(&err),
            },
            Msg::Flagged { id, wanted, result } => {
                self.pending_flags.remove(&id);
                match result {
                    Ok(_) => self.reload(),
                    Err(err) => {
                        // revert the optimistic toggle
                        if let Some(r) = self.reminders.iter_mut().find(|r| r.id == id) {
                            r.flagged = !wanted;
                        }
                        self.error(&err);
                    }
                }
            }
            Msg::Saved {
                is_edit,
                follow_flag,
                result,
            } => match result {
                Ok(payload) => {
                    for warning in warnings_of(payload.as_ref()) {
                        self.notify_titled("remctl", warning, Severity::Warning, 8);
                    }
                    self.overlay = None;
                    self.notify(
                        if is_edit {
                            "✎ Reminder updated"
                        } else {
                            "＋ Reminder added"
                        },
                        3,
                    );
                    if let Some((id, wanted)) = follow_flag {
                        let target = flag_target(payload.as_ref(), id);
                        self.write_flag(target, wanted);
                    }
                    self.reload();
                }
                Err(err) => {
                    if let Some(Overlay::Form(form)) = &mut self.overlay {
                        form.error = format!("⚠ {}", err.message);
                        form.saving = false;
                    } else {
                        self.error(&err);
                    }
                }
            },
        }
    }

    // -- the sidebar ----------------------------------------------------------

    /// Rebuild the sidebar rows, keeping the highlight on the same view.
    pub fn build_nav(&mut self) {
        let mut nav = vec![NavItem::Header("SMART LISTS")];
        nav.extend(SmartView::ALL.iter().map(|v| NavItem::Smart(*v)));
        if !self.lists.is_empty() {
            nav.push(NavItem::Blank);
            nav.push(NavItem::Header("MY LISTS"));
            nav.extend((0..self.lists.len()).map(NavItem::List));
        }
        self.nav = nav;
        let wanted = self.view;
        let found = self.nav.iter().position(|item| match (item, wanted) {
            (NavItem::Smart(v), ViewKind::Smart(w)) => *v == w,
            (NavItem::List(i), ViewKind::List(id)) => {
                self.lists.get(*i).is_some_and(|l| l.id == id)
            }
            _ => false,
        });
        match found {
            Some(index) => self.nav_cursor = index,
            None => {
                // The view's list is gone: fall back to the first smart view
                // and load it.
                self.nav_cursor = 1;
                if matches!(wanted, ViewKind::List(_)) {
                    self.view = ViewKind::Smart(SmartView::Today);
                    self.clear_filter_state();
                    self.load_view();
                }
            }
        }
    }

    pub fn current_list(&self) -> Option<&ReminderList> {
        match self.view {
            ViewKind::List(id) => self.lists.iter().find(|l| l.id == id),
            ViewKind::Smart(_) => None,
        }
    }

    pub fn current_smart(&self) -> Option<SmartView> {
        match self.view {
            ViewKind::Smart(v) => Some(v),
            ViewKind::List(_) => None,
        }
    }

    /// The list a new reminder goes to: the current one, else the first.
    pub fn default_list_title(&self) -> String {
        self.current_list()
            .or(self.lists.first())
            .map(|l| l.title.clone())
            .unwrap_or_default()
    }

    /// Move the sidebar highlight; landing on a new view loads it.
    fn nav_move(&mut self, direction: i64) {
        let len = self.nav.len() as i64;
        if len == 0 {
            return;
        }
        let mut index = self.nav_cursor as i64;
        loop {
            index += direction;
            if index < 0 || index >= len {
                return;
            }
            if self.nav[index as usize].selectable() {
                break;
            }
        }
        self.nav_select(index as usize);
    }

    fn nav_select(&mut self, index: usize) {
        if index >= self.nav.len() || !self.nav[index].selectable() {
            return;
        }
        self.nav_cursor = index;
        let view = match &self.nav[index] {
            NavItem::Smart(v) => ViewKind::Smart(*v),
            NavItem::List(i) => ViewKind::List(self.lists[*i].id),
            _ => return,
        };
        if view != self.view {
            self.view = view;
            self.clear_filter_state();
            // A new view starts at the top; the old rows would otherwise pick
            // the selection for it.
            self.reminders.clear();
            self.shown.clear();
            self.cursor = 0;
            self.scroll = 0;
            self.load_view();
        }
    }

    fn nav_edge(&mut self, top: bool) {
        let index = if top {
            self.nav.iter().position(NavItem::selectable)
        } else {
            self.nav.iter().rposition(NavItem::selectable)
        };
        if let Some(index) = index {
            self.nav_select(index);
        }
    }

    // -- the list -------------------------------------------------------------

    /// Apply the filter and keep the selection: same id, else the old index
    /// clamped, so completing a row does not jump to the top.
    pub fn populate(&mut self) {
        let previous_id = self.selected().map(|r| r.id);
        self.populate_keeping(previous_id);
    }

    fn populate_keeping(&mut self, previous_id: Option<i64>) {
        let previous_index = self.cursor;
        self.shown = self
            .reminders
            .iter()
            .enumerate()
            .filter(|(_, r)| self.filter_text.is_empty() || r.matches(&self.filter_text))
            .map(|(i, _)| i)
            .collect();
        if self.shown.is_empty() {
            self.cursor = 0;
            self.scroll = 0;
            return;
        }
        let by_id =
            previous_id.and_then(|id| self.shown.iter().position(|&i| self.reminders[i].id == id));
        self.cursor = by_id.unwrap_or_else(|| previous_index.min(self.shown.len() - 1));
    }

    pub fn selected(&self) -> Option<&Reminder> {
        self.shown.get(self.cursor).map(|&i| &self.reminders[i])
    }

    /// Whether the selection-dependent keys act right now.
    pub fn has_selection(&self) -> bool {
        self.view_loaded && !self.shown.is_empty()
    }

    fn list_move(&mut self, delta: i64) {
        if self.shown.is_empty() {
            return;
        }
        let max = self.shown.len() as i64 - 1;
        self.cursor = (self.cursor as i64 + delta).clamp(0, max) as usize;
    }

    /// Move by a share of the viewport, counting variable row heights.
    fn list_page(&mut self, direction: i64, fraction: f32) {
        if self.shown.is_empty() {
            return;
        }
        let mut budget = ((self.list_height as f32) * fraction).floor().max(1.0) as i64;
        let mut index = self.cursor as i64;
        let len = self.shown.len() as i64;
        while (0..len).contains(&(index + direction)) && budget > 0 {
            index += direction;
            budget -= self
                .row_heights
                .get(index as usize)
                .copied()
                .unwrap_or(1)
                .max(1) as i64;
        }
        self.cursor = index as usize;
    }

    /// Keep the cursor row on screen; called by the draw pass with the
    /// heights it is about to draw.
    pub fn ensure_visible(&mut self, heights: &[u16], height: u16) {
        self.row_heights = heights.to_vec();
        self.list_height = height;
        if heights.is_empty() {
            self.scroll = 0;
            return;
        }
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        }
        loop {
            let used: u16 = heights[self.scroll..=self.cursor.min(heights.len() - 1)]
                .iter()
                .sum();
            if used <= height || self.scroll >= self.cursor {
                break;
            }
            self.scroll += 1;
        }
        self.scroll = self.scroll.min(heights.len().saturating_sub(1));
    }

    // -- filter ---------------------------------------------------------------

    fn clear_filter_state(&mut self) {
        self.filter_text.clear();
        self.filter = None;
        if self.focus == Pane::Filter {
            self.focus = Pane::Reminders;
        }
    }

    fn show_filter(&mut self) {
        if self.filter.is_none() {
            self.filter = Some(Field::new(&self.filter_text));
        }
        self.focus = Pane::Filter;
    }

    /// `esc`: clear and hide the filter when there is one; true if it acted.
    fn dismiss_filter(&mut self) -> bool {
        if self.filter.is_none() && self.filter_text.is_empty() {
            return false;
        }
        self.clear_filter_state();
        self.populate();
        self.focus = Pane::Reminders;
        true
    }

    fn apply_filter(&mut self) {
        let text = self
            .filter
            .as_ref()
            .map(|f| f.value.trim().to_string())
            .unwrap_or_default();
        if text != self.filter_text {
            self.filter_text = text;
            self.populate();
        }
    }

    // -- keys -----------------------------------------------------------------

    pub fn handle_key(&mut self, key: KeyEvent) {
        if let Some(overlay) = self.overlay.clone() {
            self.overlay_key(overlay, &key);
            return;
        }
        if self.focus == Pane::Filter {
            self.filter_key(&key);
            return;
        }
        if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.open_help();
            return;
        }
        let action = self.keymap.action(&key);
        // A pending `g` is cancelled by any other key.
        if action != Some(Action::Top) {
            self.last_g = None;
        }
        let Some(action) = action else {
            self.unbound_key(&key);
            return;
        };
        self.run_action(action);
    }

    /// Keys the table does not name: arrows, enter, paging, home/end.
    fn unbound_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Down => self.run_action(Action::Down),
            KeyCode::Up => self.run_action(Action::Up),
            KeyCode::Enter => match self.focus {
                Pane::Nav => self.focus = Pane::Reminders,
                Pane::Reminders => self.run_action(Action::Edit),
                Pane::Filter => {}
            },
            KeyCode::PageDown => self.page(1, 1.0),
            KeyCode::PageUp => self.page(-1, 1.0),
            KeyCode::Home => self.edge(true),
            KeyCode::End => self.edge(false),
            _ => {}
        }
    }

    fn page(&mut self, direction: i64, fraction: f32) {
        match self.focus {
            Pane::Nav => self.nav_edge(direction < 0),
            _ => self.list_page(direction, fraction),
        }
    }

    fn edge(&mut self, top: bool) {
        match self.focus {
            Pane::Nav => self.nav_edge(top),
            _ => {
                if !self.shown.is_empty() {
                    self.cursor = if top { 0 } else { self.shown.len() - 1 };
                }
            }
        }
    }

    pub fn run_action(&mut self, action: Action) {
        match action {
            Action::Down => match self.focus {
                Pane::Nav => self.nav_move(1),
                _ => self.list_move(1),
            },
            Action::Up => match self.focus {
                Pane::Nav => self.nav_move(-1),
                _ => self.list_move(-1),
            },
            Action::FocusNav => self.focus = Pane::Nav,
            Action::FocusReminders => self.focus = Pane::Reminders,
            Action::SwitchPane => {
                self.focus = if self.focus == Pane::Nav {
                    Pane::Reminders
                } else {
                    Pane::Nav
                };
            }
            Action::Top => {
                if self.keymap.vim {
                    let now = Instant::now();
                    match self.last_g {
                        Some(at) if now.duration_since(at) <= GG_CHORD => {
                            self.last_g = None;
                            self.edge(true);
                        }
                        _ => self.last_g = Some(now),
                    }
                } else {
                    self.edge(true);
                }
            }
            Action::Bottom => self.edge(false),
            Action::HalfPageDown => self.page(1, 0.5),
            Action::HalfPageUp => self.page(-1, 0.5),
            Action::PageDown => self.page(1, 1.0),
            Action::PageUp => self.page(-1, 1.0),
            Action::Filter => self.show_filter(),
            Action::DismissFilter => {
                self.dismiss_filter();
            }
            Action::ToggleCompleted => {
                self.show_completed = !self.show_completed;
                let message = if self.show_completed {
                    "Completed reminders shown (list views)"
                } else {
                    "Completed reminders hidden (list views)"
                };
                self.notify(message, 3);
                self.load_view();
            }
            Action::Refresh => self.reload(),
            Action::Help | Action::Palette => self.open_help(),
            Action::Quit => self.running = false,
            Action::BearTriage => {
                if self.bear_enabled() {
                    self.open_triage();
                }
            }
            Action::Add | Action::VimNew => self.add_reminder(),
            Action::Edit => self.edit_selected(),
            Action::ToggleDone => self.toggle_done(),
            Action::Delete => self.delete_selected(),
            Action::ToggleFlag => self.toggle_flag(),
            Action::CyclePriority => self.cycle_priority(),
        }
    }

    fn filter_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.dismiss_filter();
            }
            KeyCode::Enter | KeyCode::Tab => self.focus = Pane::Reminders,
            KeyCode::Down => {
                self.focus = Pane::Reminders;
                self.list_move(1);
            }
            KeyCode::Up => {
                self.focus = Pane::Reminders;
            }
            _ => {
                if let Some(field) = &mut self.filter
                    && field.handle(key)
                {
                    self.apply_filter();
                }
            }
        }
    }

    fn overlay_key(&mut self, overlay: Overlay, key: &KeyEvent) {
        match overlay {
            Overlay::Form(mut form) => {
                let event = form.handle(key);
                self.overlay = Some(Overlay::Form(form));
                match event {
                    FormEvent::None => {}
                    FormEvent::Cancel => {
                        if let Some(Overlay::Form(form)) = &self.overlay
                            && !form.saving
                        {
                            self.overlay = None;
                        }
                    }
                    FormEvent::Save => self.save_form(),
                    FormEvent::Editor => self.open_notes_editor(),
                }
            }
            Overlay::Confirm {
                title,
                message,
                confirm_label,
                danger,
                focus_confirm,
                action,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('n') => self.overlay = None,
                KeyCode::Char('y') => {
                    self.overlay = None;
                    self.run_pending(action);
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.overlay = None;
                    if focus_confirm {
                        self.run_pending(action);
                    }
                }
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                    self.overlay = Some(Overlay::Confirm {
                        title,
                        message,
                        confirm_label,
                        danger,
                        focus_confirm: !focus_confirm,
                        action,
                    });
                }
                _ => {}
            },
            Overlay::Help { scroll } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => self.overlay = None,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.overlay = Some(Overlay::Help { scroll: scroll + 1 });
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.overlay = Some(Overlay::Help {
                        scroll: scroll.saturating_sub(1),
                    });
                }
                _ => {}
            },
        }
    }

    pub fn open_help(&mut self) {
        self.overlay = Some(Overlay::Help { scroll: 0 });
    }

    // -- mouse ----------------------------------------------------------------

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.overlay.is_some() {
            return;
        }
        let (x, y) = (mouse.column, mouse.row);
        let inside = |r: Rect| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if inside(self.rects.nav_rows) {
                    let index = self.nav_scroll + (y - self.rects.nav_rows.y) as usize;
                    if self.nav.get(index).is_some_and(NavItem::selectable) {
                        self.focus = Pane::Nav;
                        self.nav_select(index);
                    }
                } else if inside(self.rects.list_rows) {
                    let hit = self
                        .rects
                        .row_spans
                        .iter()
                        .position(|(top, h)| y >= *top && y < top + h)
                        .map(|offset| self.scroll + offset);
                    if let Some(index) = hit.filter(|i| *i < self.shown.len()) {
                        self.focus = Pane::Reminders;
                        self.cursor = index;
                        let now = Instant::now();
                        let double = self.last_click.is_some_and(|(at, i)| {
                            i == index && now.duration_since(at) <= DOUBLE_CLICK
                        });
                        self.last_click = Some((now, index));
                        if double {
                            self.last_click = None;
                            self.edit_selected();
                        }
                    }
                } else if inside(self.rects.filter) {
                    self.show_filter();
                }
            }
            MouseEventKind::ScrollDown => {
                if inside(self.rects.nav_rows) {
                    self.focus = Pane::Nav;
                    self.nav_move(1);
                } else if inside(self.rects.list_rows) {
                    self.list_move(1);
                }
            }
            MouseEventKind::ScrollUp => {
                if inside(self.rects.nav_rows) {
                    self.focus = Pane::Nav;
                    self.nav_move(-1);
                } else if inside(self.rects.list_rows) {
                    self.list_move(-1);
                }
            }
            _ => {}
        }
    }

    // -- editor round trip ----------------------------------------------------

    /// Whether a flag write is in flight for this reminder.
    pub fn is_pending(&self, id: i64) -> bool {
        self.pending_flags.contains(&id)
    }

    /// The main loop asks for a parked editor job and runs it with the
    /// terminal restored.
    pub fn take_editor_job(&mut self) -> Option<EditorJob> {
        self.editor_job.take()
    }

    pub fn editor_done(&mut self, job: EditorJob, outcome: std::io::Result<()>) {
        let text = match outcome.and_then(|_| crate::editor::result(&job)) {
            Ok(text) => Some(text),
            Err(err) => {
                self.notify_titled("editor", err.to_string(), Severity::Error, 8);
                None
            }
        };
        crate::editor::cleanup(&job);
        if let (Some(text), Some(Overlay::Form(form))) = (text, &mut self.overlay) {
            form.notes.set(&text);
        }
    }

    /// `ctrl+e` in the form: hand the notes to `$EDITOR`.
    fn open_notes_editor(&mut self) {
        let Some(Overlay::Form(form)) = &self.overlay else {
            return;
        };
        if form.saving {
            return;
        }
        let editor = crate::editor::resolve_editor(&|name| self.env(name));
        match crate::editor::prepare(&form.notes.text, &editor) {
            Ok(job) => self.editor_job = Some(job),
            Err(err) => self.notify_titled("editor", err.to_string(), Severity::Error, 8),
        }
    }

    // -- mutations ------------------------------------------------------------

    fn spawn_mutation(
        &mut self,
        message: String,
        work: impl std::future::Future<Output = Result<Option<serde_json::Value>, RemctlError>>
        + Send
        + 'static,
    ) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = work.await;
            let _ = tx.send(Msg::Mutated { message, result });
        });
    }

    pub fn add_reminder(&mut self) {
        if self.lists.is_empty() {
            self.notify_titled("", "No lists loaded yet.", Severity::Warning, 5);
            return;
        }
        let titles: Vec<String> = self.lists.iter().map(|l| l.title.clone()).collect();
        let form = Form::new(None, &titles, &self.default_list_title());
        self.overlay = Some(Overlay::Form(Box::new(form)));
    }

    pub fn edit_selected(&mut self) {
        let Some(reminder) = self.selected().cloned() else {
            return;
        };
        let titles: Vec<String> = self.lists.iter().map(|l| l.title.clone()).collect();
        let form = Form::new(Some(&reminder), &titles, "");
        self.overlay = Some(Overlay::Form(Box::new(form)));
    }

    /// `ctrl+s`/enter in the form: validate and write.
    fn save_form(&mut self) {
        let Some(Overlay::Form(form)) = &mut self.overlay else {
            return;
        };
        if form.saving {
            return;
        }
        if !form.validate() {
            return;
        }
        form.saving = true;
        form.error.clear();
        let client = self.client.clone();
        let tx = self.tx.clone();
        let edit = form.reminder.as_ref().map(|r| (r.id, form.edit_fields()));
        let add = form.add_fields();
        match edit {
            None => {
                tokio::spawn(async move {
                    let result = client.add(&add).await;
                    let _ = tx.send(Msg::Saved {
                        is_edit: false,
                        follow_flag: None,
                        result,
                    });
                });
            }
            Some((id, mut fields)) => {
                let follow_flag = fields.flagged.take().map(|wanted| (id, wanted));
                if !fields.has_field_edit() {
                    // Only the flag changed (or nothing): dismiss now, write in the background.
                    self.handle_msg(Msg::Saved {
                        is_edit: true,
                        follow_flag,
                        result: Ok(None),
                    });
                    return;
                }
                tokio::spawn(async move {
                    let result = client.edit(id, &fields).await;
                    let _ = tx.send(Msg::Saved {
                        is_edit: true,
                        follow_flag,
                        result,
                    });
                });
            }
        }
    }

    pub fn toggle_done(&mut self) {
        let Some(r) = self.selected().cloned() else {
            return;
        };
        let client = self.client.clone();
        if r.completed {
            self.spawn_mutation(format!("↺ Reopened “{}”", r.title), async move {
                client.undone(r.id).await
            });
        } else {
            self.spawn_mutation(format!("✓ Completed “{}”", r.title), async move {
                client.done(r.id).await
            });
        }
    }

    pub fn delete_selected(&mut self) {
        let Some(r) = self.selected().cloned() else {
            return;
        };
        self.overlay = Some(Overlay::Confirm {
            title: "🗑  Delete Reminder".into(),
            message: format!(
                "Delete “{}” from {}?\nThis cannot be undone.",
                r.title, r.list_name
            ),
            confirm_label: "Delete".into(),
            danger: true,
            focus_confirm: false,
            action: Pending::Delete(r),
        });
    }

    fn run_pending(&mut self, action: Pending) {
        match action {
            Pending::Delete(r) => {
                let client = self.client.clone();
                self.spawn_mutation(format!("🗑 Deleted “{}”", r.title), async move {
                    client.delete(r.id).await
                });
            }
        }
    }

    /// `f`: toggle the flag optimistically and write it in the background.
    pub fn toggle_flag(&mut self) {
        let Some(r) = self.selected().cloned() else {
            return;
        };
        if self.is_pending(r.id) {
            return;
        }
        self.write_flag(r.id, !r.flagged);
    }

    /// Flip the row now, mark it pending, and let remctl catch up; a failure
    /// reverts the row with an error toast.
    pub fn write_flag(&mut self, id: i64, wanted: bool) {
        if let Some(r) = self.reminders.iter_mut().find(|r| r.id == id) {
            r.flagged = wanted;
        }
        self.pending_flags.insert(id);
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = if wanted {
                client.flag(id).await
            } else {
                client.unflag(id).await
            };
            let _ = tx.send(Msg::Flagged { id, wanted, result });
        });
    }

    pub fn cycle_priority(&mut self) {
        let Some(r) = self.selected().cloned() else {
            return;
        };
        let next = r.priority.next();
        let client = self.client.clone();
        let fields = crate::client::EditFields {
            priority: Some(next.as_str().to_string()),
            ..Default::default()
        };
        self.spawn_mutation(format!("Priority → {}", next.as_str()), async move {
            client.edit(r.id, &fields).await
        });
    }

    // -- triage (Phase 10) ----------------------------------------------------

    fn open_triage(&mut self) {}
}
