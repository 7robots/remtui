"""remtui — a Textual TUI for Apple Reminders, powered by remctl."""

from __future__ import annotations

import argparse
import asyncio
import os
import shutil
import sys
from pathlib import Path
from typing import Awaitable, Callable

from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.theme import Theme
from textual.widgets import Footer, Header, Input, ListView, OptionList, Static

from remtui.client import RemctlClient, RemctlError
from remtui.models import Reminder, ReminderList
from remtui.screens import ConfirmDeleteScreen, HelpScreen, ReminderFormScreen
from remtui.widgets import (
    SMART_VIEWS,
    ReminderItem,
    ViewHeader,
    list_option,
    logo,
    nav_header,
    smart_option,
)

REMTUI_THEME = Theme(
    name="remtui",
    primary="#0A84FF",
    secondary="#5E5CE6",
    accent="#FF9F0A",
    warning="#FFD60A",
    error="#FF453A",
    success="#30D158",
    foreground="#F2F2F7",
    background="#1C1C1E",
    surface="#2C2C2E",
    panel="#3A3A3C",
    dark=True,
)

_SMART_BY_KEY = {view.key: view for view in SMART_VIEWS}
_PRIORITY_CYCLE = {"none": "low", "low": "medium", "medium": "high", "high": "none"}


class RemTuiApp(App[None]):
    """Browse, add, edit, complete, and delete Apple Reminders."""

    TITLE = "remtui"
    SUB_TITLE = "Apple Reminders"
    CSS_PATH = "remtui.tcss"

    BINDINGS = [
        Binding("a", "add_reminder", "Add"),
        Binding("e", "edit_reminder", "Edit"),
        Binding("space", "toggle_done", "Done"),
        Binding("d,delete,backspace", "delete_reminder", "Delete"),
        Binding("f", "toggle_flag", "Flag"),
        Binding("p", "cycle_priority", "Priority", show=False),
        Binding("slash", "show_filter", "Filter"),
        Binding("c", "toggle_completed", "Show done", show=False),
        Binding("r", "refresh", "Refresh", show=False),
        Binding("escape", "dismiss_filter", show=False),
        Binding("j", "vim_down", show=False),
        Binding("k", "vim_up", show=False),
        Binding("left,h", "focus_nav", "Lists", show=False),
        Binding("right,l", "focus_reminders", "Reminders", show=False),
        Binding("tab", "toggle_pane", "Switch pane"),
        Binding("g", "go_top", show=False),
        Binding("G", "go_bottom", show=False),
        Binding("question_mark", "help", "Help"),
        Binding("q", "quit", "Quit"),
    ]

    def __init__(self, client: RemctlClient) -> None:
        super().__init__()
        self.client = client
        self.lists: list[ReminderList] = []
        self.reminders: list[Reminder] = []
        self.view_kind: str = "today"
        self.view_list: ReminderList | None = None
        self.show_completed = False
        self.filter_text = ""
        self._current_option_id = ""
        # Serializes ListView rebuilds: the "view" and "populate" worker
        # groups are exclusive only within themselves, so without this two
        # concurrent _populate calls interleave clear()/extend() and
        # duplicate rows.
        self._populate_lock = asyncio.Lock()

    # -- layout -------------------------------------------------------------

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        with Horizontal(id="body"):
            with Vertical(id="sidebar"):
                yield Static(logo(), id="logo")
                yield OptionList(id="nav")
            with Vertical(id="main"):
                yield ViewHeader(id="view-header")
                yield Input(placeholder="filter this view…", id="filter")
                yield ListView(id="reminders")
                yield Static("", id="empty")
        yield Footer()

    def on_mount(self) -> None:
        self.register_theme(REMTUI_THEME)
        self.theme = "remtui"
        self._fit_logo()
        self._build_nav()
        self.query_one("#nav", OptionList).focus()
        self.refresh_lists()

    def on_resize(self) -> None:
        self._fit_logo()

    def _fit_logo(self) -> None:
        # On short terminals the sidebar space belongs to the lists.
        self.query_one("#logo", Static).display = self.size.height >= 20

    # -- sidebar ------------------------------------------------------------

    def _build_nav(self) -> None:
        nav = self.query_one("#nav", OptionList)
        selected_id = self._current_option_id or f"view:{self.view_kind}"
        nav.clear_options()
        nav.add_option(nav_header("Smart Lists"))
        for view in SMART_VIEWS:
            nav.add_option(smart_option(view))
        if self.lists:
            nav.add_option(nav_header(""))
            nav.add_option(nav_header("My Lists"))
            for lst in self.lists:
                nav.add_option(list_option(lst))
        try:
            index = nav.get_option_index(selected_id)
        except Exception:
            index = 1  # first smart view
        nav.highlighted = index

    @work(exclusive=True, group="lists")
    async def refresh_lists(self) -> None:
        try:
            self.lists = await self.client.get_lists()
        except RemctlError as exc:
            self.notify(exc.message, title="remctl", severity="error", timeout=8)
            return
        self._build_nav()
        if self.view_list is not None:
            # Counts in the header come from the list object; swap in the
            # freshly fetched one and re-render, or they go stale after
            # every mutation.
            self.view_list = next(
                (lst for lst in self.lists if lst.id == self.view_list.id),
                self.view_list,
            )
            self.repopulate()

    @on(OptionList.OptionHighlighted, "#nav")
    def _nav_highlighted(self, event: OptionList.OptionHighlighted) -> None:
        option_id = event.option.id
        if not option_id or option_id == self._current_option_id:
            return
        self._current_option_id = option_id
        kind, _, ref = option_id.partition(":")
        if kind == "view":
            self.view_kind = ref
            self.view_list = None
        else:
            self.view_kind = "list"
            self.view_list = next(
                (lst for lst in self.lists if str(lst.id) == ref), None
            )
        self.filter_text = ""
        filter_input = self.query_one("#filter", Input)
        filter_input.value = ""
        filter_input.remove_class("-visible")
        self.load_view(show_loading=True)

    @on(OptionList.OptionSelected, "#nav")
    def _nav_selected(self) -> None:
        self.query_one("#reminders", ListView).focus()

    # -- data loading ---------------------------------------------------------

    @work(exclusive=True, group="view")
    async def load_view(self, show_loading: bool = False) -> None:
        list_view = self.query_one("#reminders", ListView)
        if show_loading:
            list_view.loading = True
        try:
            if self.view_kind == "list" and self.view_list is not None:
                items = await self.client.get_reminders(
                    self.view_list.title, include_completed=self.show_completed
                )
            elif self.view_kind == "today":
                items = await self.client.today()
            elif self.view_kind == "upcoming":
                items = await self.client.upcoming(7)
            elif self.view_kind == "overdue":
                items = await self.client.overdue()
            elif self.view_kind == "flagged":
                items = await self.client.flagged()
            else:
                items = []
        except RemctlError as exc:
            self.notify(exc.message, title="remctl", severity="error", timeout=8)
            items = []
        finally:
            list_view.loading = False
        self.reminders = items
        await self._populate()

    async def _populate(self) -> None:
        async with self._populate_lock:
            list_view = self.query_one("#reminders", ListView)
            shown = [
                r for r in self.reminders
                if not self.filter_text or r.matches(self.filter_text)
            ]
            previous_id = self._selected_reminder_id()
            previous_index = list_view.index or 0
            await list_view.clear()
            await list_view.extend(ReminderItem(r) for r in shown)
            if shown:
                # Reselect the same reminder; if it's gone (completed or
                # deleted), stay near its old position instead of jumping
                # to the top.
                index = next(
                    (i for i, r in enumerate(shown) if r.id == previous_id),
                    min(previous_index, len(shown) - 1),
                )
                list_view.index = index
            self._update_header(len(shown))
            self._update_empty(bool(shown))

    def _update_header(self, shown: int) -> None:
        header = self.query_one("#view-header", ViewHeader)
        if self.view_kind == "list" and self.view_list is not None:
            lst = self.view_list
            icon = lst.emoji or "●"
            header.show_view(
                label=lst.title,
                icon=icon,
                color=lst.color_hex,
                shown=shown,
                active=lst.active,
                completed=lst.completed,
                filter_text=self.filter_text,
            )
        else:
            view = _SMART_BY_KEY.get(self.view_kind, SMART_VIEWS[0])
            header.show_view(
                label=view.label,
                icon=view.icon,
                color=view.color,
                shown=shown,
                filter_text=self.filter_text,
            )

    def _update_empty(self, has_items: bool) -> None:
        empty = self.query_one("#empty", Static)
        if has_items:
            empty.remove_class("-visible")
            return
        if self.filter_text:
            message = f'○  nothing matches "{self.filter_text}"'
        elif self.view_kind == "list":
            message = "○  no reminders here — press a to add one"
        else:
            view = _SMART_BY_KEY.get(self.view_kind)
            message = f"✓  {view.empty}" if view else "✓  nothing here"
        empty.update(message)
        empty.add_class("-visible")

    # -- selection helpers ----------------------------------------------------

    def _selected_reminder(self) -> Reminder | None:
        item = self.query_one("#reminders", ListView).highlighted_child
        if isinstance(item, ReminderItem):
            return item.reminder
        return None

    def _selected_reminder_id(self) -> int | None:
        reminder = self._selected_reminder()
        return reminder.id if reminder else None

    def _default_list_title(self) -> str:
        if self.view_kind == "list" and self.view_list is not None:
            return self.view_list.title
        return self.lists[0].title if self.lists else ""

    # -- mutations --------------------------------------------------------------

    @work(group="mutate")
    async def _mutate(
        self, action: Callable[[], Awaitable[object]], message: str
    ) -> None:
        try:
            await action()
        except RemctlError as exc:
            self.notify(exc.message, title="remctl", severity="error", timeout=8)
            return
        if message:
            self.notify(message, timeout=3)
        self.load_view()
        self.refresh_lists()

    def action_toggle_done(self) -> None:
        reminder = self._selected_reminder()
        if reminder is None:
            return
        if reminder.completed:
            self._mutate(
                lambda: self.client.undone(reminder.id),
                f"↺ Reopened “{reminder.title}”",
            )
        else:
            self._mutate(
                lambda: self.client.done(reminder.id),
                f"✓ Completed “{reminder.title}”",
            )

    def action_toggle_flag(self) -> None:
        reminder = self._selected_reminder()
        if reminder is None:
            return
        if reminder.flagged:
            self._mutate(lambda: self.client.unflag(reminder.id), "")
        else:
            self._mutate(lambda: self.client.flag(reminder.id), "")

    def action_cycle_priority(self) -> None:
        reminder = self._selected_reminder()
        if reminder is None:
            return
        new_priority = _PRIORITY_CYCLE[reminder.priority]
        self._mutate(
            lambda: self.client.edit(reminder.id, priority=new_priority),
            f"Priority → {new_priority}",
        )

    def action_delete_reminder(self) -> None:
        reminder = self._selected_reminder()
        if reminder is None:
            return

        def on_confirm(confirmed: bool | None) -> None:
            if confirmed:
                self._mutate(
                    lambda: self.client.delete(reminder.id),
                    f"🗑 Deleted “{reminder.title}”",
                )

        self.push_screen(ConfirmDeleteScreen(reminder), on_confirm)

    def action_add_reminder(self) -> None:
        if not self.lists:
            self.notify("No lists loaded yet.", severity="warning")
            return

        def on_close(saved: bool | None) -> None:
            if saved:
                self.notify("＋ Reminder added", timeout=3)
                self.load_view()
                self.refresh_lists()

        self.push_screen(
            ReminderFormScreen(
                self.client, self.lists, default_list=self._default_list_title()
            ),
            on_close,
        )

    def action_edit_reminder(self) -> None:
        reminder = self._selected_reminder()
        if reminder is None:
            return

        def on_close(saved: bool | None) -> None:
            if saved:
                self.notify("✎ Reminder updated", timeout=3)
                self.load_view()
                self.refresh_lists()

        self.push_screen(
            ReminderFormScreen(self.client, self.lists, reminder=reminder),
            on_close,
        )

    @on(ListView.Selected, "#reminders")
    def _reminder_selected(self) -> None:
        self.action_edit_reminder()

    # -- filtering ---------------------------------------------------------------

    def action_show_filter(self) -> None:
        filter_input = self.query_one("#filter", Input)
        filter_input.add_class("-visible")
        filter_input.focus()

    def action_dismiss_filter(self) -> None:
        filter_input = self.query_one("#filter", Input)
        if self.filter_text or filter_input.has_class("-visible"):
            filter_input.value = ""
            filter_input.remove_class("-visible")
            self.filter_text = ""
            self.repopulate()
            self.query_one("#reminders", ListView).focus()

    @on(Input.Changed, "#filter")
    def _filter_changed(self, event: Input.Changed) -> None:
        self.filter_text = event.value.strip()
        self.repopulate()

    @on(Input.Submitted, "#filter")
    def _filter_submitted(self) -> None:
        self.query_one("#reminders", ListView).focus()

    @work(exclusive=True, group="populate")
    async def repopulate(self) -> None:
        await self._populate()

    # -- misc actions --------------------------------------------------------------

    def action_toggle_completed(self) -> None:
        self.show_completed = not self.show_completed
        state = "shown" if self.show_completed else "hidden"
        self.notify(f"Completed reminders {state} (list views)", timeout=3)
        self.load_view(show_loading=True)

    def action_refresh(self) -> None:
        self.load_view()
        self.refresh_lists()

    def action_help(self) -> None:
        self.push_screen(HelpScreen())

    def action_vim_down(self) -> None:
        self._vim_move(1)

    def action_vim_up(self) -> None:
        self._vim_move(-1)

    def _vim_move(self, delta: int) -> None:
        focused = self.focused
        if isinstance(focused, (ListView, OptionList)):
            if delta > 0:
                focused.action_cursor_down()
            else:
                focused.action_cursor_up()

    def action_focus_nav(self) -> None:
        self.query_one("#nav", OptionList).focus()

    def action_focus_reminders(self) -> None:
        self.query_one("#reminders", ListView).focus()

    def action_toggle_pane(self) -> None:
        if isinstance(self.focused, OptionList):
            self.action_focus_reminders()
        else:
            self.action_focus_nav()

    def action_go_top(self) -> None:
        focused = self.focused
        if isinstance(focused, ListView) and len(focused) > 0:
            focused.index = 0
        elif isinstance(focused, OptionList):
            focused.action_first()

    def action_go_bottom(self) -> None:
        focused = self.focused
        if isinstance(focused, ListView) and len(focused) > 0:
            focused.index = len(focused) - 1
        elif isinstance(focused, OptionList):
            focused.action_last()


def build_client(argv: list[str] | None = None) -> RemctlClient:
    parser = argparse.ArgumentParser(
        prog="remtui",
        description="A Textual TUI for Apple Reminders, powered by remctl.",
    )
    parser.add_argument(
        "--demo",
        action="store_true",
        help="run against a bundled fake reminders store (no remctl needed)",
    )
    parser.add_argument(
        "--remctl",
        metavar="PATH",
        help="path to the remctl binary (default: $REMTUI_REMCTL or 'remctl' on PATH)",
    )
    args = parser.parse_args(argv)

    if args.demo:
        fake = Path(__file__).parent / "fake_remctl.py"
        return RemctlClient([sys.executable, str(fake)])

    binary = args.remctl or os.environ.get("REMTUI_REMCTL", "remctl")
    if shutil.which(binary) is None:
        sys.exit(
            f"remtui: '{binary}' not found on PATH.\n"
            "Install remctl (https://github.com/viticci/remctl) and run "
            "'remctl onboard', or try 'remtui --demo'."
        )
    return RemctlClient(binary)


def main() -> None:
    RemTuiApp(build_client()).run()


if __name__ == "__main__":
    main()
