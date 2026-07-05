"""User configuration: ~/.config/remtui/config.toml (XDG)."""

from __future__ import annotations

import os
import tomllib
from pathlib import Path

_DEFAULT_TOML = """\
# remtui configuration.

[keys]
# Key profile: "default", or "vim" for extra vim motions (gg, ctrl+d/u/f/b,
# ":" for the command palette, "o" to add a reminder). The --vim flag and
# the REMTUI_KEYS environment variable override this setting.
profile = "default"
# Per-binding overrides by binding id (see the README for the id list).
# An override replaces the binding's keys entirely; comma-separate to keep
# several, e.g.:
# "reminder.add" = "a,n"
"""


def config_path() -> Path:
    base = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    return base / "remtui" / "config.toml"


def load_keys() -> tuple[str, dict[str, str]]:
    """Return (profile, overrides) from [keys], writing a commented default
    config on first run. Unreadable or malformed config degrades to defaults
    rather than blocking startup."""
    path = config_path()
    if not path.exists():
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(_DEFAULT_TOML)
        except OSError:
            pass
        return "default", {}
    try:
        with open(path, "rb") as f:
            data = tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError):
        return "default", {}
    keys = data.get("keys", {})
    if not isinstance(keys, dict):
        return "default", {}
    profile = str(keys.get("profile", "default"))
    overrides = {
        k: v for k, v in keys.items() if k != "profile" and isinstance(v, str)
    }
    return profile, overrides
