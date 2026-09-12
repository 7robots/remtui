# remtui in Rust
Bear mirror: D5618AF6-EABA-4A95-BA6D-FB62D9E03D6F

Status: in progress, started 2026-09-11.

A re-implementation of remtui (`~/GitHub/remtui`, Python + Textual) in Rust with
ratatui, following the bjorn-rust port: same remctl and bearcli contracts, same
config file, same fakes, native binary. Feature parity with remtui at commit
`eb24c5b` is the target; the Python repo is untouched and keeps working, since
librarian embeds it in-process and a Rust binary cannot be embedded. Phase
numbers continue from remtui's last (6).

## Rulings

- 2026-09-11 — "I'd like you to build a plan and then execute on that plan to re-implement remtui in rust." (plan written and executed in the same session under that instruction)

## Decisions

- **Toolchain.** rustup stable via `/opt/homebrew/opt/rustup/bin`, edition 2024,
  `rustfmt` and `clippy --all-targets -D warnings` clean. No Python in the repo
  except `tools/pytest_rust_fake.py`, which runs remtui's pytest suite against
  the Rust fakes.
- **Crates.** `ratatui` + `crossterm` (event stream), `tokio` (process, sync,
  time), `serde`/`serde_json`, `toml`, `clap`, `chrono`, `regex`, `sha1`,
  `unicode-width`, `tempfile`, `shlex`, `thiserror`/`anyhow`. Dev:
  `pretty_assertions`.
- **Crate layout.** One package `remtui`, four binaries: `remtui`,
  `fake-remctl`, `fake-bearcli`, `remtui-gate`. Library modules mirror the
  Python ones: `client` (remctl), `models`, `dates`, `config`, `keys`
  (binding table, profiles, overrides), `bear`, `todos`, `triage`, `app`
  (state + update), `ui/` (sidebar, list, header, form, confirm, help, triage),
  `fake/` (both fakes' engines), `harness`, `editor`.
- **remctl contract unchanged.** `<sub> --json` first, `--opt=value` tokens,
  `--` before title/query positionals, `NO_COLOR=1` and `REMCTL_SKIP_ONBOARD=1`
  in the child env, `$REMTUI_REMCTL` then `remctl` on PATH, `--remctl` wins.
  Flag changes route through `flag`/`unflag`, never `edit --flagged`; a list
  move retargets the flag call at the returned `id`. `edit` sends only changed
  fields, empty due becomes `--due=clear`, empty priority `--priority=none`.
  `warnings` on exit 0 are surfaced as toasts. Writes serialize on one lock;
  reads run concurrently. Addition: every call gets a timeout, 30 s for reads
  and field edits, 120 s for `flag`/`unflag` (measured 25 s steady, 76 s cold).
  `displayDate` is preferred over `dueDate`.
- **Fakes are Rust binaries with the Python fakes' contract.** Same env vars
  (`REMTUI_FAKE_STATE`, `REMTUI_FAKE_BEAR_STATE`, `REMTUI_FAKE_FLAG_FAILS`),
  same default paths under `~/.cache/remtui/`, same seeds (5 lists, 18
  reminders ids 101–118; 4 Bear notes incl. one locked), same JSON shapes,
  error text and exit codes, same `opened` side record, atomic writes.
  `remtui --demo` finds them next to its own executable and forces `[bear]`
  on with list `Work`, due `today`. remtui's Python suite must pass against
  them; the gate proves it.
- **Config shared with the Python remtui.** `~/.config/remtui/config.toml`
  (`XDG_CONFIG_HOME` honored), same `[keys]` and `[bear]` tables, same lenient
  coercion, same default file written on first run. Precedence for vim:
  `--vim` > `REMTUI_KEYS=vim` > `profile = "vim"`. Overrides by binding id
  replace a binding's keys entirely (comma-separated), same 26 ids.
- **Runtime shape as in bjorn-rust.** Single UI task owns `App`; remctl work
  runs in tokio tasks reporting `Msg` values tagged with a generation; stale
  results dropped. Overlays are one enum (`Form`, `ConfirmDelete`, `Confirm`,
  `Help`, `Triage`) with the continuation carried in the value. Selection after
  a reload: same id, else the old index clamped. Toasts are a timed queue
  bottom-right. Terminal guard restores raw mode from `Drop`; `$EDITOR`
  (default `vim`) for the notes field leaves the alternate screen and clears
  on return.
- **Rendering.** Sidebar 30 cols with the RemTUI wordmark when height ≥ 20,
  smart views and lists with `●` in the list's hex color, header with title,
  stats and a progress bar for list views, rows with `◯`/`⬤`, `!!!` priority
  marks, `↻`, `⤷n`, details line, right-aligned `⚑` and humanized due.
  Apple palette as RGB where the Python used it; text and background take the
  terminal's defaults. Mouse: click to select, double-click to edit, wheel to
  scroll.
- **Flag writes get feedback** (the open UX problem in remtui's ROADMAP): `f`
  toggles the row optimistically and shows a pending glyph until remctl
  answers, reverting with an error toast on failure. The edit form dismisses
  after the field edit; a flag change continues in the background with the same
  pending state.
- **Not ported.** Textual's command palette and theme switching (`ctrl+p` and
  `:` open the help overlay instead); in-process embedding (librarian keeps the
  Python remtui); `SafeSelectMixin`, Textual worker groups and the populate
  lock, all obviated by the single-owner state.
- **Install.** `install.sh` builds `--release`, installs the four binaries
  under `~/.local/share/remtui-rs/bin` and links `~/bin/remtui-rs`. The
  Python `~/bin/remtui` stays for the comparison.
- **Tests.** Unit tests beside each module; integration tests in `tests/`
  spawn `env!("CARGO_BIN_EXE_fake-remctl")`; UI tests drive `App` through a
  `Harness` on a `TestBackend` with injected events and `wait_until`.

## Phases

### Phase 7 — Toolchain, config, client, models, fakes
Cargo skeleton, `config` with the default file, `keys` binding table with
profiles and overrides, `dates`, `models`, `client` with the routing rules and
timeouts, `bear` client and `todos` parser with the byte-identical key,
`triage::join`, both fakes complete against the Python fakes' surface.
Verify: `cargo test && cargo clippy --all-targets -- -D warnings`
Status: [x] done 2026-09-11 (52 passed: 36 unit, 11 client, 5 bear)

### Phase 8 — Read-only TUI
Terminal loop, sidebar with logo, smart views and lists, reminder rows, view
header with stats and bar, empty states, filter (`/`, live, `esc`), `c`, `r`,
navigation incl. vim profile (`gg` chord, half/full page), pane switching,
mouse, help overlay, `q`, `--demo`. `Harness` lands here with the UI tests.
Verify: `cargo test && cargo clippy --all-targets -- -D warnings`
Status: [x] done 2026-09-11 (74 passed: 42 unit, 11 client, 5 bear, 16 UI; demo checked in tmux)

### Phase 9 — Mutations
Add/edit form (title, notes, due, priority, list, flag, `ctrl+s`, `ctrl+e`
editor, validation, changed-fields-only edit, double-submit guard), delete
confirm with the safe button focused, `space` done/undone, `p` priority cycle,
`f` with optimistic toggle and pending glyph, toasts and remctl warnings,
selection kept across reloads, header counts refreshed after every write.
Verify: `cargo test && cargo clippy --all-targets -- -D warnings`
Status: [ ]

### Phase 10 — Bear triage
`b` behind `[bear] enabled`, grouped rows with note headers, mark/unmark,
add marked (or current) with linked notes, `o` open in Bear at the section,
`x` tick a done row after confirm, filter, reload, stats line, `bearcli`
resolution (`$REMTUI_BEARCLI` > config > PATH), missing-bearcli message.
Verify: `cargo test && cargo clippy --all-targets -- -D warnings`
Status: [ ]

### Phase 11 — Acceptance gate
1. remtui's pytest suite passes with the Rust fakes swapped in through
   `tools/pytest_rust_fake.py` (contract proof).
2. `cargo run --release --bin remtui-gate` against live remctl: lists load, a
   scratch reminder is added, edited, completed, reopened and deleted with
   read-back after each step; `--with-flag` adds flag/unflag.
3. Benchmarks under `caffeinate`, run twice: start to first frame, warm view
   reload, peak RSS, binary size; Python remtui measured the same way. Table in
   this repo's README.
4. `./install.sh`, then `~/bin/remtui-rs --demo` from a fresh shell.
Verify: the four steps above, in order, all passing
Status: [ ]
