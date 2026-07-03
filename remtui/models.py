"""Data models mirroring remctl's --json output schemas."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Any

from remtui.dates import parse_due, sort_key

_PRIORITY_RANK = {"high": 0, "medium": 1, "low": 2, "none": 3}


@dataclass(slots=True)
class Reminder:
    """One reminder, as serialized by remctl (serialize_reminder)."""

    id: int
    title: str
    list_name: str = ""
    completed: bool = False
    flagged: bool = False
    urgent: bool = False
    priority: str = "none"
    notes: str = ""
    url: str = ""
    section: str = ""
    tags: tuple[str, ...] = ()
    due_raw: str = ""
    all_day: bool = False
    subtask_count: int = 0
    is_subtask: bool = False
    recurring: bool = False

    @classmethod
    def from_json(cls, data: dict[str, Any]) -> Reminder:
        priority = data.get("priority") or "none"
        if priority not in _PRIORITY_RANK:
            priority = "none"
        return cls(
            id=int(data.get("id", 0)),
            title=str(data.get("title", "")),
            list_name=str(data.get("list", "")),
            completed=bool(data.get("completed")),
            flagged=bool(data.get("flagged")),
            urgent=bool(data.get("urgent")),
            priority=priority,
            notes=str(data.get("notes") or ""),
            url=str(data.get("url") or ""),
            section=str(data.get("section") or ""),
            tags=tuple(data.get("tags") or ()),
            due_raw=str(data.get("dueDate") or ""),
            all_day=bool(data.get("allDay")),
            subtask_count=int(data.get("subtaskCount") or 0),
            is_subtask=bool(data.get("isSubtask")),
            recurring=bool(data.get("recurrence")),
        )

    @property
    def due(self) -> datetime | None:
        return parse_due(self.due_raw)

    @property
    def display_key(self) -> tuple:
        """Sort: active before completed, then due date, priority, title."""
        return (
            self.completed,
            sort_key(self.due),
            _PRIORITY_RANK[self.priority],
            self.title.casefold(),
        )

    def matches(self, query: str) -> bool:
        """Case-insensitive substring match on title, notes, and tags."""
        needle = query.casefold()
        haystacks = (self.title, self.notes, " ".join(self.tags))
        return any(needle in text.casefold() for text in haystacks)


@dataclass(slots=True)
class ReminderList:
    """One reminder list, as serialized by remctl (list_to_dict)."""

    id: int
    title: str
    color_name: str = "blue"
    color_hex: str = "#007AFF"
    emoji: str = ""
    is_group: bool = False
    is_groceries: bool = False
    pinned: bool = False
    active: int = 0
    completed: int = 0
    total: int = 0

    @classmethod
    def from_json(cls, data: dict[str, Any]) -> ReminderList:
        color = data.get("color") or {}
        badge = data.get("badge") or {}
        counts = data.get("counts") or {}
        return cls(
            id=int(data.get("id", 0)),
            title=str(data.get("title", "")),
            color_name=str(color.get("name") or "blue"),
            color_hex=str(color.get("hex") or "#007AFF"),
            emoji=str(badge.get("emoji") or ""),
            is_group=bool(data.get("isGroup")),
            is_groceries=bool(data.get("isGroceries")),
            pinned=bool(data.get("pinned")),
            active=int(counts.get("active") or 0),
            completed=int(counts.get("completed") or 0),
            total=int(counts.get("total") or 0),
        )


def parse_lists(payload: list[dict[str, Any]]) -> list[ReminderList]:
    """Parse `lists --json` output: drop group containers, dedupe the
    flattened child duplicates by id, keep original order."""
    seen: set[int] = set()
    result: list[ReminderList] = []
    for item in payload:
        lst = ReminderList.from_json(item)
        if lst.is_group or lst.id in seen:
            continue
        seen.add(lst.id)
        result.append(lst)
    return result


def parse_reminders(payload: list[dict[str, Any]]) -> list[Reminder]:
    reminders = [Reminder.from_json(item) for item in payload]
    reminders.sort(key=lambda r: r.display_key)
    return reminders
