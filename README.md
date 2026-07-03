# remtui

A fast, keyboard-driven TUI for **Apple Reminders**, built with
[Textual](https://github.com/Textualize/textual) on top of
[remctl](https://github.com/viticci/remctl).

Browse your lists in a colored sidebar, work through smart views (Today,
Upcoming, Overdue, Flagged), and do full CRUD on reminders — add, edit,
complete, flag, and delete — without leaving the terminal. Mouse works
everywhere too.

![remtui showing the Upcoming smart view](docs/screenshot.png)

## Requirements

- macOS with Apple Reminders
- [remctl](https://github.com/viticci/remctl) installed and onboarded:

  ```bash
  git clone https://github.com/viticci/remctl.git
  cd remctl && ./install.sh --bootstrap
  remctl onboard
  remctl permissions full-disk-access   # recommended for fast reads
  remctl doctor
  ```

- Python 3.12+ and [uv](https://docs.astral.sh/uv/)

## Install

```bash
./install.sh
remtui
```

The install script syncs the project environment with uv and drops a
`remtui` launcher into the same directory as your `remctl` binary
(e.g. `~/bin`), falling back to `~/.local/bin` if remctl isn't found.
Use `--dir DIR` to pick a location explicitly, `--uninstall` to remove it,
and re-run `./install.sh` after a `git pull` to update.

You can also skip installation and run from the checkout:

```bash
uv sync
uv run remtui
```

No remctl yet? Try the built-in demo backend (a fake reminders store, full
CRUD, no permissions needed):

```bash
uv run remtui --demo
```

Point at a specific remctl binary with `--remctl /path/to/remctl` or
`REMTUI_REMCTL=/path/to/remctl`.

## Keys

| Key | Action |
| --- | --- |
| `j`/`k`, `↑`/`↓` | move |
| `←`/`h` / `→`/`l`, `tab` | switch between sidebar and reminders |
| `g` / `G` | top / bottom |
| `a` | add reminder |
| `e` / `enter` | edit selected |
| `space` | toggle done |
| `d` / `⌫` | delete (asks first) |
| `f` | toggle flag |
| `p` | cycle priority |
| `/` | filter current view (`esc` clears) |
| `c` | show/hide completed (list views) |
| `r` | refresh |
| `ctrl+p` | command palette (switch themes, …) |
| `?` | help |
| `q` | quit |

Due dates in the add/edit form accept remctl's formats: `2026-08-01`,
`tomorrow 09:30`, `today at 3pm`, `fri 15:00`, `+3d` — leave blank for none;
clearing the field on edit removes the due date.

## Development

```bash
uv run pytest                          # full suite (unit + pilot-driven TUI tests)
uv run textual run --dev -c remtui --demo   # run under Textual devtools
```

The test suite and `--demo` mode run against `remtui/fake_remctl.py`, a
faithful emulation of remctl's JSON contract backed by a JSON state file
(`$REMTUI_FAKE_STATE`, default `~/.cache/remtui/demo.json`).

## Architecture

- `remtui/client.py` — async subprocess wrapper around `remctl … --json`;
  reads run concurrently, mutations are serialized (EventKit writes race).
- `remtui/models.py` — dataclasses mirroring remctl's JSON schemas.
- `remtui/app.py` — the Textual app: sidebar (smart views + lists), reminder
  pane, workers for loading and mutations.
- `remtui/screens.py` — modal add/edit form, delete confirmation, help.
- `remtui/widgets.py` — reminder rows, sidebar options, view header.

## Acknowledgements

remtui exists because of [remctl](https://github.com/viticci/remctl), the
excellent Apple Reminders CLI by [Federico Viticci](https://www.macstories.net).
Everything hard about talking to Reminders — fast reads straight from the
local store, writes that respect iCloud sync, sections, tags, smart lists,
and a clean, scriptable JSON interface designed with automation and AI
agents in mind — is remctl's work. This project is just a friendly terminal
face on top of it.

If you find remtui useful, the thanks belong upstream: go star remctl, and
check out Federico's writing at [MacStories](https://www.macstories.net).
Thank you, Federico, for building such a thoughtful, well-crafted tool and
sharing it with the community.
