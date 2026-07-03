"""Async wrapper around the remctl CLI.

Reads use `--json` (bare arrays on stdout); mutations emit compact status
JSON. Errors arrive on stderr as either structured
{"status": "error", "code": ..., "message": ...} JSON or plain
"Error: ..." text — both are normalized into RemctlError.

Reads are safe to run concurrently (remctl opens the store read-only);
mutations are serialized through an asyncio lock because concurrent
EventKit writes race.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import os
from typing import Any, Sequence

from remtui.models import Reminder, ReminderList, parse_lists, parse_reminders

#: Sentinel distinguishing "leave unchanged" from "set to empty" in edit().
UNSET: Any = object()


class RemctlError(Exception):
    """A remctl invocation failed."""

    def __init__(self, message: str, *, code: str = "", exit_code: int = 1) -> None:
        super().__init__(message)
        self.message = message
        self.code = code
        self.exit_code = exit_code


def _parse_stderr(stderr: str) -> tuple[str, str]:
    """Extract (message, code) from remctl stderr — structured JSON if
    present, otherwise the first non-empty plain-text line."""
    for line in stderr.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            data = json.loads(line)
        except ValueError:
            continue
        if isinstance(data, dict) and data.get("status") == "error":
            return str(data.get("message") or "remctl error"), str(data.get("code") or "")
    for line in stderr.splitlines():
        if line.strip():
            return line.strip(), ""
    return "remctl failed with no error output", ""


class RemctlClient:
    """Shells out to remctl with --json and parses the results."""

    def __init__(self, command: str | Sequence[str] = "remctl") -> None:
        self.command: tuple[str, ...] = (
            (command,) if isinstance(command, str) else tuple(command)
        )
        self._write_lock = asyncio.Lock()

    async def _run(self, *args: str) -> Any:
        env = {**os.environ, "NO_COLOR": "1", "REMCTL_SKIP_ONBOARD": "1"}
        # --json goes right after the subcommand: trailing positionals may be
        # preceded by a "--" guard, after which no flags are recognized.
        subcommand, *rest = args
        try:
            proc = await asyncio.create_subprocess_exec(
                *self.command,
                subcommand,
                "--json",
                *rest,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                stdin=asyncio.subprocess.DEVNULL,
                env=env,
            )
        except FileNotFoundError as exc:
            raise RemctlError(
                f"remctl not found ({self.command[0]}). Install it from "
                "https://github.com/viticci/remctl or run with --demo.",
                code="not_found",
            ) from exc
        try:
            stdout_b, stderr_b = await proc.communicate()
        except asyncio.CancelledError:
            # Don't leave an orphaned write racing the next mutation: the
            # caller's lock is released on cancellation, so the process must
            # not outlive this coroutine.
            with contextlib.suppress(ProcessLookupError):
                proc.kill()
            with contextlib.suppress(Exception):
                await proc.wait()
            raise
        stdout, stderr = stdout_b.decode(), stderr_b.decode()
        if proc.returncode != 0:
            message, code = _parse_stderr(stderr)
            raise RemctlError(message, code=code, exit_code=proc.returncode or 1)
        if not stdout.strip():
            return None
        try:
            return json.loads(stdout)
        except ValueError as exc:
            raise RemctlError(f"remctl returned invalid JSON: {exc}") from exc

    # -- reads ------------------------------------------------------------

    async def get_lists(self) -> list[ReminderList]:
        return parse_lists(await self._run("lists") or [])

    async def get_reminders(
        self, list_title: str, *, include_completed: bool = False
    ) -> list[Reminder]:
        args = ["show"]
        if include_completed:
            args.append("--completed")
        args += ["--", list_title]
        return parse_reminders(await self._run(*args) or [])

    async def today(self) -> list[Reminder]:
        return parse_reminders(await self._run("today") or [])

    async def upcoming(self, days: int = 7) -> list[Reminder]:
        return parse_reminders(await self._run("upcoming", str(days)) or [])

    async def overdue(self) -> list[Reminder]:
        return parse_reminders(await self._run("overdue") or [])

    async def flagged(self) -> list[Reminder]:
        return parse_reminders(await self._run("flagged") or [])

    async def search(
        self, query: str, *, include_completed: bool = False
    ) -> list[Reminder]:
        args = ["search"]
        if include_completed:
            args.append("--completed")
        args += ["--", query]
        return parse_reminders(await self._run(*args) or [])

    # -- mutations ---------------------------------------------------------

    async def _mutate(self, *args: str) -> Any:
        async with self._write_lock:
            return await self._run(*args)

    async def add(
        self,
        title: str,
        *,
        list_title: str = "",
        notes: str = "",
        due: str = "",
        priority: str = "",
        flagged: bool = False,
        tags: str = "",
        url: str = "",
    ) -> Any:
        args = ["add"]
        if list_title:
            args.append(f"--list={list_title}")
        if notes:
            args.append(f"--notes={notes}")
        if due:
            args.append(f"--due={due}")
        if priority and priority != "none":
            args.append(f"--priority={priority}")
        if flagged:
            args.append("--flag")
        if tags:
            args.append(f"--tags={tags}")
        if url:
            args.append(f"--url={url}")
        args += ["--", title]
        return await self._mutate(*args)

    async def edit(
        self,
        reminder_id: int,
        *,
        title: Any = UNSET,
        notes: Any = UNSET,
        due: Any = UNSET,
        priority: Any = UNSET,
        list_title: Any = UNSET,
        flagged: Any = UNSET,
    ) -> Any:
        args = ["edit", str(reminder_id)]
        if title is not UNSET:
            args.append(f"--title={title}")
        if notes is not UNSET:
            args.append(f"--notes={notes}")
        if due is not UNSET:
            args.append(f"--due={due or 'clear'}")
        if priority is not UNSET:
            args.append(f"--priority={priority or 'none'}")
        if list_title is not UNSET:
            args.append(f"--list={list_title}")
        if flagged is not UNSET:
            args.append("--flagged" if flagged else "--no-flagged")
        if len(args) == 2:
            return None
        return await self._mutate(*args)

    async def done(self, reminder_id: int) -> Any:
        return await self._mutate("done", str(reminder_id))

    async def undone(self, reminder_id: int) -> Any:
        return await self._mutate("undone", str(reminder_id))

    async def delete(self, reminder_id: int) -> Any:
        return await self._mutate("delete", str(reminder_id), "--force")

    async def flag(self, reminder_id: int) -> Any:
        return await self._mutate("flag", str(reminder_id))

    async def unflag(self, reminder_id: int) -> Any:
        return await self._mutate("unflag", str(reminder_id))
