"""End-to-end TUI tests: pilot-driven app against the fake remctl backend."""

import sys

import pytest
from textual.widgets import Input, ListView, OptionList

from remtui.app import RemTuiApp
from remtui.client import RemctlClient
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
    lst = next(lst for lst in app.lists if lst.title == title)
    nav.highlighted = nav.get_option_index(f"list:{lst.id}")


async def test_startup_shows_today_and_lists(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        assert app.view_kind == "today"
        assert len(app.lists) == 5
        assert app.reminders, "today view should have seeded reminders"
        nav = app.query_one("#nav", OptionList)
        # 2 headers + 1 spacer + 4 smart views + 5 lists
        assert nav.option_count == 12


async def test_switch_to_list_view(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Groceries")
        await _settle(pilot)
        assert app.view_kind == "list"
        assert app.view_list.title == "Groceries"
        titles = [r.title for r in app.reminders]
        assert "Milk" in titles
        assert "Coffee beans" not in titles  # completed, hidden by default


async def test_toggle_completed_visibility(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Groceries")
        await _settle(pilot)
        await pilot.press("c")
        await _settle(pilot)
        titles = [r.title for r in app.reminders]
        assert "Coffee beans" in titles


async def test_toggle_done_removes_from_active_view(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Groceries")
        await _settle(pilot)
        before = len(app.reminders)
        app.query_one("#reminders", ListView).focus()
        await pilot.press("space")
        await _settle(pilot, 1.0)
        assert len(app.reminders) == before - 1


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
        assert any(r.title == "Clean the gutters" for r in app.reminders)


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
        assert any(r.title == original + " — edited" for r in app.reminders)


async def test_delete_with_confirmation(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        before = {r.id for r in app.reminders}
        app.query_one("#reminders", ListView).focus()
        await pilot.press("d")
        await pilot.pause(0.3)
        assert isinstance(app.screen, ConfirmDeleteScreen)
        doomed = app.screen.reminder.id
        await pilot.press("y")
        await _settle(pilot, 1.2)
        assert doomed in before
        assert doomed not in {r.id for r in app.reminders}


async def test_delete_cancel_keeps_reminder(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        before = {r.id for r in app.reminders}
        app.query_one("#reminders", ListView).focus()
        await pilot.press("d")
        await pilot.pause(0.3)
        await pilot.press("n")
        await _settle(pilot)
        assert {r.id for r in app.reminders} == before


async def test_flag_toggle(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        app.query_one("#reminders", ListView).focus()
        target = app._selected_reminder()
        assert not target.flagged
        await pilot.press("f")
        await _settle(pilot, 1.2)
        flagged_now = next(r for r in app.reminders if r.id == target.id)
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
        assert app.filter_text == ""
        assert len(list_view.children) > 1


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
        assert app.view_kind == "upcoming"
        await pilot.press("j")  # Overdue
        await _settle(pilot)
        assert app.view_kind == "overdue"
        assert any("passport" in r.title.lower() for r in app.reminders)


async def test_double_enter_in_form_creates_one_reminder(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Home")
        await _settle(pilot)
        before = len(app.reminders)
        await pilot.press("a")
        await pilot.pause(0.3)
        app.screen.query_one("#f-title", Input).value = "Only once"
        await pilot.press("enter")
        await pilot.press("enter")  # double-submit must be a no-op
        await _settle(pilot, 1.5)
        matches = [r for r in app.reminders if r.title == "Only once"]
        assert len(matches) == 1
        assert len(app.reminders) == before + 1


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
        assert app.filter_text == ""


async def test_header_counts_fresh_after_mutation(app: RemTuiApp):
    async with app.run_test(size=(120, 36)) as pilot:
        await _settle(pilot, 1.0)
        _select_list(app, "Groceries")
        await _settle(pilot)
        active_before = app.view_list.active
        app.query_one("#reminders", ListView).focus()
        await pilot.press("space")  # complete one
        await _settle(pilot, 1.5)
        assert app.view_list.active == active_before - 1


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
    # Unmounted app: no selection, so selection actions are grayed (None),
    # and the vim extras are disabled (False) in the default profile.
    app = RemTuiApp(RemctlClient([sys.executable, str(FAKE)]))
    assert app.check_action("edit_reminder", ()) is None
    assert app.check_action("toggle_done", ()) is None
    assert app.check_action("half_page_down", ()) is False
    assert app.check_action("quit", ()) is True
