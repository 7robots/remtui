"""Custom widgets: reminder rows, sidebar option builders, view header."""

from __future__ import annotations

from dataclasses import dataclass

from rich.table import Table
from rich.text import Text
from textual.app import ComposeResult
from textual.widget import Widget
from textual.widgets import ListItem, ProgressBar, Static
from textual.widgets.option_list import Option

from remtui.dates import humanize_due, is_due_today, is_overdue
from remtui.models import Reminder, ReminderList

PRIORITY_MARKS = {"high": "!!!", "medium": "!!", "low": "!"}
PRIORITY_COLORS = {"high": "#FF453A", "medium": "#FF9F0A", "low": "#FFD60A"}

COLOR_TODAY = "#0A84FF"
COLOR_OVERDUE = "#FF453A"
COLOR_FLAG = "#FF9F0A"
COLOR_DONE = "#30D158"
COLOR_TAG = "#64D2FF"


@dataclass(frozen=True)
class SmartView:
    """A built-in virtual view backed by a remctl smart command."""

    key: str
    label: str
    icon: str
    color: str
    empty: str


SMART_VIEWS = (
    SmartView("today", "Today", "◉", COLOR_TODAY, "Nothing due today — enjoy it"),
    SmartView("upcoming", "Upcoming", "◷", "#BF5AF2", "Nothing scheduled in the next 7 days"),
    SmartView("overdue", "Overdue", "⚠", COLOR_OVERDUE, "Nothing overdue — inbox zero energy"),
    SmartView("flagged", "Flagged", "⚑", COLOR_FLAG, "No flagged reminders"),
)


class ReminderItem(ListItem):
    """One reminder row: check glyph | title + details | flags + due date."""

    def __init__(self, reminder: Reminder) -> None:
        super().__init__()
        self.reminder = reminder
        self.overdue = not reminder.completed and is_overdue(
            reminder.due, all_day=reminder.all_day
        )
        if reminder.completed:
            self.add_class("-done")

    def compose(self) -> ComposeResult:
        yield Static(self._check(), classes="check")
        yield Static(self._body(), classes="body")
        meta = self._meta()
        meta_static = Static(meta, classes="meta")
        # Horizontal layout gives the 1fr body the remainder only after
        # fixed widths resolve; pin the meta cell to its rendered width so
        # "Today 11:30" never wraps or crops.
        meta_static.styles.width = max(meta.cell_len, 1)
        yield meta_static

    def _check(self) -> Text:
        if self.reminder.completed:
            return Text("●", style=COLOR_DONE)
        return Text("○", style=COLOR_OVERDUE if self.overdue else "dim")

    def _body(self) -> Text:
        r = self.reminder
        body = Text(overflow="ellipsis")
        if not r.completed and r.priority in PRIORITY_MARKS:
            body.append(
                PRIORITY_MARKS[r.priority] + " ",
                style=f"bold {PRIORITY_COLORS[r.priority]}",
            )
        body.append(r.title, style="dim strike" if r.completed else "")
        if r.recurring:
            body.append(" ↻", style=f"dim {COLOR_TAG}")
        if r.subtask_count:
            body.append(f" ⤷{r.subtask_count}", style="dim")

        details: list[Text] = []
        if r.section:
            details.append(Text(f"§ {r.section}", style="dim italic"))
        if r.notes:
            first_line = r.notes.splitlines()[0]
            if len(first_line) > 72:
                first_line = first_line[:71] + "…"
            details.append(Text(first_line, style="dim"))
        if r.tags:
            details.append(
                Text(" ".join(f"#{tag}" for tag in r.tags), style=f"dim {COLOR_TAG}")
            )
        if r.url:
            details.append(Text("🔗", style="dim"))
        if details:
            body.append("\n")
            body.append(Text(" · ", style="dim").join(details))
        return body

    def _meta(self) -> Text:
        r = self.reminder
        meta = Text(justify="right", no_wrap=True)
        if r.urgent:
            meta.append("⏰ ", style=COLOR_FLAG)
        if r.flagged:
            meta.append("⚑ ", style=COLOR_FLAG)
        due = r.due
        if due is not None:
            label = humanize_due(due, all_day=r.all_day)
            if r.completed:
                style = "dim"
            elif self.overdue:
                style = f"bold {COLOR_OVERDUE}"
            elif is_due_today(due):
                style = COLOR_TODAY
            else:
                style = "dim"
            meta.append(label, style=style)
        return meta


def nav_header(label: str) -> Option:
    """A non-selectable section heading for the sidebar."""
    return Option(Text(label.upper(), style="bold #6E6E73"), disabled=True)


def smart_option(view: SmartView) -> Option:
    prompt = Text(no_wrap=True, overflow="ellipsis")
    prompt.append(f"{view.icon} ", style=view.color)
    prompt.append(view.label)
    return Option(prompt, id=f"view:{view.key}")


def list_option(lst: ReminderList) -> Option:
    grid = Table.grid(expand=True)
    grid.add_column(ratio=1, no_wrap=True, overflow="ellipsis")
    grid.add_column(justify="right")
    left = Text()
    left.append("● ", style=lst.color_hex)
    if lst.emoji:
        left.append(f"{lst.emoji} ")
    left.append(lst.title)
    right = Text(str(lst.active) if lst.active else "", style="dim")
    grid.add_row(left, right)
    return Option(grid, id=f"list:{lst.id}")


class ViewHeader(Widget):
    """Header strip above the reminder list: view name, counts, progress."""

    DEFAULT_CSS = """
    ViewHeader {
        height: auto;
        padding: 1 2 0 2;
    }
    ViewHeader #vh-title { text-style: bold; }
    ViewHeader #vh-stats { color: $text-muted; }
    ViewHeader #vh-bar { width: 32; height: 1; display: none; }
    ViewHeader #vh-bar.-visible { display: block; }
    """

    def compose(self) -> ComposeResult:
        yield Static("", id="vh-title")
        yield Static("", id="vh-stats")
        yield ProgressBar(id="vh-bar", show_eta=False)

    def show_view(
        self,
        *,
        label: str,
        icon: str,
        color: str,
        shown: int,
        active: int | None = None,
        completed: int | None = None,
        filter_text: str = "",
    ) -> None:
        title = Text()
        title.append(f"{icon} ", style=color)
        title.append(label, style=f"bold {color}")
        self.query_one("#vh-title", Static).update(title)

        parts: list[str] = []
        if active is not None:
            parts.append(f"{active} active")
        if completed:
            parts.append(f"{completed} done")
        if active is None:
            parts.append(f"{shown} reminder{'s' if shown != 1 else ''}")
        if filter_text:
            parts.append(f'filter "{filter_text}" → {shown} match{"es" if shown != 1 else ""}')
        self.query_one("#vh-stats", Static).update(Text(" · ".join(parts)))

        bar = self.query_one("#vh-bar", ProgressBar)
        total = (active or 0) + (completed or 0)
        if completed is not None and total > 0:
            bar.add_class("-visible")
            bar.update(total=total, progress=completed)
        else:
            bar.remove_class("-visible")
