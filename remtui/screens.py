"""Modal screens: add/edit form, delete confirmation, help."""

from __future__ import annotations

import os
import subprocess
import tempfile

from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import (
    Button,
    Checkbox,
    Footer,
    Input,
    Select,
    Static,
    TextArea,
)

from remtui.client import RemctlClient, RemctlError, warnings_of
from remtui.models import Reminder, ReminderList

PRIORITY_OPTIONS = [
    ("None", "none"),
    ("! Low", "low"),
    ("!! Medium", "medium"),
    ("!!! High", "high"),
]

DUE_PLACEHOLDER = "tomorrow 09:30 · +3d · none"


def _prefill_due(reminder: Reminder) -> str:
    """Render the current due date in a form remctl can parse back."""
    due = reminder.due
    if due is None:
        return ""
    if reminder.all_day or (due.hour, due.minute) == (0, 0):
        return due.strftime("%Y-%m-%d")
    return due.strftime("%Y-%m-%d %H:%M")


class ReminderFormScreen(ModalScreen[bool]):
    """Add or edit a reminder. Dismisses with True after a successful save."""

    # The dialog styles travel with the dialogs, so the panel keeps its look
    # when another app hosts it.
    CSS_PATH = "dialogs.tcss"

    # priority=True on all three: an Input or TextArea with focus binds several
    # of these itself (ctrl+e, ctrl+k...), and a focused widget is checked
    # before the screen -- so without priority the dialog's own shortcuts stop
    # working the moment the cursor is in a field, which is the whole time. It
    # also keeps the Footer honest, since it lists the reachable bindings.
    BINDINGS = [
        Binding("escape", "cancel", "Cancel", priority=True),
        Binding("ctrl+s", "save", "Save", priority=True),
        Binding("ctrl+e", "open_in_editor", "Editor", priority=True),
    ]

    def __init__(
        self,
        client: RemctlClient,
        lists: list[ReminderList],
        reminder: Reminder | None = None,
        default_list: str = "",
    ) -> None:
        super().__init__()
        self.client = client
        self.lists = lists
        self.reminder = reminder
        self.default_list = default_list
        self._saving = False

    @property
    def is_edit(self) -> bool:
        return self.reminder is not None

    def compose(self) -> ComposeResult:
        r = self.reminder
        list_options = [(lst.title, lst.title) for lst in self.lists]
        current_list = r.list_name if r else self.default_list
        if current_list and current_list not in {t for _, t in list_options}:
            list_options.insert(0, (current_list, current_list))
        def titled(widget, title: str):
            widget.border_title = title
            return widget

        dialog = VerticalScroll(id="dialog", classes="form-dialog")
        dialog.border_title = "✎ Edit Reminder" if self.is_edit else "＋ New Reminder"
        with dialog:
            yield titled(
                Input(
                    value=r.title if r else "",
                    placeholder="What needs doing?",
                    id="f-title",
                ),
                "Title",
            )
            yield titled(TextArea(r.notes if r else "", id="f-notes"), "Notes")
            with Horizontal(classes="form-row"):
                yield titled(
                    Input(
                        value=_prefill_due(r) if r else "",
                        placeholder=DUE_PLACEHOLDER,
                        id="f-due",
                    ),
                    "Due",
                )
                yield titled(
                    Select(
                        PRIORITY_OPTIONS,
                        value=r.priority if r else "none",
                        allow_blank=False,
                        id="f-priority",
                    ),
                    "Priority",
                )
            with Horizontal(classes="form-row"):
                yield titled(
                    Select(
                        list_options,
                        value=current_list if current_list else Select.BLANK,
                        allow_blank=not current_list,
                        id="f-list",
                    ),
                    "List",
                )
                yield Checkbox("Flagged ⚑", value=r.flagged if r else False, id="f-flag")
            yield Static("", id="form-error")
            # Secondary actions first, then Cancel, then the primary -- the
            # shape every dialog in both apps uses. Shortcuts are not repeated
            # in the labels; the Footer below derives them from BINDINGS, so
            # they cannot drift.
            with Horizontal(classes="form-buttons"):
                yield Button("Editor", id="btn-editor")
                yield Button("Cancel", id="btn-cancel")
                yield Button(
                    "Save" if self.is_edit else "Add",
                    variant="primary",
                    id="btn-save",
                )
            yield Footer()

    def on_mount(self) -> None:
        self.query_one("#f-title", Input).focus()

    def action_cancel(self) -> None:
        if self._saving:
            # The write subprocess may already have committed; aborting the
            # dialog now would leave the app unaware of it.
            return
        self.dismiss(False)

    def action_save(self) -> None:
        # A plain guard, not an exclusive worker: exclusivity would cancel
        # the in-flight save (whose subprocess still commits) and run a
        # second one — duplicating the reminder.
        if self._saving:
            return
        self.run_worker(self._save(), group="save")

    def action_open_in_editor(self) -> None:
        """Edit the notes field in an external editor ($EDITOR, default vim)."""
        text_area = self.query_one("#f-notes", TextArea)
        with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
            f.write(text_area.text)
            temp_path = f.name
        try:
            editor = os.environ.get("EDITOR", "vim")
            with self.app.suspend():
                subprocess.run([editor, temp_path], check=True)
            with open(temp_path) as f:
                edited = f.read()
            text_area.clear()
            text_area.insert(edited)
        finally:
            if os.path.exists(temp_path):
                os.unlink(temp_path)

    @on(Button.Pressed, "#btn-editor")
    def _editor_pressed(self) -> None:
        self.action_open_in_editor()

    @on(Button.Pressed, "#btn-cancel")
    def _cancel_pressed(self) -> None:
        self.dismiss(False)

    @on(Button.Pressed, "#btn-save")
    def _save_pressed(self) -> None:
        self.action_save()

    @on(Input.Submitted)
    def _submitted(self) -> None:
        self.action_save()

    def _show_error(self, message: str) -> None:
        error = self.query_one("#form-error", Static)
        error.update(f"⚠ {message}")
        error.add_class("-visible")

    async def _save(self) -> None:
        title = self.query_one("#f-title", Input).value.strip()
        notes = self.query_one("#f-notes", TextArea).text.rstrip()
        due = self.query_one("#f-due", Input).value.strip()
        priority = self.query_one("#f-priority", Select).value
        list_value = self.query_one("#f-list", Select).value
        list_title = "" if list_value is Select.BLANK else str(list_value)
        flagged = self.query_one("#f-flag", Checkbox).value

        if not title:
            self._show_error("A title is required.")
            self.query_one("#f-title", Input).focus()
            return

        save_button = self.query_one("#btn-save", Button)
        save_button.disabled = True
        self._saving = True
        try:
            if self.is_edit:
                result = await self._save_edit(
                    title, notes, due, str(priority), list_title, flagged
                )
            else:
                result = await self.client.add(
                    title,
                    list_title=list_title,
                    notes=notes,
                    due=due,
                    priority=str(priority),
                    flagged=flagged,
                )
        except RemctlError as exc:
            self._show_error(exc.message)
            save_button.disabled = False
            self._saving = False
            return
        # The write succeeded but remctl reported a partial failure (e.g. the
        # reminder was created and only the flag write failed). Notify on the
        # app, not this screen: it is about to be dismissed.
        for warning in warnings_of(result):
            self.app.notify(warning, title="remctl", severity="warning", timeout=8)
        self.dismiss(True)

    async def _save_edit(
        self,
        title: str,
        notes: str,
        due: str,
        priority: str,
        list_title: str,
        flagged: bool,
    ) -> object:
        """Send only the fields that changed."""
        r = self.reminder
        assert r is not None
        kwargs: dict = {}
        if title != r.title:
            kwargs["title"] = title
        if notes != r.notes.rstrip():
            kwargs["notes"] = notes
        if due != _prefill_due(r):
            kwargs["due"] = due  # empty string -> clear
        if priority != r.priority:
            kwargs["priority"] = priority
        if list_title and list_title != r.list_name:
            kwargs["list_title"] = list_title
        if flagged != r.flagged:
            kwargs["flagged"] = flagged
        if not kwargs:
            return None
        return await self.client.edit(r.id, **kwargs)


class ConfirmDeleteScreen(ModalScreen[bool]):
    """Are-you-sure dialog for deletion."""

    # The dialog styles travel with the dialogs, so the panel keeps its look
    # when another app hosts it.
    CSS_PATH = "dialogs.tcss"

    BINDINGS = [
        Binding("escape,n", "cancel", "Cancel"),
        Binding("y", "confirm", "Delete"),
    ]

    def __init__(self, reminder: Reminder) -> None:
        super().__init__()
        self.reminder = reminder

    def compose(self) -> ComposeResult:
        with Vertical(id="dialog", classes="confirm-dialog"):
            yield Static("🗑  Delete Reminder", id="confirm-title")
            yield Static(
                f"Delete “{self.reminder.title}” from {self.reminder.list_name}?\n"
                "This cannot be undone.",
                id="confirm-message",
            )
            with Horizontal(classes="form-buttons"):
                yield Button("Cancel", id="btn-cancel")
                yield Button("Delete", variant="error", id="btn-delete")
            yield Footer()

    def on_mount(self) -> None:
        # Focus the safe option: a stray Enter right after pressing "d"
        # must not delete irreversibly.
        self.query_one("#btn-cancel", Button).focus()

    def action_cancel(self) -> None:
        self.dismiss(False)

    def action_confirm(self) -> None:
        self.dismiss(True)

    @on(Button.Pressed, "#btn-cancel")
    def _cancel_pressed(self) -> None:
        self.dismiss(False)

    @on(Button.Pressed, "#btn-delete")
    def _delete_pressed(self) -> None:
        self.dismiss(True)


HELP_TEXT = """\
[bold $accent]Navigate[/]
  j / ↓, k / ↑        move down / up
  ← / h, → / l        focus sidebar / reminders
  tab                 switch pane
  g, G                jump to top / bottom

[bold $accent]Reminders[/]
  a                   add a reminder
  e / enter           edit selected
  space               toggle done
  d / ⌫               delete (asks first)
  f                   toggle flag ⚑
  p                   cycle priority

[bold $accent]Views[/]
  /                   filter current view
  esc                 clear filter
  c                   show/hide completed (list views)
  r                   refresh

[bold $accent]Form (add / edit)[/]
  ctrl+s              save
  ctrl+e              edit notes in $EDITOR
  esc                 cancel

[bold $accent]App[/]
  ctrl+p              command palette (themes & more)
  ?                   this help
  q                   quit

[bold $accent]Vim profile[/]  [dim](--vim, REMTUI_KEYS=vim, or config)[/]
  gg / G              jump to top / bottom
  ctrl+d / ctrl+u     half page down / up
  ctrl+f / ctrl+b     full page down / up
  :                   command palette
  o                   add a reminder
"""


class HelpScreen(ModalScreen[None]):
    # The dialog styles travel with the dialogs, so the panel keeps its look
    # when another app hosts it.
    CSS_PATH = "dialogs.tcss"

    BINDINGS = [Binding("escape,q,question_mark", "close", "Close")]

    def compose(self) -> ComposeResult:
        with Vertical(id="dialog", classes="help-dialog"):
            yield Static("⌨  Keyboard Reference", id="help-title")
            yield Static(HELP_TEXT, id="help-body")
            yield Static("[dim]esc to close[/]", id="help-footer")

    def action_close(self) -> None:
        self.dismiss(None)
