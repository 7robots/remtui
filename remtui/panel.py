"""The reminders panel: remtui's UI as an embeddable widget."""

from __future__ import annotations

import asyncio
import time
from typing import Awaitable, Callable

from textual import on, work
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.widgets import Input, ListView, OptionList, Static

from remtui.client import RemctlClient, RemctlError
from remtui.models import Reminder, ReminderList
from remtui.screens import ConfirmDeleteScreen, HelpScreen, ReminderFormScreen
from remtui.widgets import (
    SMART_VIEWS,
    ReminderItem,
    ReminderListView,
    ViewHeader,
    list_option,
    logo,
    nav_header,
    smart_option,
)

_SMART_BY_KEY = {view.key: view for view in SMART_VIEWS}
_PRIORITY_CYCLE = {"none": "low", "low": "medium", "medium": "high", "high": "none"}


class RemindersPanel(Vertical):
    """The reminders UI: sidebar of views and lists, plus the reminder list.

    A widget rather than a Screen or an App, so it can be composed anywhere --
    remtui's own screen wraps it in a Header and Footer, and librarian mounts it
    in a modal over its right-hand panels. Styles live in DEFAULT_CSS, which
    Textual scopes to this widget, so hosting it cannot restyle the host.
    """

    DEFAULT_CSS = """
/* ── layout ─────────────────────────────────────────────────────────── */

/* The panel fills whatever hosts it. Without an explicit height it sizes to
   its content, and then hiding the logo on short terminals shrinks the panel,
   which re-fires on_resize and flip-flops. */
RemindersPanel {
    width: 100%;
    height: 1fr;
}

#body {
    height: 1fr;
}

#sidebar {
    width: 30;
    min-width: 22;
    max-width: 40;
    background: $surface;
    border-right: vkey $panel;
    padding: 1 0 0 0;
}

#logo {
    height: auto;
    padding: 0 1 1 1;
    text-align: center;
}

#nav {
    background: transparent;
    border: none;
    padding: 0 1;
    scrollbar-size-vertical: 1;
}

#nav:focus {
    border: none;
    background: transparent;
}

#main {
    background: $background;
}

/* ── reminder list ──────────────────────────────────────────────────── */

#reminders {
    background: transparent;
    padding: 0 1 1 1;
    scrollbar-size-vertical: 1;
}

ReminderItem {
    layout: horizontal;
    height: auto;
    padding: 0 1;
}

/* The app stylesheet overrides ListView's built-in cursor styling, so the
   selection states are (re)defined here: a visible gray bar when the pane
   is unfocused, a primary tint when it has focus. */
#reminders > ReminderItem.-highlight {
    background: $panel;
}

#reminders:focus > ReminderItem.-highlight {
    background: $primary 35%;
}

#reminders > ReminderItem.-hovered {
    background: $boost;
}

ReminderItem .check {
    width: 3;
}

ReminderItem .body {
    width: 1fr;
    height: auto;
}

ReminderItem .meta {
    margin-left: 2;
}

ReminderItem.-done {
    opacity: 55%;
}

#filter {
    display: none;
    margin: 0 2;
    border: tall $primary;
}

#filter.-visible {
    display: block;
}

#empty {
    display: none;
    height: 1fr;
    content-align: center middle;
    color: $text-muted;
    text-style: italic;
}

#empty.-visible {
    display: block;
}
    """

    BINDINGS = [
        Binding("a,n", "add_reminder", "Add", id="reminder.add"),
        Binding("e", "edit_reminder", "Edit", id="reminder.edit"),
        Binding("space", "toggle_done", "Done", id="reminder.done"),
        Binding("d,delete,backspace", "delete_reminder", "Delete", id="reminder.delete"),
        Binding("f", "toggle_flag", "Flag", id="reminder.flag"),
        Binding("p", "cycle_priority", "Priority", show=False, id="reminder.priority"),
        Binding("slash", "show_filter", "Filter", id="view.filter"),
        Binding("c", "toggle_completed", "Show done", show=False, id="view.show-completed"),
        Binding("r", "refresh", "Refresh", show=False, id="view.refresh"),
        Binding("escape", "dismiss_filter", show=False, id="view.dismiss-filter"),
        Binding("j", "vim_down", show=False, id="nav.down"),
        Binding("k", "vim_up", show=False, id="nav.up"),
        Binding("left,h", "focus_nav", "Lists", show=False, id="nav.left"),
        Binding("right,l", "focus_reminders", "Reminders", show=False, id="nav.right"),
        # priority so it beats the Screen's built-in tab → focus_next binding
        Binding(
            "tab", "toggle_pane", "Switch pane", priority=True, id="nav.switch-pane"
        ),
        Binding("g", "go_top", show=False, id="nav.top"),
        Binding("G", "go_bottom", show=False, id="nav.bottom"),
        Binding("question_mark", "help", "Help", id="app.help"),
        # `app.quit`, not `quit`: a widget binding's action resolves against
        # the widget, and the panel has no action_quit, so a bare `quit` here
        # silently does nothing. Namespacing it targets the app that hosts the
        # panel -- which a host can override with a priority binding of its own.
        Binding("q", "app.quit", "Quit", id="app.quit"),
        # vim profile extras — inert unless --vim / REMTUI_KEYS=vim
        Binding("ctrl+d", "half_page_down", "½ page down", show=False, id="vim.half-down"),
        Binding("ctrl+u", "half_page_up", "½ page up", show=False, id="vim.half-up"),
        Binding("ctrl+f", "cursor_page_down", "Page down", show=False, id="vim.page-down"),
        Binding("ctrl+b", "cursor_page_up", "Page up", show=False, id="vim.page-up"),
        Binding("colon", "vim_palette", "Palette", show=False, id="vim.palette"),
        Binding("o", "vim_new", "New", show=False, id="vim.new"),
    ]

    # Actions that only exist in the vim key profile.
    _VIM_ACTIONS = frozenset(
        {"half_page_down", "half_page_up", "cursor_page_down", "cursor_page_up",
         "vim_palette", "vim_new"}
    )
    # Actions that need a selected reminder (grayed in the footer without one).
    _SELECTION_ACTIONS = frozenset(
        {"edit_reminder", "toggle_done", "delete_reminder", "toggle_flag",
         "cycle_priority"}
    )
    # Actions that must not fire while a modal is open (app bindings stay
    # live under modal screens for any key the modal doesn't consume).
    _MAIN_SCREEN_ACTIONS = _VIM_ACTIONS | _SELECTION_ACTIONS | frozenset(
        {"add_reminder", "show_filter", "toggle_completed", "refresh",
         "toggle_pane", "go_top", "go_bottom", "focus_nav", "focus_reminders",
         "vim_down", "vim_up"}
    )

    # Seconds within which a second `g` completes the gg chord (vim profile).
    _GG_CHORD_SECONDS = 0.75

    def __init__(
        self,
        client: RemctlClient,
        vim: bool = False,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.client = client
        self.lists: list[ReminderList] = []
        self.reminders: list[Reminder] = []
        self.view_kind: str = "today"
        self.view_list: ReminderList | None = None
        self.show_completed = False
        self.filter_text = ""
        self._current_option_id = ""
        self._vim = vim
        self._last_g = 0.0  # monotonic time of the last pending `g` press
        # Serializes ListView rebuilds: the "view" and "populate" worker
        # groups are exclusive only within themselves, so without this two
        # concurrent _populate calls interleave clear()/extend() and
        # duplicate rows.
        self._populate_lock = asyncio.Lock()

    # -- layout -------------------------------------------------------------

    def compose(self) -> ComposeResult:
        with Horizontal(id="body"):
            with Vertical(id="sidebar"):
                yield Static(logo(), id="logo")
                yield OptionList(id="nav")
            with Vertical(id="main"):
                yield ViewHeader(id="view-header")
                yield Input(placeholder="filter this view…", id="filter")
                yield ReminderListView(id="reminders")
                yield Static("", id="empty")

    def on_mount(self) -> None:
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
            # Selection may have changed; let the footer re-evaluate its
            # grayed-out (selection-dependent) states.
            self.refresh_bindings()

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
        try:
            item = self.query_one("#reminders", ListView).highlighted_child
        except Exception:
            # check_action can run before the widget tree is mounted.
            return None
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

        self.app.push_screen(ConfirmDeleteScreen(reminder), on_confirm)

    def action_add_reminder(self) -> None:
        if not self.lists:
            self.notify("No lists loaded yet.", severity="warning")
            return

        def on_close(saved: bool | None) -> None:
            if saved:
                self.notify("＋ Reminder added", timeout=3)
                self.load_view()
                self.refresh_lists()

        self.app.push_screen(
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

        self.app.push_screen(
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

    # Deliberately not exclusive. Cancelling a populate worker part-way
    # through rebuilding the ListView ends the app's message loop -- silently,
    # with no exception and no exit() call -- which showed up as escape after
    # filtering killing the app once this worker belonged to the panel rather
    # than to the App. _populate_lock already serializes rebuilds, so
    # cancellation bought nothing: the queued runs just settle in order.
    @work(group="populate")
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
        self.app.push_screen(HelpScreen())

    def check_action(self, action: str, parameters) -> bool | None:
        if action in self._VIM_ACTIONS and not self._vim:
            return False
        if action in self._MAIN_SCREEN_ACTIONS and self._modal_is_open():
            return False
        if action in self._SELECTION_ACTIONS and self._selected_reminder() is None:
            return None  # disabled, shown grayed in the footer
        return True

    def _modal_is_open(self) -> bool:
        """Whether a screen is stacked above the one holding this panel.

        Answerable before mounting, where there is no app to ask.
        """
        if not self.is_mounted:
            return False
        return self.app.screen is not self.screen

    def action_vim_down(self) -> None:
        self._vim_move(1)

    def action_vim_up(self) -> None:
        self._vim_move(-1)

    def _vim_move(self, delta: int) -> None:
        focused = self.app.focused
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
        if isinstance(self.app.focused, OptionList):
            self.action_focus_reminders()
        else:
            self.action_focus_nav()

    def action_go_top(self) -> None:
        # In the vim profile `g` is a prefix: only the gg chord jumps.
        if self._vim:
            now = time.monotonic()
            if now - self._last_g > self._GG_CHORD_SECONDS:
                self._last_g = now
                return
            self._last_g = 0.0
        focused = self.app.focused
        if isinstance(focused, ListView) and len(focused) > 0:
            focused.index = 0
        elif isinstance(focused, OptionList):
            focused.action_first()

    def action_go_bottom(self) -> None:
        focused = self.app.focused
        if isinstance(focused, ListView) and len(focused) > 0:
            focused.index = len(focused) - 1
        elif isinstance(focused, OptionList):
            focused.action_last()

    # -- vim profile extras ---------------------------------------------------

    def action_half_page_down(self) -> None:
        self._cursor_page(1, 0.5)

    def action_half_page_up(self) -> None:
        self._cursor_page(-1, 0.5)

    def action_cursor_page_down(self) -> None:
        self._cursor_page(1, 1.0)

    def action_cursor_page_up(self) -> None:
        self._cursor_page(-1, 1.0)

    def _cursor_page(self, direction: int, fraction: float) -> None:
        focused = self.app.focused
        if isinstance(focused, ReminderListView):
            focused.cursor_page(direction, fraction)
        elif isinstance(focused, OptionList):
            # The sidebar is short; half and full pages both map to a page
            # (looping cursor_down would wrap past the ends).
            if direction > 0:
                focused.action_page_down()
            else:
                focused.action_page_up()

    def action_vim_palette(self) -> None:
        self.action_command_palette()

    def action_vim_new(self) -> None:
        self.action_add_reminder()


