"""remtui — a Textual TUI for Apple Reminders, powered by remctl."""

from __future__ import annotations

import argparse
import os
import shutil
import sys
from pathlib import Path
from typing import Iterable

from textual.app import App, SystemCommand
from textual.screen import Screen
from textual.theme import Theme

from remtui.client import RemctlClient, resolve_remctl
from remtui.config import load_keys
from remtui.panel import RemindersPanel
from remtui.screen import RemindersScreen

REMTUI_THEME = Theme(
    name="remtui",
    primary="#0A84FF",
    secondary="#5E5CE6",
    accent="#FF9F0A",
    warning="#FFD60A",
    error="#FF453A",
    success="#30D158",
    foreground="#F2F2F7",
    background="#1C1C1E",
    surface="#2C2C2E",
    panel="#3A3A3C",
    dark=True,
)


class RemTuiApp(App[None]):
    """Browse, add, edit, complete, and delete Apple Reminders.

    A shell around `RemindersScreen`: this class owns only what belongs to an
    application -- the theme, the keymap, and the command palette entries. The
    UI and its logic live in `RemindersPanel`, which other apps can mount.
    """

    TITLE = "remtui"
    SUB_TITLE = "Apple Reminders"

    def __init__(
        self,
        client: RemctlClient,
        vim: bool = False,
        key_overrides: dict[str, str] | None = None,
    ) -> None:
        super().__init__()
        self.client = client
        self._vim = vim
        self._key_overrides = key_overrides or {}

    def get_default_screen(self) -> RemindersScreen:
        # The reminders view *is* this app, so it is the default screen rather
        # than something pushed on top: screen_stack stays one deep until a
        # modal opens.
        return RemindersScreen(self.client, vim=self._vim)

    def on_mount(self) -> None:
        self.register_theme(REMTUI_THEME)
        self.theme = "remtui"
        if self._key_overrides:
            self.set_keymap(dict(self._key_overrides))

    @property
    def panel(self) -> RemindersPanel:
        """The reminders panel of the default screen."""
        return self.query_one("#reminders-panel", RemindersPanel)

    def get_system_commands(self, screen: Screen) -> Iterable[SystemCommand]:
        yield from super().get_system_commands(screen)
        yield SystemCommand(
            "Add reminder", "Create a new reminder", self.panel.action_add_reminder
        )
        yield SystemCommand(
            "Refresh", "Reload lists and the current view", self.panel.action_refresh
        )
        yield SystemCommand(
            "Toggle completed reminders",
            "Show or hide completed reminders in list views",
            self.panel.action_toggle_completed,
        )
        yield SystemCommand(
            "Keyboard reference", "Show the key bindings", self.panel.action_help
        )


def build_client(
    argv: list[str] | None = None,
) -> tuple[RemctlClient, bool, dict[str, str]]:
    """Parse CLI args; return the client, vim-profile flag, and key overrides.

    The vim profile is enabled by `--vim`, `REMTUI_KEYS=vim`, or
    `profile = "vim"` in the config file (in that precedence order).
    Per-binding key overrides come from the config's [keys] section.
    """
    parser = argparse.ArgumentParser(
        prog="remtui",
        description="A Textual TUI for Apple Reminders, powered by remctl.",
    )
    parser.add_argument(
        "--demo",
        action="store_true",
        help="run against a bundled fake reminders store (no remctl needed)",
    )
    parser.add_argument(
        "--remctl",
        metavar="PATH",
        help="path to the remctl binary (default: $REMTUI_REMCTL or 'remctl' on PATH)",
    )
    parser.add_argument(
        "--vim",
        action="store_true",
        help="enable the vim key profile (gg/G, ctrl+d/u/f/b, :, o); "
        "also enabled via REMTUI_KEYS=vim or the config file",
    )
    args = parser.parse_args(argv)

    profile, overrides = load_keys()
    vim = (
        args.vim
        or os.environ.get("REMTUI_KEYS", "").lower() == "vim"
        or profile == "vim"
    )

    if args.demo:
        fake = Path(__file__).parent / "fake_remctl.py"
        return RemctlClient([sys.executable, str(fake)]), vim, overrides

    binary = args.remctl or resolve_remctl()
    if shutil.which(binary) is None:
        sys.exit(
            f"remtui: '{binary}' not found on PATH.\n"
            "Install remctl (https://github.com/viticci/remctl) and run "
            "'remctl onboard', or try 'remtui --demo'."
        )
    return RemctlClient(binary), vim, overrides


def main() -> None:
    client, vim, overrides = build_client()
    RemTuiApp(client, vim=vim, key_overrides=overrides).run()


if __name__ == "__main__":
    main()
