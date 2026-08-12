"""End-to-end TUI tests: pilot-driven app against the fake remctl backend."""

import sys

import pytest
from textual.widgets import Button, Checkbox, Input, ListView, OptionList

from remtui.app import RemTuiApp
from remtui.client import RemctlClient
from remtui.panel import RemindersPanel
from remtui.screens import ConfirmDeleteScreen, HelpScreen, ReminderFormScreen
from tests.conftest import FAKE


@pytest.fixture
def app(fake_state) -> RemTuiApp:
    return RemTuiApp(RemctlClient([sys.executable, str(FAKE)]))


@pytest.fixture
def vim_app(fake_state) -> RemTuiApp:
    return RemTuiApp(RemctlClient([sys.executable, str(FAKE)]), vim=True)


async def _settle(pilot, delay: float = 0.6) -> None:
    await pilot.pause(delay)


def _select_list(app: RemTuiApp, title: str) -> None:
    nav = app.query_one("#nav", OptionList)
    lst = next(lst for lst in app.panel.lists if lst.title == title)
    nav.highlighted = nav.get_option_index(f"list:{lst.id}")


async def test_startup_shows_today_and_lists(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        assert app.panel.view_kind == "today"
        assert len(app.panel.lists) == 5
        assert app.panel.reminders, "today view should have seeded reminders"
        nav = app.query_one("#nav", OptionList)
        # 2 headers + 1 spacer + 4 smart views + 5 lists
        assert nav.option_count == 12


async def test_switch_to_list_view(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Groceries")
        await _settle(pilot)
        assert app.panel.view_kind == "list"
        assert app.panel.view_list.title == "Groceries"
        titles = [r.title for r in app.panel.reminders]
        assert "Milk" in titles
        assert "Coffee beans" not in titles  # completed, hidden by default


async def test_toggle_completed_visibility(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Groceries")
        await _settle(pilot)
        await pilot.press("c")
        await _settle(pilot)
        titles = [r.title for r in app.panel.reminders]
        assert "Coffee beans" in titles


async def test_toggle_done_removes_from_active_view(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Groceries")
        await _settle(pilot)
        before = len(app.panel.reminders)
        app.query_one("#reminders", ListView).focus()
        await pilot.press("space")
        await _settle(pilot, 1.0)
        assert len(app.panel.reminders) == before - 1


async def test_add_reminder_via_form(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        await pilot.press("a")
        await pilot.pause(0.3)
        assert isinstance(app.screen, ReminderFormScreen)
        title_input = app.screen.query_one("#f-title", Input)
        title_input.value = "Clean the gutters"
        await pilot.press("enter")  # submit from the title input
        await _settle(pilot, 1.2)
        assert not isinstance(app.screen, ReminderFormScreen)
        assert any(r.title == "Clean the gutters" for r in app.panel.reminders)


async def test_add_requires_title(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        await pilot.press("a")
        await pilot.pause(0.3)
        await pilot.press("enter")
        await pilot.pause(0.3)
        assert isinstance(app.screen, ReminderFormScreen)  # still open
        error = app.screen.query_one("#form-error")
        assert error.has_class("-visible")
        await pilot.press("escape")


async def test_edit_reminder_via_form(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        app.query_one("#reminders", ListView).focus()
        await pilot.press("e")
        await pilot.pause(0.3)
        assert isinstance(app.screen, ReminderFormScreen)
        original = app.screen.reminder.title
        title_input = app.screen.query_one("#f-title", Input)
        title_input.value = original + " — edited"
        await pilot.press("enter")
        await _settle(pilot, 1.2)
        assert any(r.title == original + " — edited" for r in app.panel.reminders)


async def test_delete_with_confirmation(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        before = {r.id for r in app.panel.reminders}
        app.query_one("#reminders", ListView).focus()
        await pilot.press("d")
        await pilot.pause(0.3)
        assert isinstance(app.screen, ConfirmDeleteScreen)
        doomed = app.screen.reminder.id
        await pilot.press("y")
        await _settle(pilot, 1.2)
        assert doomed in before
        assert doomed not in {r.id for r in app.panel.reminders}


async def test_delete_cancel_keeps_reminder(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        before = {r.id for r in app.panel.reminders}
        app.query_one("#reminders", ListView).focus()
        await pilot.press("d")
        await pilot.pause(0.3)
        await pilot.press("n")
        await _settle(pilot)
        assert {r.id for r in app.panel.reminders} == before


async def test_flag_toggle(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        app.query_one("#reminders", ListView).focus()
        target = app.panel._selected_reminder()
        assert not target.flagged
        await pilot.press("f")
        await _settle(pilot, 1.2)
        flagged_now = next(r for r in app.panel.reminders if r.id == target.id)
        assert flagged_now.flagged


async def test_filter_narrows_and_escape_clears(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Groceries")
        await _settle(pilot)
        await pilot.press("slash")
        for ch in "milk":
            await pilot.press(ch)
        await _settle(pilot)
        list_view = app.query_one("#reminders", ListView)
        assert len(list_view.children) == 1
        await pilot.press("escape")
        await _settle(pilot)
        assert app.panel.filter_text == ""
        assert len(list_view.children) > 1
        # Dismissing the filter used to end the app's message loop when the
        # populate worker was cancelled mid-rebuild.
        assert app.is_running


async def test_help_screen_opens_and_closes(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        await pilot.press("question_mark")
        await pilot.pause(0.3)
        assert isinstance(app.screen, HelpScreen)
        await pilot.press("escape")
        await pilot.pause(0.3)
        assert not isinstance(app.screen, HelpScreen)


async def test_smart_view_navigation_via_keyboard(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        app.query_one("#nav", OptionList).focus()
        await pilot.press("j")  # Upcoming
        await _settle(pilot)
        assert app.panel.view_kind == "upcoming"
        await pilot.press("j")  # Overdue
        await _settle(pilot)
        assert app.panel.view_kind == "overdue"
        assert any("passport" in r.title.lower() for r in app.panel.reminders)


async def test_double_enter_in_form_creates_one_reminder(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        before = len(app.panel.reminders)
        await pilot.press("a")
        await pilot.pause(0.3)
        app.screen.query_one("#f-title", Input).value = "Only once"
        await pilot.press("enter")
        await pilot.press("enter")  # double-submit must be a no-op
        await _settle(pilot, 1.5)
        matches = [r for r in app.panel.reminders if r.title == "Only once"]
        assert len(matches) == 1
        assert len(app.panel.reminders) == before + 1


async def test_filter_box_hidden_after_view_switch(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Groceries")
        await _settle(pilot)
        await pilot.press("slash")
        for ch in "milk":
            await pilot.press(ch)
        await _settle(pilot)
        _select_list(app, "Home")
        await _settle(pilot)
        filter_input = app.query_one("#filter", Input)
        assert not filter_input.has_class("-visible")
        assert app.panel.filter_text == ""


async def test_header_counts_fresh_after_mutation(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Groceries")
        await _settle(pilot)
        active_before = app.panel.view_list.active
        app.query_one("#reminders", ListView).focus()
        await pilot.press("space")  # complete one
        await _settle(pilot, 1.5)
        assert app.panel.view_list.active == active_before - 1


async def test_pane_switching_keys(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        nav = app.query_one("#nav", OptionList)
        reminders = app.query_one("#reminders", ListView)
        assert app.focused is nav
        await pilot.press("right")
        assert app.focused is reminders
        await pilot.press("left")
        assert app.focused is nav
        await pilot.press("tab")
        assert app.focused is reminders
        await pilot.press("tab")
        assert app.focused is nav
        await pilot.press("l")
        assert app.focused is reminders
        await pilot.press("h")
        assert app.focused is nav


async def test_tab_toggles_panes_from_start(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        assert isinstance(app.focused, OptionList)
        await pilot.press("tab")
        assert isinstance(app.focused, ListView)
        await pilot.press("tab")
        assert isinstance(app.focused, OptionList)


async def test_app_keys_gated_while_modal_open(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        app.query_one("#reminders", ListView).focus()
        await pilot.press("d")  # open the delete confirmation
        await pilot.pause(0.3)
        assert isinstance(app.screen, ConfirmDeleteScreen)
        # "a" (add reminder) must not stack a form over the confirm.
        await pilot.press("a")
        await pilot.pause(0.3)
        assert len(app.screen_stack) == 2
        assert isinstance(app.screen, ConfirmDeleteScreen)
        await pilot.press("n")  # cancel


async def test_vim_profile_gg_and_paging(vim_app: RemTuiApp):
    async with vim_app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(vim_app, "Personal")
        await _settle(pilot)
        list_view = vim_app.query_one("#reminders", ListView)
        list_view.focus()
        list_view.index = 2
        # Single g is a prefix in vim mode: no jump.
        await pilot.press("g")
        assert list_view.index == 2
        # gg jumps to the top.
        await pilot.press("g")
        assert list_view.index == 0
        # ctrl+d moves the selection (half page down).
        await pilot.press("ctrl+d")
        assert list_view.index > 0


async def test_default_profile_has_no_vim_extras(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Personal")
        await _settle(pilot)
        list_view = app.query_one("#reminders", ListView)
        list_view.focus()
        list_view.index = 2
        await pilot.press("g")  # jumps immediately, no chord
        assert list_view.index == 0
        await pilot.press("ctrl+d")  # vim extra: disabled in default profile
        assert list_view.index == 0
        await pilot.press("o")  # vim extra: no add-reminder modal
        await pilot.pause(0.3)
        assert app.screen is app.screen_stack[0]


async def test_palette_lists_app_commands(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        titles = {c.title for c in app.get_system_commands(app.screen)}
        assert {
            "Add reminder",
            "Refresh",
            "Toggle completed reminders",
            "Keyboard reference",
        } <= titles


def test_check_action_grays_selection_actions_pre_mount(fake_state):
    # Unmounted panel: no selection, so selection actions are grayed (None),
    # and the vim extras are disabled (False) in the default profile. Built
    # directly rather than through an app, since there is no screen to reach
    # the panel through before mounting.
    panel = RemindersPanel(RemctlClient([sys.executable, str(FAKE)]))
    assert panel.check_action("edit_reminder", ()) is None
    assert panel.check_action("toggle_done", ()) is None
    assert panel.check_action("half_page_down", ()) is False
    assert panel.check_action("quit", ()) is True


async def test_edit_form_flag_checkbox_saves_with_other_fields(app: RemTuiApp):
    """The Flagged checkbox must not break the save.

    remctl rejects `edit --flagged` without --private, so routing the flag
    through the edit command used to fail the whole dialog and silently drop
    the title/priority changes alongside it.
    """
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        app.query_one("#reminders", ListView).focus()
        target = app.panel._selected_reminder()
        assert not target.flagged

        await pilot.press("e")
        await pilot.pause(0.3)
        assert isinstance(app.screen, ReminderFormScreen)
        app.screen.query_one("#f-title", Input).value = "Renamed and flagged"
        app.screen.query_one("#f-flag", Checkbox).value = True
        app.screen.query_one("#btn-save", Button).press()
        await _settle(pilot, 1.5)

        assert not isinstance(app.screen, ReminderFormScreen), "save should dismiss"
        saved = next(r for r in app.panel.reminders if r.id == target.id)
        assert saved.title == "Renamed and flagged"
        assert saved.flagged


async def test_edit_form_can_unflag(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Personal")
        await _settle(pilot)
        list_view = app.query_one("#reminders", ListView)
        list_view.focus()
        flagged_index = next(
            i for i, r in enumerate(app.panel.reminders) if r.flagged
        )
        list_view.index = flagged_index
        await pilot.pause(0.2)
        target = app.panel._selected_reminder()
        assert target.flagged

        await pilot.press("e")
        await pilot.pause(0.3)
        app.screen.query_one("#f-flag", Checkbox).value = False
        app.screen.query_one("#btn-save", Button).press()
        await _settle(pilot, 1.5)

        assert not next(r for r in app.panel.reminders if r.id == target.id).flagged


async def test_add_form_surfaces_flag_failure_warning(app: RemTuiApp, monkeypatch):
    """A created-but-unflagged reminder must not look like a clean success."""
    monkeypatch.setenv("REMTUI_FAKE_FLAG_FAILS", "1")
    notifications: list[tuple[str, str | None]] = []
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        original_notify = app.notify
        monkeypatch.setattr(
            app,
            "notify",
            lambda message, **kw: (
                notifications.append((message, kw.get("severity"))),
                original_notify(message, **kw),
            )[1],
        )
        await pilot.press("a")
        await pilot.pause(0.3)
        app.screen.query_one("#f-title", Input).value = "Wanted a flag"
        app.screen.query_one("#f-flag", Checkbox).value = True
        app.screen.query_one("#btn-save", Button).press()
        await _settle(pilot, 1.5)

        added = next(r for r in app.panel.reminders if r.title == "Wanted a flag")
        assert not added.flagged  # the flag write failed
        assert any(
            "flag_not_set" in message and severity == "warning"
            for message, severity in notifications
        ), f"expected a flag warning, got {notifications}"


async def test_q_quits_the_standalone_app(app: RemTuiApp):
    """A widget binding's action resolves against the widget, so the panel's
    `q` has to name the app explicitly or quit silently does nothing."""
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        app.query_one("#reminders", ListView).focus()

        await pilot.press("q")
        await pilot.pause()

        assert not app.is_running


# ── the logo is optional, for embedding ───────────────────────────────────


async def test_logo_shows_by_default(app: RemTuiApp):
    async with app.run_test(size=(120, 40)) as pilot:
        await _settle(pilot, 1.0)
        assert app.panel.query("#logo")


async def test_logo_can_be_dropped_for_a_host(fake_state):
    """Embedded in another TUI, the host's own chrome already names the app,
    so three rows of sidebar are better spent on the lists."""
    from textual.app import App, ComposeResult

    class Host(App):
        def compose(self) -> ComposeResult:
            yield RemindersPanel(
                RemctlClient([sys.executable, str(FAKE)]),
                show_logo=False,
                id="p",
            )

    host = Host()
    async with host.run_test(size=(120, 40)) as pilot:
        await _settle(pilot, 1.0)
        panel = host.query_one("#p", RemindersPanel)

        assert not panel.query("#logo")
        assert panel.query_one("#nav")  # the nav got the rows back


async def test_dropping_the_logo_survives_a_resize(fake_state):
    """_fit_logo runs on every resize and used to query a widget that exists."""
    from textual.app import App, ComposeResult

    class Host(App):
        def compose(self) -> ComposeResult:
            yield RemindersPanel(
                RemctlClient([sys.executable, str(FAKE)]),
                show_logo=False,
                id="p",
            )

    host = Host()
    async with host.run_test(size=(120, 40)) as pilot:
        await _settle(pilot, 1.0)
        # A short terminal is what makes _fit_logo want to hide the logo.
        await pilot.resize_terminal(100, 12)
        await _settle(pilot, 0.5)

        assert host.is_running


# ── dialog contract: shared with projection ───────────────────────────────
#
# Both apps' dialogs follow one shape, so moving between them (or meeting one
# embedded in librarian) does not mean relearning the buttons:
#
#   [secondary…] [Cancel] [Primary]     right-aligned, primary last
#   ^e Editor  esc Cancel  ^s Save      Footer, derived from BINDINGS
#
# with focus starting in the first field, and the safe option focused in a
# destructive confirm.


async def _open_form(app: RemTuiApp, pilot) -> ReminderFormScreen:
    await _settle(pilot, 1.0)
    await pilot.press("a")
    await pilot.pause(0.4)
    assert isinstance(app.screen, ReminderFormScreen)
    return app.screen


async def test_form_button_order_and_ids(app: RemTuiApp):
    async with app.run_test(size=(110, 34)) as pilot:
        modal = await _open_form(app, pilot)

        buttons = [(b.id, str(b.label)) for b in modal.query("Button")]
        assert buttons == [
            ("btn-editor", "Editor"),
            ("btn-cancel", "Cancel"),
            ("btn-save", "Add"),
        ]


async def test_form_buttons_are_right_aligned(app: RemTuiApp):
    async with app.run_test(size=(110, 34)) as pilot:
        modal = await _open_form(app, pilot)

        row = modal.query_one(".form-buttons")
        assert row.styles.align_horizontal == "right"


async def test_form_labels_carry_no_shortcut_text(app: RemTuiApp):
    """The Footer owns the hints, so labels cannot drift from the bindings."""
    async with app.run_test(size=(110, 34)) as pilot:
        modal = await _open_form(app, pilot)

        for button in modal.query("Button"):
            label = str(button.label)
            assert "^" not in label and "Ctrl" not in label and "(" not in label


async def test_form_has_a_footer_listing_its_shortcuts(app: RemTuiApp):
    async with app.run_test(size=(110, 34)) as pilot:
        modal = await _open_form(app, pilot)

        assert modal.query("Footer"), "the dialog should carry its own Footer"

        shown = {
            key: ab.binding.description
            for key, ab in modal.active_bindings.items()
            if ab.binding.show
        }
        assert shown["ctrl+s"] == "Save"
        assert shown["ctrl+e"] == "Editor"
        assert shown["escape"] == "Cancel"


async def test_form_starts_in_the_first_field(app: RemTuiApp):
    async with app.run_test(size=(110, 34)) as pilot:
        await _open_form(app, pilot)

        assert app.focused is not None
        assert app.focused.id == "f-title"


async def test_shortcuts_reach_the_form_from_inside_a_field(app: RemTuiApp):
    """The bug priority=True fixes.

    An Input binds ctrl+e and friends itself, and a focused widget is checked
    before the screen -- so the dialog's shortcuts used to die the moment the
    cursor was in a field, which is where the dialog puts it.
    """
    async with app.run_test(size=(110, 34)) as pilot:
        modal = await _open_form(app, pilot)
        assert isinstance(app.focused, Input)

        for key in ("ctrl+s", "ctrl+e", "escape"):
            binding = modal.active_bindings.get(key)
            assert binding is not None, f"{key} unreachable"
            assert binding.node is modal, f"{key} is being eaten by {binding.node!r}"


async def test_ctrl_e_fires_with_the_cursor_in_a_field(
    app: RemTuiApp, monkeypatch
):
    """Functional half of the above: the key actually dispatches."""
    async with app.run_test(size=(110, 34)) as pilot:
        modal = await _open_form(app, pilot)

        fired = []
        monkeypatch.setattr(
            type(modal), "action_open_in_editor", lambda self: fired.append(1)
        )
        await pilot.press("ctrl+e")
        await pilot.pause(0.3)

        assert fired, "ctrl+e did not reach the dialog"


async def test_tab_walks_fields_then_buttons_ending_on_the_primary(app: RemTuiApp):
    async with app.run_test(size=(110, 34)) as pilot:
        await _open_form(app, pilot)

        seen = []
        for _ in range(20):
            await pilot.press("tab")
            await pilot.pause()
            if app.focused is not None:
                seen.append(app.focused.id)
            if seen[-1:] == ["btn-save"]:
                break

        buttons = [i for i in seen if i and i.startswith("btn-")]
        assert buttons == ["btn-editor", "btn-cancel", "btn-save"], seen


async def test_delete_confirm_focuses_the_safe_option(app: RemTuiApp):
    async with app.run_test(size=(110, 34)) as pilot:
        await _settle(pilot, 1.0)
        app.query_one("#reminders", ListView).focus()
        await _settle(pilot)
        await pilot.press("d")
        await pilot.pause(0.4)

        modal = app.screen
        assert isinstance(modal, ConfirmDeleteScreen)
        buttons = [(b.id, str(b.label)) for b in modal.query("Button")]
        assert buttons == [("btn-cancel", "Cancel"), ("btn-delete", "Delete")]
        assert app.focused.id == "btn-cancel"
        assert modal.query("Footer")
