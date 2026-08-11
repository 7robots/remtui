"""remtui's own screen: the panel, framed by a header and footer."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.screen import Screen
from textual.widgets import Footer, Header

from remtui.client import RemctlClient
from remtui.panel import RemindersPanel


class RemindersScreen(Screen[None]):
    """Full-screen reminders view.

    Thin by design: everything that does the work lives in `RemindersPanel`, so
    another app can mount the panel without inheriting a header, a footer, or a
    theme.
    """

    def __init__(
        self,
        client: RemctlClient,
        vim: bool = False,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self._client = client
        self._vim = vim

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        yield RemindersPanel(self._client, vim=self._vim, id="reminders-panel")
        yield Footer()

    @property
    def panel(self) -> RemindersPanel:
        return self.query_one("#reminders-panel", RemindersPanel)
