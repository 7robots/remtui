"""Tests for the XDG config file ([keys] profile + overrides)."""

import sys
from pathlib import Path

from remtui.app import RemTuiApp, build_client
from remtui.client import RemctlClient
from remtui.config import config_path, load_keys

FAKE = Path(__file__).parent.parent / "remtui" / "fake_remctl.py"


def test_first_run_writes_default_config(tmp_path, monkeypatch):
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path))
    profile, overrides = load_keys()
    assert profile == "default"
    assert overrides == {}
    assert config_path().exists()
    assert 'profile = "default"' in config_path().read_text()


def test_load_keys_parses_profile_and_overrides(tmp_path, monkeypatch):
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path))
    path = tmp_path / "remtui" / "config.toml"
    path.parent.mkdir(parents=True)
    path.write_text('[keys]\nprofile = "vim"\n"reminder.add" = "w"\n')
    profile, overrides = load_keys()
    assert profile == "vim"
    assert overrides == {"reminder.add": "w"}


def test_malformed_config_degrades_to_defaults(tmp_path, monkeypatch):
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path))
    path = tmp_path / "remtui" / "config.toml"
    path.parent.mkdir(parents=True)
    path.write_text("[keys\nnot toml")
    assert load_keys() == ("default", {})


def test_build_client_reads_vim_profile_from_config(tmp_path, monkeypatch):
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path))
    monkeypatch.delenv("REMTUI_KEYS", raising=False)
    path = tmp_path / "remtui" / "config.toml"
    path.parent.mkdir(parents=True)
    path.write_text('[keys]\nprofile = "vim"\n"reminder.add" = "w"\n')
    _, vim, overrides = build_client(["--demo"])
    assert vim is True
    assert overrides == {"reminder.add": "w"}


def test_flag_beats_config_default(tmp_path, monkeypatch):
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path))
    monkeypatch.delenv("REMTUI_KEYS", raising=False)
    _, vim, _ = build_client(["--demo", "--vim"])
    assert vim is True
    _, vim, _ = build_client(["--demo"])
    assert vim is False


async def test_keymap_override_rebinds_action(fake_state):
    app = RemTuiApp(
        RemctlClient([sys.executable, str(FAKE)]),
        key_overrides={"reminder.add": "w"},
    )
    async with app.run_test(size=(120, 36)) as pilot:
        await pilot.pause(1.0)
        app.query_one("#reminders").focus()
        await pilot.press("w")
        await pilot.pause(0.3)
        assert len(app.screen_stack) == 2  # add-reminder form opened via override
