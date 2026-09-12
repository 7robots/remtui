# Roadmap

Single source of truth for planned and deferred work in remtui (the Rust build). The
active plan lives in `docs/plans/remtui-rust.md`.

## Next

  or the two keep coexisting. librarian embeds the Python panel in-process, so
  the Python package stays installed either way.

## Deferred (carried from remtui's ROADMAP, still open here)

- Sidebar counts are blank on real data: remctl attaches `counts` only to
  lists nested in a group. Derive them client-side or drop the column.
- Attachment badges (`attachments` array on every list command; 🔗 and 🌄).
- Recurrence editing (remctl 1.6.1 grammar); only the `↻` badge is shown.
- Wider mutation surface: tags, sections, subtasks, alarms, urgent,
  assignments, smart lists, templates (most need `--private`).
- Bear triage: in-screen list picker, rewording detection, nested todos under
  a ticked parent.
