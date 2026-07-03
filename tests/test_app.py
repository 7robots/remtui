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
