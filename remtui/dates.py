"""Date parsing and humanization helpers for reminder due dates."""

from __future__ import annotations

from datetime import date, datetime, timedelta

_WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


def parse_due(value: str | None) -> datetime | None:
    """Parse a due date string from remctl JSON output.

    Accepts ISO 8601 forms, with or without a time component or timezone.
    Returns a naive local datetime, or None if the value is empty/unparseable.
    """
    if not value:
        return None
    text = value.strip()
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError:
        return None
    if parsed.tzinfo is not None:
        parsed = parsed.astimezone().replace(tzinfo=None)
    return parsed


def humanize_due(due: datetime | None, *, all_day: bool = False, now: datetime | None = None) -> str:
    """Render a due datetime as a compact human string: 'Today 14:00', 'Tomorrow',
    'Yesterday', 'Fri 21:30', 'Jun 12', '3d overdue' is left to callers via is_overdue."""
    if due is None:
        return ""
    now = now or datetime.now()
    today = now.date()
    day = due.date()
    delta_days = (day - today).days

    if delta_days == 0:
        label = "Today"
    elif delta_days == 1:
        label = "Tomorrow"
    elif delta_days == -1:
        label = "Yesterday"
    elif 1 < delta_days <= 6:
        label = _WEEKDAYS[day.weekday()]
    elif day.year == today.year:
        label = f"{day:%b %-d}"
    else:
        label = f"{day:%b %-d, %Y}"

    if all_day or (due.hour, due.minute, due.second) == (0, 0, 0):
        return label
    return f"{label} {due:%H:%M}"


def is_overdue(due: datetime | None, *, all_day: bool = False, now: datetime | None = None) -> bool:
    """A reminder is overdue if its due moment has passed.

    All-day reminders only become overdue after their day ends; the allDay
    flag from remctl is authoritative (a timed reminder can be due 00:00).
    """
    if due is None:
        return False
    now = now or datetime.now()
    if all_day:
        return due.date() < now.date()
    return due < now


def is_due_today(due: datetime | None, *, now: datetime | None = None) -> bool:
    if due is None:
        return False
    now = now or datetime.now()
    return due.date() == now.date()


def sort_key(due: datetime | None) -> tuple[int, datetime | date]:
    """Sort reminders with due dates first (ascending), undated last."""
    if due is None:
        return (1, datetime.max)
    return (0, due)
