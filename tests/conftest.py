import sys
from pathlib import Path

import pytest

from remtui.client import RemctlClient

FAKE = Path(__file__).parent.parent / "remtui" / "fake_remctl.py"


@pytest.fixture
def fake_state(tmp_path, monkeypatch):
    state = tmp_path / "state.json"
    monkeypatch.setenv("REMTUI_FAKE_STATE", str(state))
    return state


@pytest.fixture
def client(fake_state) -> RemctlClient:
    return RemctlClient([sys.executable, str(FAKE)])
