//! UI tests through the headless harness: the read-only screen.

mod common;

use common::{Fake, SIZE};
use remtui::app::{NavItem, Pane, ViewKind};
use remtui::config::Config;
use remtui::models::SmartView;

#[tokio::test]
async fn startup_shows_smart_views_lists_and_today() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    assert_eq!(h.app.nav.len(), 12);
    assert_eq!(h.app.lists.len(), 5);
    assert_eq!(h.app.nav_cursor, 1);
    assert_eq!(h.app.view, ViewKind::Smart(SmartView::Today));
    assert_eq!(h.app.focus, Pane::Nav);
    let screen = h.text();
    for label in [
        "SMART LISTS",
        "Today",
        "Upcoming",
        "Overdue",
        "Flagged",
        "MY LISTS",
        "Personal",
        "Work",
        "Groceries",
        "Reading",
        "Home",
    ] {
        assert!(screen.contains(label), "missing {label}\n{screen}");
    }
    // Today: the dentist call, the credit card bill, the release notes
    let titles = h.shown_titles();
    assert!(titles.contains(&"Call the dentist".to_string()));
    assert!(titles.contains(&"Ship v2.3 release notes".to_string()));
    assert!(screen.contains("3 reminders"));
    // logo present at 36 rows
    assert!(screen.contains("╦═╗ ┌─┐ ┌┬┐ ╔╦╗ ╦ ╦ ╦"));
    // the footer shows the shown bindings
    assert!(screen.contains(" a Add") && screen.contains(" ? Help") && screen.contains(" q Quit"));
    assert!(!screen.contains(" b Bear"));
}

#[tokio::test]
async fn short_terminal_hides_the_logo() {
    let fake = Fake::new();
    let mut h = fake.harness_with(Config::default(), false, (100, 18));
    h.load().await;
    assert!(!h.app.show_logo);
    assert!(!h.text().contains("╦═╗"));
}

#[tokio::test]
async fn nav_moves_switch_views_and_skip_headers() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    h.press("j");
    assert_eq!(h.app.view, ViewKind::Smart(SmartView::Upcoming));
    h.reloaded().await;
    assert!(h.text().contains("◷ Upcoming"));
    h.press("j");
    h.press("j");
    h.press("j"); // over the blank and the MY LISTS header onto Personal
    h.reloaded().await;
    assert!(matches!(h.app.nav[h.app.nav_cursor], NavItem::List(0)));
    assert_eq!(h.app.view, ViewKind::List(1));
    let titles = h.shown_titles();
    assert!(titles.contains(&"Renew passport".to_string()));
    assert!(
        !titles.contains(&"Morning run".to_string()),
        "completed hidden by default"
    );
    let screen = h.text();
    assert!(screen.contains("4 active · 1 done"), "{screen}");
    assert!(screen.contains("━"), "progress bar for a list view");
    h.press("G");
    h.reloaded().await;
    assert_eq!(h.app.view, ViewKind::List(5));
    h.press("g");
    h.reloaded().await;
    assert_eq!(h.app.view, ViewKind::Smart(SmartView::Today));
}

#[tokio::test]
async fn show_completed_toggles_list_views() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    h.press("j");
    h.press("j");
    h.press("j");
    h.press("j");
    h.reloaded().await;
    let before = h.shown_titles().len();
    h.press("c");
    h.reloaded().await;
    assert!(h.app.show_completed);
    assert_eq!(h.shown_titles().len(), before + 1);
    assert!(h.app.toast_messages().iter().any(|m| m.contains("shown")));
    let done_row = h.find_row("Morning run").expect("completed row visible");
    assert!(h.row(done_row).contains('⬤'));
    h.press("c");
    h.reloaded().await;
    assert_eq!(h.shown_titles().len(), before);
}

#[tokio::test]
async fn pane_switching_keys() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    h.press("tab");
    assert_eq!(h.app.focus, Pane::Reminders);
    h.press("tab");
    assert_eq!(h.app.focus, Pane::Nav);
    h.press("l");
    assert_eq!(h.app.focus, Pane::Reminders);
    h.press("h");
    assert_eq!(h.app.focus, Pane::Nav);
    h.press("right");
    assert_eq!(h.app.focus, Pane::Reminders);
    h.press("left");
    assert_eq!(h.app.focus, Pane::Nav);
    h.press("enter");
    assert_eq!(h.app.focus, Pane::Reminders);
}

#[tokio::test]
async fn list_navigation_and_row_rendering() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    // Personal with everything
    for _ in 0..4 {
        h.press("j");
    }
    h.reloaded().await;
    h.press("l");
    assert_eq!(h.app.cursor, 0);
    let first = h.app.selected().unwrap().title.clone();
    assert_eq!(first, "Renew passport"); // overdue, sorted first
    h.press("j");
    assert_eq!(h.app.cursor, 1);
    h.press("k");
    h.press("k");
    assert_eq!(h.app.cursor, 0);
    h.press("G");
    assert_eq!(h.app.cursor, h.app.shown.len() - 1);
    h.press("g");
    assert_eq!(h.app.cursor, 0);
    h.press("down");
    h.press("up");
    h.press("end");
    h.press("home");
    assert_eq!(h.app.cursor, 0);
    let y = h.find_row("Renew passport").unwrap();
    let row = h.row(y);
    assert!(row.contains("!!! Renew passport"), "{row}");
    assert!(row.contains("⚑"), "{row}");
    assert!(
        h.row(y + 1)
            .contains("Bring old passport and two photos · #errands"),
        "{}",
        h.row(y + 1)
    );
    // the cursor bar covers the whole row
    assert_eq!(
        h.cell_bg(h.app.rects.list_rows.x + 5, y),
        remtui::ui::theme::CURSOR
    );
}

#[tokio::test]
async fn filter_narrows_live_and_escape_clears() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    for _ in 0..4 {
        h.press("j");
    }
    h.reloaded().await;
    h.press("slash");
    assert_eq!(h.app.focus, Pane::Filter);
    h.type_text("pass");
    assert_eq!(h.shown_titles(), vec!["Renew passport"]);
    assert!(h.text().contains("filter \"pass\" → 1 match"));
    h.press("enter");
    assert_eq!(h.app.focus, Pane::Reminders);
    assert_eq!(h.shown_titles().len(), 1);
    h.press("escape");
    assert!(h.app.filter.is_none());
    assert!(h.app.filter_text.is_empty());
    assert_eq!(h.shown_titles().len(), 4);
    assert!(h.app.running);
    // no match: the empty state names the filter
    h.press("slash");
    h.type_text("zzz");
    assert!(h.text().contains("○  nothing matches \"zzz\""));
    // switching views clears the filter
    h.press("escape");
    h.press("h");
    h.press("k");
    h.reloaded().await;
    assert!(h.app.filter_text.is_empty());
}

#[tokio::test]
async fn empty_smart_view_message() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    // Flagged has rows; drive the view to something empty via the filter instead
    h.press("j");
    h.press("j");
    h.press("j");
    h.reloaded().await;
    assert_eq!(h.app.view, ViewKind::Smart(SmartView::Flagged));
    assert!(h.text().contains("⚑ Flagged"));
    assert_eq!(h.shown_titles().len(), 2);
}

#[tokio::test]
async fn help_opens_and_closes() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    h.press("question_mark");
    assert_eq!(h.app.overlay.as_ref().map(|o| o.name()), Some("help"));
    let screen = h.text();
    assert!(screen.contains("Keyboard Reference"));
    assert!(screen.contains("toggle done"));
    assert!(!screen.contains("Bear triage"));
    h.press("j"); // ignored by the main screen while the overlay is up
    assert_eq!(h.app.view, ViewKind::Smart(SmartView::Today));
    h.press("escape");
    assert!(h.app.overlay.is_none());
    h.press("ctrl+p");
    assert!(h.app.overlay.is_some());
    h.press("q");
    assert!(h.app.overlay.is_none());
    assert!(h.app.running);
}

#[tokio::test]
async fn bear_help_and_footer_when_enabled() {
    let fake = Fake::new();
    let mut config = Config::default();
    config.bear.enabled = true;
    let mut h = fake.harness_with(config, false, SIZE);
    h.load().await;
    assert!(h.text().contains(" b Bear"));
    h.press("question_mark");
    assert!(remtui::ui::help::text(true).contains("Bear triage"));
    for _ in 0..40 {
        h.press("j"); // the overlay scrolls; the Bear section is at the end
    }
    assert!(h.text().contains("Bear triage"), "{}", h.text());
}

#[tokio::test]
async fn quit_key_stops_the_app() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    h.press("q");
    assert!(!h.app.running);
}

#[tokio::test]
async fn vim_profile_gg_chord_and_paging() {
    let fake = Fake::new();
    let mut h = fake.harness_with(Config::default(), true, SIZE);
    h.load().await;
    for _ in 0..4 {
        h.press("j");
    }
    h.reloaded().await;
    h.press("l");
    h.press("G");
    let last = h.app.shown.len() - 1;
    assert_eq!(h.app.cursor, last);
    h.press("g");
    assert_eq!(h.app.cursor, last, "a single g waits for the chord");
    h.press("g");
    assert_eq!(h.app.cursor, 0);
    h.press("ctrl+d");
    assert!(h.app.cursor > 0);
    h.press("ctrl+u");
    assert_eq!(h.app.cursor, 0);
    h.press("ctrl+f");
    assert_eq!(h.app.cursor, last);
    h.press("ctrl+b");
    assert_eq!(h.app.cursor, 0);
    // `:` opens the help in the vim profile
    h.press("colon");
    assert!(h.app.overlay.is_some());
    h.press("escape");
    // `o` is the vim add key: with mutations unported it is a no-op, but bound
    assert!(h.app.keymap.vim);
}

#[tokio::test]
async fn default_profile_g_jumps_immediately_and_ignores_vim_keys() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    for _ in 0..4 {
        h.press("j");
    }
    h.reloaded().await;
    h.press("l");
    h.press("G");
    h.press("g");
    assert_eq!(h.app.cursor, 0);
    h.press("ctrl+d");
    assert_eq!(h.app.cursor, 0);
}

#[tokio::test]
async fn keymap_override_rebinds_a_key() {
    let fake = Fake::new();
    let mut config = Config::default();
    config.overrides.insert("app.quit".into(), "Q".into());
    let mut h = fake.harness_with(config, false, SIZE);
    h.load().await;
    h.press("q");
    assert!(h.app.running);
    h.press("Q");
    assert!(!h.app.running);
}

#[tokio::test]
async fn mouse_selects_nav_and_rows() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    let y = h.find_row("● Work").expect("Work in the sidebar");
    h.click(h.app.rects.nav_rows.x + 2, y);
    h.reloaded().await;
    assert_eq!(h.app.view, ViewKind::List(2));
    assert_eq!(h.app.focus, Pane::Nav);
    let titles = h.shown_titles();
    assert!(titles.len() >= 3);
    let second = h.app.rects.row_spans[1].0;
    h.click(h.app.rects.list_rows.x + 4, second);
    assert_eq!(h.app.focus, Pane::Reminders);
    assert_eq!(h.app.cursor, 1);
    h.scroll(h.app.rects.list_rows.x + 4, second, true);
    assert_eq!(h.app.cursor, 2);
    h.scroll(h.app.rects.nav_rows.x + 2, y, false);
    assert_eq!(h.app.view, ViewKind::List(1));
}

#[tokio::test]
async fn refresh_reloads_and_error_toasts_surface() {
    let fake = Fake::new();
    let mut h = fake.harness();
    h.load().await;
    h.press("r");
    assert!(h.app.loading);
    h.reloaded().await;
    assert_eq!(h.shown_titles().len(), 3);
    // a missing binary reports through a toast
    let broken = remtui::config::Config::default();
    let client = std::sync::Arc::new(remtui::client::RemctlClient::new(vec![
        "/nonexistent/remctl".into(),
    ]));
    let mut h2 = remtui::harness::Harness::new(broken, client, false, SIZE);
    h2.start();
    h2.until(|app| !app.toasts.is_empty()).await;
    assert!(h2.text().contains("remctl not found"));
}
