"""pytest plugin: run the Python remtui's suite against the Rust fakes.

The Python conftest builds `RemctlClient([python, fake_remctl.py])` and
`BearClient([python, fake_bearcli.py])` directly, and `build_client(["--demo"])`
does the same. This plugin rewrites those commands to the Rust `fake-remctl` /
`fake-bearcli` binaries, so every test that drives a fake as a subprocess proves
the Rust fake honours the same contract. The state files and their env vars are
unchanged.

    cd ~/GitHub/remtui
    REMTUI_RUST_BIN=~/GitHub/remtui-rust/target/release \
      PYTHONPATH=~/GitHub/remtui-rust/tools uv run pytest -p pytest_rust_fake -q
"""

from __future__ import annotations

import os
from pathlib import Path

RUST_BIN = Path(
    os.environ.get(
        "REMTUI_RUST_BIN",
        str(Path.home() / "GitHub" / "remtui-rust" / "target" / "release"),
    )
)


def _swap(command, python_name: str, rust_name: str):
    parts = (command,) if isinstance(command, str) else tuple(command)
    if len(parts) >= 2 and str(parts[-1]).endswith(python_name):
        return (str(RUST_BIN / rust_name),)
    return parts


def pytest_configure(config):
    import remtui.bear
    import remtui.client

    remctl_init = remtui.client.RemctlClient.__init__

    def remctl_wrapped(self, command="remctl"):
        remctl_init(self, _swap(command, "fake_remctl.py", "fake-remctl"))

    remtui.client.RemctlClient.__init__ = remctl_wrapped

    bear_init = remtui.bear.BearClient.__init__

    def bear_wrapped(self, command=remtui.bear.DEFAULT_COMMAND):
        bear_init(self, _swap(command, "fake_bearcli.py", "fake-bearcli"))

    remtui.bear.BearClient.__init__ = bear_wrapped
    config.addinivalue_line("markers", "rust_fake: suite is running against the Rust fakes")
    print(f"\n[pytest_rust_fake] remctl/bearcli fakes -> {RUST_BIN}")
