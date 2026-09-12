# remtui in Rust

A terminal front end for **Apple Reminders**, written in Rust with
[ratatui](https://ratatui.rs) on top of [remctl](https://github.com/viticci/remctl).
It is a re-implementation of [remtui](https://github.com/7robots/remtui)
(Python and Textual), built to find out how the same design behaves as a
native binary. The two share one config file, one remctl contract, one set of
fakes, and one feature set; the Rust fakes of `remctl` and `bearcli` pass the
Python suite.

Smart views (Today, Upcoming, Overdue, Flagged) and your lists in a colored
sidebar, full CRUD on reminders (add, edit, complete, flag, delete, priority),
a live filter, mouse support, and the optional Bear todo triage behind
`[bear] enabled = true`. Keys, config and the triage screen are documented in
the Python project's README; they are the same here.

## Install

Requires macOS with Apple Reminders, remctl installed and onboarded, and a
Rust toolchain (`brew install rustup && rustup default stable`).

```sh
git clone https://github.com/7robots/remtui.git
cd remtui
./install.sh          # builds --release, puts a `remtui` launcher in ~/bin
remtui                # or: cargo run --release --bin remtui
remtui --demo         # sample reminders and Bear notes through the built-in fakes
remtui --vim          # the vim key profile (gg/G, ctrl+d/u/f/b, :, o)
```

The Python original is archived at
[7robots/remtui-python](https://github.com/7robots/remtui-python);
[librarian](https://github.com/7robots/librarian) still embeds its panel
in-process. `--remctl PATH` or `REMTUI_REMCTL` point at a specific remctl
binary.

## What differs from the Python remtui

- **Flag writes get feedback.** remctl's `flag`/`unflag` go through
  AppleScript and take about 25 s (76 s cold). `f` flips the row at once and
  shows a `◌` pending glyph until remctl answers, reverting with an error toast
  if the write fails. A flag change in the edit form dismisses the form and
  writes in the background the same way.
- Every `remctl` and `bearcli` call has a timeout: 30 s for reads and field
  edits, 120 s for flag writes.
- Textual's command palette and theme switching are not ported; `ctrl+p` and
  the vim profile's `:` open the keyboard reference instead. Colors take the
  terminal's foreground and background, with the Apple palette as accents.
- Switching views starts at the top of the new view instead of following the
  previously selected reminder.

## Measurements

Both implementations against the same live Reminders database, headless,
under `caffeinate`, each number the second of two runs that agreed. Timings
through `remctl` are the same in both; the Rust build changes what happens
after remctl answers.

| | Python (Textual) | Rust (ratatui) |
|---|---|---|
| Launch to first frame (6 lists, Today view) | 241 ms | 118 ms |
| Warm reload (`r`, of which remctl ≈ 107 ms) | 159 ms | 114 ms |
| Keypress to frame (median of 20 `j`/`k`) | 69.2 ms | 0.11 ms |
| Resident memory after the run | 51.6 MB | 9.1 MB |
| Installed size | 28.1 MB venv plus Python 3.12 | 3.3 MB binary |

remctl itself is a Python program and answers `lists` or `today` in about
105 ms, so both builds spend most of a reload waiting on it; the Rust build's
own work at startup is under 15 ms. The number that is felt is the keypress:
Textual re-renders the row widgets on every cursor move, ratatui redraws
styled lines. `tools/bench.sh` runs both; `remtui-gate --bench` and
`tools/bench_python.py` print the same keys.

## Development

```sh
cargo test                                 # unit, client, bear, UI, writes, triage
cargo clippy --all-targets -- -D warnings
cargo run --release --bin remtui-gate -- --list inbox            # acceptance gate against live remctl
cargo run --release --bin remtui-gate -- --list inbox --with-flag
cargo run --release --bin remtui-gate -- --bench
```

The Python suite runs against the Rust fakes with the plugin in `tools/`:

```sh
cd ../remtui-python
REMTUI_RUST_BIN=../remtui/target/release PYTHONPATH=../remtui/tools \
  uv run pytest -p pytest_rust_fake -q
```

The plan and its status live in `docs/plans/remtui-rust.md`; deferred work in
`docs/ROADMAP.md`. Module names mirror the Python package so the two trees
read side by side.
