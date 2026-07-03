#!/usr/bin/env python3
"""A local stand-in for remctl, used for demo mode and tests.

Implements the subset of remctl's CLI that remtui drives, matching the real
tool's JSON contract: bare arrays on stdout for reads, compact status JSON
for mutations, "Error: #<id> not found" plain text on stderr (exit 1), and
structured {"status": "error", ...} JSON for invalid due dates (exit 2).

State lives in a JSON file at $REMTUI_FAKE_STATE (default
~/.cache/remtui/demo.json) and is seeded with sample data on first run.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from datetime import datetime, timedelta
from pathlib import Path

ISO = "%Y-%m-%dT%H:%M:%S"

LIST_COLORS = {
    "red": "#FF3B30",
    "orange": "#FF9500",
    "yellow": "#FFCC00",
    "green": "#34C759",
    "blue": "#007AFF",
    "purple": "#AF52DE",
    "brown": "#A2845E",
}


def state_path() -> Path:
    override = os.environ.get("REMTUI_FAKE_STATE")
    if override:
        return Path(override)
    return Path.home() / ".cache" / "remtui" / "demo.json"


# -- due date parsing --------------------------------------------------------

_TIME_RE = re.compile(r"^(\d{1,2}):(\d{2})$|^(\d{1,2})(am|pm)$", re.IGNORECASE)

_WEEKDAYS = {
    "mon": 0, "monday": 0,
    "tue": 1, "tues": 1, "tuesday": 1,
    "wed": 2, "wednesday": 2,
    "thu": 3, "thur": 3, "thurs": 3, "thursday": 3,
    "fri": 4, "friday": 4,
    "sat": 5, "saturday": 5,
    "sun": 6, "sunday": 6,
}


def _parse_time(text: str) -> tuple[int, int] | None:
    match = _TIME_RE.match(text.strip())
    if not match:
        return None
    if match.group(4):
        hour = int(match.group(3)) % 12
        if match.group(4).lower() == "pm":
            hour += 12
        return hour, 0
    return int(match.group(1)), int(match.group(2))


def parse_due_input(text: str, now: datetime) -> tuple[str, bool]:
    """Parse a user-supplied due string -> (ISO datetime, all_day).

    Raises ValueError on unparseable input, like the real remctl (exit 2).
    """
    text = text.strip()
    lowered = text.lower()

    if re.match(r"^\d{4}-\d{2}-\d{2}$", text):
        return f"{text}T00:00:00", True
    match = re.match(r"^(\d{4}-\d{2}-\d{2})[ T](\d{1,2}):(\d{2})$", text)
    if match:
        day, hour, minute = match.group(1), int(match.group(2)), int(match.group(3))
        return f"{day}T{hour:02d}:{minute:02d}:00", False

    match = re.match(r"^\+(\d+)d$", lowered)
    if match:
        day = (now + timedelta(days=int(match.group(1)))).date()
        return f"{day.isoformat()}T00:00:00", True

    words = lowered.replace(" at ", " ").split()
    base = {"today": 0, "tonight": 0, "tomorrow": 1}.get(words[0]) if words else None
    if base is not None:
        day = (now + timedelta(days=base)).date()
        if len(words) == 1:
            if words[0] == "tonight":
                return f"{day.isoformat()}T21:00:00", False
            return f"{day.isoformat()}T00:00:00", True
        parsed = _parse_time(words[1])
        if parsed:
            hour, minute = parsed
            return f"{day.isoformat()}T{hour:02d}:{minute:02d}:00", False

    if words and words[0] == "next":
        words = words[1:]
    if words and words[0] in _WEEKDAYS:
        delta = (_WEEKDAYS[words[0]] - now.weekday()) % 7 or 7
        day = (now + timedelta(days=delta)).date()
        if len(words) == 1:
            return f"{day.isoformat()}T00:00:00", True
        parsed = _parse_time(words[1])
        if parsed:
            hour, minute = parsed
            return f"{day.isoformat()}T{hour:02d}:{minute:02d}:00", False

    raise ValueError(text)


# -- state -------------------------------------------------------------------


def seed_state(now: datetime) -> dict:
    def iso_in(days: int, hour: int = 0, minute: int = 0) -> str:
        day = (now + timedelta(days=days)).date()
        return f"{day.isoformat()}T{hour:02d}:{minute:02d}:00"

    lists = [
        {"id": 1, "title": "Personal", "color": "blue", "emoji": ""},
        {"id": 2, "title": "Work", "color": "red", "emoji": ""},
        {"id": 3, "title": "Groceries", "color": "green", "emoji": "🛒"},
        {"id": 4, "title": "Reading", "color": "purple", "emoji": "📚"},
        {"id": 5, "title": "Home", "color": "orange", "emoji": ""},
    ]
    raw = [
        # title, list, due, all_day, priority, flagged, notes, tags, completed
        ("Renew passport", "Personal", iso_in(-3), True, "high", True,
         "Bring old passport and two photos", ["errands"], False),
        ("Call the dentist", "Personal", iso_in(0, 11, 30), False, "medium", False,
         "Reschedule the cleaning", [], False),
        ("Pay credit card bill", "Personal", iso_in(0), True, "high", False,
         "", ["finance"], False),
        ("Book flights for August", "Personal", iso_in(4), True, "none", False,
         "Check points balance first", ["travel"], False),
        ("Morning run", "Personal", iso_in(1, 7, 0), False, "none", False,
         "5k along the river", ["health"], True),
        ("Ship v2.3 release notes", "Work", iso_in(0, 16, 0), False, "high", True,
         "Waiting on changelog review from Sam", ["release"], False),
        ("Review Q3 hiring plan", "Work", iso_in(-1, 10, 0), False, "medium", False,
         "", [], False),
        ("1:1 prep — Patricia", "Work", iso_in(1, 9, 0), False, "medium", False,
         "Bring dashboard sync status", ["meetings"], False),
        ("File expense report", "Work", iso_in(6), True, "low", False,
         "Conference receipts in Downloads", [], False),
        ("Archive old sprint boards", "Work", "", False, "none", False, "", [], True),
        ("Milk", "Groceries", "", False, "none", False, "", [], False),
        ("Sourdough bread", "Groceries", "", False, "none", False, "", [], False),
        ("Olive oil", "Groceries", "", False, "none", False, "The good one", [], False),
        ("Coffee beans", "Groceries", "", False, "high", False, "", [], True),
        ("Finish 'The Shallows'", "Reading", iso_in(9), True, "none", False,
         "Chapter 7 onward", ["books"], False),
        ("Start sci-fi book club pick", "Reading", "", False, "low", False,
         "", ["books"], False),
        ("Replace furnace filter", "Home", iso_in(2), True, "medium", False,
         "16x25x1 — two spares in garage", [], False),
        ("Fix squeaky hinge", "Home", "", False, "none", False, "", [], False),
    ]
    reminders = []
    for index, row in enumerate(raw, start=101):
        title, list_name, due, all_day, priority, flagged, notes, tags, completed = row
        reminders.append(
            {
                "id": index,
                "title": title,
                "list": list_name,
                "completed": completed,
                "flagged": flagged,
                "urgent": False,
                "priority": priority,
                "notes": notes,
                "tags": tags,
                "dueDate": due,
                "allDay": all_day,
            }
        )
    for row in reminders:
        if row["title"] == "Start sci-fi book club pick":
            row["url"] = "https://bookclub.example.com/picks"
    return {"next_id": 200, "lists": lists, "reminders": reminders}


def load_state(now: datetime) -> dict:
    path = state_path()
    if not path.exists():
        state = seed_state(now)
        save_state(state)
        return state
    return json.loads(path.read_text())


def save_state(state: dict) -> None:
    path = state_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state, indent=2, ensure_ascii=False))


# -- serialization (matches remctl's field surface) ---------------------------


def serialize_reminder(row: dict) -> dict:
    out = {
        "id": row["id"],
        "title": row["title"],
        "list": row["list"],
        "completed": bool(row.get("completed")),
        "flagged": bool(row.get("flagged")),
        "urgent": bool(row.get("urgent")),
        "priority": row.get("priority") or "none",
        "subtaskCount": 0,
        "isSubtask": False,
    }
    if row.get("notes"):
        out["notes"] = row["notes"]
    if row.get("url"):
        out["url"] = row["url"]
    if row.get("dueDate"):
        out["dueDate"] = row["dueDate"]
        out["allDay"] = bool(row.get("allDay"))
    if row.get("tags"):
        out["tags"] = row["tags"]
    if row.get("completionDate"):
        out["completionDate"] = row["completionDate"]
    return out


def serialize_list(lst: dict, reminders: list[dict]) -> dict:
    mine = [r for r in reminders if r["list"] == lst["title"]]
    active = sum(1 for r in mine if not r["completed"])
    color = lst.get("color", "blue")
    out = {
        "id": lst["id"],
        "title": lst["title"],
        "listType": "groceries" if lst["title"] == "Groceries" else "standard",
        "isGroup": False,
        "isGroceries": lst["title"] == "Groceries",
        "color": {"name": color, "hex": LIST_COLORS.get(color, "#007AFF")},
        "counts": {"active": active, "completed": len(mine) - active, "total": len(mine)},
    }
    if lst.get("emoji"):
        out["badge"] = {"raw": lst["emoji"], "emoji": lst["emoji"]}
    return out


# -- helpers -----------------------------------------------------------------


def fail_not_found(reminder_id: int) -> None:
    print(f"Error: #{reminder_id} not found", file=sys.stderr)
    sys.exit(1)


def fail_invalid_due(value: str) -> None:
    payload = {
        "status": "error",
        "code": "invalid_due_date",
        "message": f"Could not parse due date: {value!r}",
        "field": "due",
        "input": value,
        "examples": ["2026-06-01", "tomorrow 09:30", "today at 3pm", "+3d"],
    }
    print(json.dumps(payload), file=sys.stderr)
    sys.exit(2)


def find_reminder(state: dict, reminder_id: int) -> dict:
    for row in state["reminders"]:
        if row["id"] == reminder_id:
            return row
    fail_not_found(reminder_id)
    raise AssertionError  # unreachable


def find_list(state: dict, name: str) -> dict | None:
    for lst in state["lists"]:
        if lst["title"] == name:
            return lst
    for lst in state["lists"]:
        if lst["title"].casefold() == name.casefold():
            return lst
    return None


def emit_array(rows: list[dict]) -> None:
    print(json.dumps([serialize_reminder(r) for r in rows], indent=2, ensure_ascii=False))


def emit_status(payload: dict) -> None:
    print(json.dumps(payload, ensure_ascii=False))


def due_datetime(row: dict) -> datetime | None:
    if not row.get("dueDate"):
        return None
    return datetime.strptime(row["dueDate"], ISO)


def apply_due(row: dict, value: str, now: datetime) -> None:
    if value == "clear":
        row["dueDate"] = ""
        row["allDay"] = False
        return
    try:
        iso, all_day = parse_due_input(value, now)
    except ValueError:
        fail_invalid_due(value)
        return
    row["dueDate"] = iso
    row["allDay"] = all_day


PRIORITY_ALIASES = {
    "high": "high", "h": "high",
    "medium": "medium", "med": "medium", "m": "medium",
    "low": "low", "l": "low",
    "none": "none",
}


# -- command handlers ---------------------------------------------------------


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(prog="remctl (fake)")
    sub = parser.add_subparsers(dest="command", required=True)

    def add_json(p: argparse.ArgumentParser) -> argparse.ArgumentParser:
        p.add_argument("--json", action="store_true")
        return p

    add_json(sub.add_parser("lists"))
    p_show = add_json(sub.add_parser("show"))
    p_show.add_argument("list")
    p_show.add_argument("--completed", action="store_true")
    add_json(sub.add_parser("today"))
    p_up = add_json(sub.add_parser("upcoming"))
    p_up.add_argument("days", nargs="?", type=int, default=7)
    add_json(sub.add_parser("overdue"))
    add_json(sub.add_parser("flagged"))
    p_search = add_json(sub.add_parser("search"))
    p_search.add_argument("query")
    p_search.add_argument("--completed", action="store_true")
    p_info = add_json(sub.add_parser("info"))
    p_info.add_argument("id", type=int)

    p_add = add_json(sub.add_parser("add"))
    p_add.add_argument("title")
    p_add.add_argument("-l", "--list", dest="list_name")
    p_add.add_argument("-n", "--notes", default="")
    p_add.add_argument("-d", "--due", default="")
    p_add.add_argument("-p", "--priority", default="")
    p_add.add_argument("-f", "--flag", action="store_true")
    p_add.add_argument("-t", "--tags", default="")
    p_add.add_argument("--url", default="")

    p_edit = add_json(sub.add_parser("edit"))
    p_edit.add_argument("id", type=int)
    p_edit.add_argument("--title")
    p_edit.add_argument("-n", "--notes")
    p_edit.add_argument("-d", "--due")
    p_edit.add_argument("-p", "--priority")
    p_edit.add_argument("-l", "--list", dest="list_name")
    flag_group = p_edit.add_mutually_exclusive_group()
    flag_group.add_argument("--flagged", dest="flagged", action="store_true", default=None)
    flag_group.add_argument("--no-flagged", dest="flagged", action="store_false")

    for name in ("done", "undone", "flag", "unflag"):
        p = add_json(sub.add_parser(name))
        p.add_argument("id", type=int)
    p_delete = add_json(sub.add_parser("delete"))
    p_delete.add_argument("id", type=int)
    p_delete.add_argument("--force", action="store_true")

    args = parser.parse_args(argv)
    now = datetime.now()
    state = load_state(now)
    reminders = state["reminders"]
    active = [r for r in reminders if not r["completed"]]

    if args.command == "lists":
        print(json.dumps(
            [serialize_list(lst, reminders) for lst in state["lists"]],
            indent=2, ensure_ascii=False,
        ))

    elif args.command == "show":
        lst = find_list(state, args.list)
        if lst is None:
            print(f"Error: list '{args.list}' not found", file=sys.stderr)
            sys.exit(1)
        rows = [r for r in reminders if r["list"] == lst["title"]]
        if not args.completed:
            rows = [r for r in rows if not r["completed"]]
        emit_array(rows)

    elif args.command == "today":
        emit_array([r for r in active
                    if (d := due_datetime(r)) and d.date() == now.date()])

    elif args.command == "upcoming":
        horizon = now.date() + timedelta(days=args.days)
        emit_array([r for r in active
                    if (d := due_datetime(r)) and now.date() <= d.date() <= horizon])

    elif args.command == "overdue":
        emit_array([
            r for r in active
            if (d := due_datetime(r)) is not None and (
                d.date() < now.date() if r.get("allDay") else d < now
            )
        ])

    elif args.command == "flagged":
        emit_array([r for r in active if r.get("flagged")])

    elif args.command == "search":
        pool = reminders if args.completed else active
        needle = args.query.casefold()
        emit_array([
            r for r in pool
            if needle in r["title"].casefold() or needle in (r.get("notes") or "").casefold()
        ])

    elif args.command == "info":
        row = find_reminder(state, args.id)
        payload = serialize_reminder(row)
        payload["subtasks"] = []
        print(json.dumps(payload, indent=2, ensure_ascii=False))

    elif args.command == "add":
        list_name = args.list_name or state["lists"][0]["title"]
        lst = find_list(state, list_name)
        if lst is None:
            print(f"Error: list '{list_name}' not found", file=sys.stderr)
            sys.exit(1)
        if args.priority and args.priority.lower() not in PRIORITY_ALIASES:
            print(f"Error: invalid priority '{args.priority}'", file=sys.stderr)
            sys.exit(1)
        priority = PRIORITY_ALIASES[args.priority.lower()] if args.priority else "none"
        row = {
            "id": state["next_id"],
            "title": args.title,
            "list": lst["title"],
            "completed": False,
            "flagged": bool(args.flag),
            "urgent": False,
            "priority": priority,
            "notes": args.notes,
            "tags": [t.strip() for t in args.tags.split(",") if t.strip()],
            "dueDate": "",
            "allDay": False,
        }
        if args.url:
            row["url"] = args.url
        if args.due:
            apply_due(row, args.due, now)
        state["next_id"] += 1
        reminders.append(row)
        save_state(state)
        emit_status({
            "status": "created",
            "id": f"FAKE-CK-{row['id']}",
            "title": row["title"],
            "numericId": row["id"],
        })

    elif args.command == "edit":
        row = find_reminder(state, args.id)
        if args.title is not None:
            row["title"] = args.title
        if args.notes is not None:
            row["notes"] = args.notes
        if args.priority is not None:
            if args.priority.lower() not in ("high", "medium", "low", "none"):
                print(f"Error: invalid priority '{args.priority}'", file=sys.stderr)
                sys.exit(1)
            row["priority"] = args.priority.lower()
        if args.flagged is not None:
            row["flagged"] = args.flagged
        if args.list_name is not None:
            lst = find_list(state, args.list_name)
            if lst is None:
                print(f"Error: list '{args.list_name}' not found", file=sys.stderr)
                sys.exit(1)
            row["list"] = lst["title"]
        if args.due is not None:
            apply_due(row, args.due, now)
        save_state(state)
        emit_status({"status": "updated", "id": row["id"]})

    elif args.command in ("done", "undone"):
        row = find_reminder(state, args.id)
        row["completed"] = args.command == "done"
        if args.command == "done":
            row["completionDate"] = now.strftime(ISO)
            status = "completed"
        else:
            row.pop("completionDate", None)
            status = "uncompleted"
        save_state(state)
        emit_status({"status": status, "id": row["id"], "title": row["title"]})

    elif args.command in ("flag", "unflag"):
        row = find_reminder(state, args.id)
        row["flagged"] = args.command == "flag"
        save_state(state)
        emit_status({
            "status": "flagged" if row["flagged"] else "unflagged",
            "id": row["id"],
            "title": row["title"],
        })

    elif args.command == "delete":
        row = find_reminder(state, args.id)
        if not args.force:
            print("Cancelled.")
            return
        reminders.remove(row)
        save_state(state)
        emit_status({"status": "deleted", "id": row["id"], "title": row["title"]})


if __name__ == "__main__":
    main()
