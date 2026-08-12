# remtui Roadmap

Single source of truth for planned and deferred work.

Last reviewed 2026-08-10 against [remctl](https://github.com/viticci/remctl) v1.6.1.

## Planned

### Attachment badges in the reminder list

remctl 1.5.0 added an `attachments` array to the JSON of every list command
(`show`, `today`, `upcoming`, `overdue`, `flagged`, `urgent`, `search`), not
just `info`. remctl's own human output ends one-line summaries with 🔗 for a
rich link and 🌄 for an image attachment; remtui shows neither.

- Add `attachments` to `Reminder.from_json` and render the two badges in
  `widgets.py` next to the existing `⚑`/`↻` markers.
- Entry shape: `{filename, type, path, resolved, uti, width, height}`. `type`
  is a **string** (`"image"`, `"file"`, `"url"`) — note `docs/commands.md` in
  remctl still shows `"type": 1` in its read-output example; the corrected
  shape is in the Inline Images section of that file.
- `resolved: false` with `path: null` means the attachment never synced to
  this Mac. Treat as unavailable, not an error — badge it as present but
  don't imply it can be opened.
- Inline image *rendering* (`--images`, Kitty/iTerm2 protocols) is out of
  scope: Textual owns the screen, so escape sequences from remctl would
  corrupt it. Opening the verified `path` in an external viewer is the
  sensible equivalent if we want previews.

### Sidebar active counts on real data

`ReminderList` reads `counts.active/completed/total`, but real remctl only
attaches `counts` to lists nested inside a **group** (`attach_group_counts`);
top-level lists have no `counts` at all. So the sidebar's count column is
always blank against a real database while looking correct in demo mode,
because `fake_remctl.py` always emits `counts`.

Options, cheapest first:

1. Derive counts client-side from the reminders remtui already fetches per
   list (extra reads on refresh, no remctl change needed).
2. Drop the column for non-group lists so it stops looking broken.
3. Ask upstream to attach `counts` to every list in `lists --json`.

Whichever we pick, make `fake_remctl.py` match remctl and only emit `counts`
for group children — the current divergence is what hid this.

## Cross-cutting work

Three items span all four projects (librarian, remtui, projection, taskpapertui) and are tracked in
**librarian's** `docs/ROADMAP.md` rather than duplicated here:

- **Performance review** — establish a baseline before optimizing; nothing is known to be slow yet.
- **A Rust/ratatui prototype**, strictly conditional on that review. Note the constraint: this
  project's panel is embedded *in-process* by librarian, so rewriting it in Rust would drop it to the
  suspend-and-launch handoff.
- **Security and code review** — subprocess construction, untrusted text from other people's
  calendars/reminders reaching filenames and rendered output, path handling, and token safety.

## Deferred

### Recurrence editing

remctl 1.6.1 grew a real recurrence grammar: interval tokens (`daily x2`,
`weekly x2 thu`) and monthly Nth-weekday rules (`monthly 4th-fri`,
`monthly last-fri`, negative forms to `-5-fri`). Reads decode it back into a
stable `recurrence` object, with week-pinned weekdays as `daysOfWeekDetailed`
entries carrying `weekNumber`.

remtui only keeps `recurring: bool` and shows a `↻` badge, with no way to set
or change a schedule. A recurrence editor is a genuine feature, not an
integration fix, so it waits until the basics above are done. If we build it:
prefer `last-fri` over `5th-fri` (EventKit silently skips months without a
fifth Friday), and note that a rule ending after N occurrences now renders as
`daily, 5 times` because `x5` means the interval.

### Wider mutation surface

remctl supports plenty remtui doesn't expose: tags (`--set-tags`,
`--remove-tag`), sections, subtasks, alarms, early reminders, location alarms,
urgent state, assignments, smart lists, templates. Most need `--private`
(unsupported private ReminderKit writes). No plan to add these yet; listed so
the omission is a decision rather than an oversight.

## Notes on the remctl contract

Gotchas worth keeping in mind when touching `client.py`; verified against
v1.6.1.

- **The flag is private metadata.** `edit ID --flagged` / `--no-flagged` exits
  1 without `--private`, so flag changes must go through the separate
  `flag`/`unflag` commands. `client.edit(flagged=...)` does this routing.
- **Flagging is AppleScript-only** and needs macOS Automation permission.
  Since 1.6.1 it addresses reminders at application level (so lists nested in
  groups work), never touches priority, and fails honestly with
  `code: "applescript_flag_failed"` rather than reporting a fake success.
- **Partial success is a thing.** `add --flag` exits 0 with
  `warnings: ["flag_not_set: ..."]` when the reminder was created but only the
  flag write failed. `warnings_of()` extracts these; callers must surface them
  or the failure is invisible.
- **A list move can change the id.** `edit --list` may fall back to
  clone-delete, returning a new `id` plus `oldId`. Anything chained after a
  move must use the returned `id`.
- **Flag writes take ~25 seconds.** Measured against a real database on
  2026-08-10 under `caffeinate` (six consecutive `flag`/`unflag` calls):
  **76s for the first, then 24, 25, 37, 24, 25s** — so roughly 25s steady
  state with a ~76s cold first call, presumably Reminders.app and the
  AppleScript bridge warming up. A field-only `edit`, by contrast, is under a
  second. The cost is Reminders.app answering AppleScript, so it is remctl's
  floor, not something remtui can optimize away.

  Measure this under `caffeinate -dimsu` and check `pmset -g log` afterwards.
  An earlier unmeasured-sleep run produced a bogus 3m37s outlier: the laptop
  entered Idle Sleep for 228s mid-write, and the wall-clock time absorbed it.

  This is the main open UX problem. Pressing `f` and changing the flag in the
  edit dialog both block for around half a minute. Both run in workers so the
  app stays responsive, but there is no signal that anything is happening —
  the row does not change and the dialog just holds a disabled Save button.
  Worth doing:

  - a pending/spinner state on the row or dialog while a flag write is in
    flight, so a multi-minute write does not read as a hang;
  - an optimistic flag toggle in the list that reverts if the write fails
    (safe now that 1.6.1 reports failure honestly);
  - consider whether the edit dialog should dismiss immediately and let the
    flag write finish in the background, rather than holding the dialog.
- `edit` with nothing to change returns
  `{"status": "unchanged", "code": "nothing_to_update", ...}` with exit 0 —
  deliberately not an error status. remtui short-circuits before calling, so
  this is informational.
