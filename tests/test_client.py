"""Integration tests: RemctlClient driving the fake remctl subprocess."""

import pytest

from remtui.client import RemctlClient, RemctlError, warnings_of


async def test_get_lists(client: RemctlClient):
    lists = await client.get_lists()
    titles = [lst.title for lst in lists]
    assert "Personal" in titles and "Groceries" in titles
    personal = next(lst for lst in lists if lst.title == "Personal")
    assert personal.color_hex == "#007AFF"
    assert personal.active > 0
    groceries = next(lst for lst in lists if lst.title == "Groceries")
    assert groceries.is_groceries
    assert groceries.emoji == "🛒"


async def test_get_reminders_excludes_completed_by_default(client: RemctlClient):
    active = await client.get_reminders("Personal")
    assert all(not r.completed for r in active)
    everything = await client.get_reminders("Personal", include_completed=True)
    assert len(everything) > len(active)
    assert any(r.completed for r in everything)


async def test_reminder_fields_parse(client: RemctlClient):
    reminders = await client.get_reminders("Personal", include_completed=True)
    passport = next(r for r in reminders if "passport" in r.title.lower())
    assert passport.priority == "high"
    assert passport.flagged
    assert passport.all_day
    assert passport.due is not None
    assert "errands" in passport.tags
    assert passport.notes


async def test_smart_views(client: RemctlClient):
    today = await client.today()
    assert today and all(not r.completed for r in today)
    overdue = await client.overdue()
    assert any("passport" in r.title.lower() for r in overdue)
    flagged = await client.flagged()
    assert all(r.flagged for r in flagged)
    upcoming = await client.upcoming(30)
    assert len(upcoming) >= len(today)


async def test_search(client: RemctlClient):
    hits = await client.search("dentist")
    assert len(hits) == 1
    assert hits[0].title == "Call the dentist"


async def test_add_edit_done_delete_roundtrip(client: RemctlClient):
    result = await client.add(
        "Wash the car",
        list_title="Home",
        notes="Use the good soap",
        due="tomorrow 09:30",
        priority="medium",
        flagged=True,
        tags="chores,car",
    )
    assert result["status"] == "created"
    new_id = result["numericId"]

    reminders = await client.get_reminders("Home")
    added = next(r for r in reminders if r.id == new_id)
    assert added.priority == "medium"
    assert added.flagged
    assert added.due is not None and not added.all_day
    assert "chores" in added.tags

    result = await client.edit(new_id, title="Wash & wax the car", priority="high", due="")
    assert result["status"] == "updated"
    edited = next(
        r for r in await client.get_reminders("Home") if r.id == new_id
    )
    assert edited.title == "Wash & wax the car"
    assert edited.priority == "high"
    assert edited.due is None  # due="" means clear

    assert (await client.done(new_id))["status"] == "completed"
    assert (await client.undone(new_id))["status"] == "uncompleted"
    assert (await client.unflag(new_id))["status"] == "unflagged"
    assert (await client.flag(new_id))["status"] == "flagged"
    assert (await client.delete(new_id))["status"] == "deleted"
    assert all(r.id != new_id for r in await client.get_reminders("Home"))


async def test_add_title_with_leading_dash(client: RemctlClient):
    result = await client.add("-dangerous title", list_title="Home")
    assert result["status"] == "created"
    reminders = await client.get_reminders("Home")
    assert any(r.title == "-dangerous title" for r in reminders)


async def test_move_between_lists(client: RemctlClient):
    result = await client.add("Migrating task", list_title="Home")
    new_id = result["numericId"]
    await client.edit(new_id, list_title="Work")
    assert any(r.id == new_id for r in await client.get_reminders("Work"))
    assert all(r.id != new_id for r in await client.get_reminders("Home"))


async def test_not_found_raises_plain_text_error(client: RemctlClient):
    with pytest.raises(RemctlError) as excinfo:
        await client.done(99999)
    assert "not found" in str(excinfo.value)
    assert excinfo.value.exit_code == 1


async def test_invalid_due_raises_structured_error(client: RemctlClient):
    with pytest.raises(RemctlError) as excinfo:
        await client.add("Bad due", due="whenever")
    assert excinfo.value.code == "invalid_due_date"
    assert excinfo.value.exit_code == 2


async def test_missing_binary():
    client = RemctlClient("definitely-not-a-real-binary-xyz")
    with pytest.raises(RemctlError) as excinfo:
        await client.get_lists()
    assert excinfo.value.code == "not_found"


async def test_search_with_leading_dash_is_guarded(client: RemctlClient):
    hits = await client.search("-milk")
    assert hits == []  # parsed as a query, not an option


async def test_weekday_due_form(client: RemctlClient):
    result = await client.add("Weekday due", list_title="Home", due="fri 9:00")
    reminders = await client.get_reminders("Home")
    added = next(r for r in reminders if r.id == result["numericId"])
    assert added.due is not None
    assert added.due.weekday() == 4
    assert (added.due.hour, added.due.minute) == (9, 0)


async def test_url_field_roundtrip(client: RemctlClient):
    result = await client.add("With link", list_title="Home", url="https://example.com/x")
    reminders = await client.get_reminders("Home")
    added = next(r for r in reminders if r.id == result["numericId"])
    assert added.url == "https://example.com/x"


# -- flag routing ------------------------------------------------------------
# remctl rejects `edit --flagged` without --private, so edit() must never emit
# it; flag changes go through the flag/unflag commands instead.


async def test_edit_flagged_uses_flag_command(client: RemctlClient):
    new_id = (await client.add("Flag me", list_title="Home"))["numericId"]

    result = await client.edit(new_id, flagged=True)
    assert result["status"] == "flagged"
    edited = next(r for r in await client.get_reminders("Home") if r.id == new_id)
    assert edited.flagged

    assert (await client.edit(new_id, flagged=False))["status"] == "unflagged"
    assert not next(
        r for r in await client.get_reminders("Home") if r.id == new_id
    ).flagged


async def test_edit_applies_fields_and_flag_together(client: RemctlClient):
    new_id = (await client.add("Both", list_title="Home", priority="low"))["numericId"]
    # The field edit's payload wins, so callers still see status "updated".
    result = await client.edit(new_id, title="Both changed", priority="high", flagged=True)
    assert result["status"] == "updated"
    edited = next(r for r in await client.get_reminders("Home") if r.id == new_id)
    assert edited.title == "Both changed"
    assert edited.priority == "high"
    assert edited.flagged


async def test_edit_flag_follows_new_id_after_clone_delete_move(
    client: RemctlClient, monkeypatch
):
    """A pure list move can clone-delete and return a new id; flag that one.

    The fake keeps ids stable across moves, so the reassignment real remctl
    does on its clone-delete fallback is stubbed in here.
    """
    flagged_ids = []

    async def fake_mutate(*args):
        return {"status": "updated", "id": 777, "oldId": 1, "method": "clone-delete"}

    async def fake_flag(reminder_id):
        flagged_ids.append(reminder_id)
        return {"status": "flagged", "id": reminder_id}

    monkeypatch.setattr(client, "_mutate", fake_mutate)
    monkeypatch.setattr(client, "flag", fake_flag)
    await client.edit(1, list_title="Work", flagged=True)
    assert flagged_ids == [777]


async def test_edit_flag_uses_original_id_without_a_field_edit(
    client: RemctlClient, monkeypatch
):
    flagged_ids = []

    async def fake_flag(reminder_id):
        flagged_ids.append(reminder_id)
        return {"status": "flagged", "id": reminder_id}

    monkeypatch.setattr(client, "flag", fake_flag)
    await client.edit(42, flagged=True)
    assert flagged_ids == [42]


async def test_edit_with_no_changes_is_a_noop(client: RemctlClient):
    new_id = (await client.add("Untouched", list_title="Home"))["numericId"]
    assert await client.edit(new_id) is None


# -- partial failures --------------------------------------------------------


@pytest.fixture
def failing_flags(monkeypatch):
    """Simulate a Mac without the Automation permission remctl's flag needs."""
    monkeypatch.setenv("REMTUI_FAKE_FLAG_FAILS", "1")


async def test_add_reports_flag_failure_as_warning(client: RemctlClient, failing_flags):
    result = await client.add("Created but unflagged", list_title="Home", flagged=True)
    # The reminder still exists: a failed flag write is not a failed add.
    assert result["status"] == "created"
    added = next(
        r for r in await client.get_reminders("Home") if r.id == result["numericId"]
    )
    assert not added.flagged
    assert any(w.startswith("flag_not_set:") for w in warnings_of(result))


async def test_flag_failure_raises_instead_of_faking_success(
    client: RemctlClient, failing_flags
):
    new_id = (await client.add("Cannot flag", list_title="Home"))["numericId"]
    with pytest.raises(RemctlError) as excinfo:
        await client.flag(new_id)
    assert excinfo.value.code == "applescript_flag_failed"


async def test_edit_keeps_field_changes_when_flag_write_fails(
    client: RemctlClient, failing_flags
):
    new_id = (await client.add("Partial edit", list_title="Home"))["numericId"]
    with pytest.raises(RemctlError):
        await client.edit(new_id, title="Renamed anyway", flagged=True)
    # The field edit ran first, so it survives the flag failure.
    edited = next(r for r in await client.get_reminders("Home") if r.id == new_id)
    assert edited.title == "Renamed anyway"
    assert not edited.flagged


def test_warnings_of_tolerates_odd_payloads():
    assert warnings_of(None) == []
    assert warnings_of({"status": "created"}) == []
    assert warnings_of({"warnings": "single"}) == ["single"]
    assert warnings_of({"warnings": ["a", "", None, "b"]}) == ["a", "b"]


# ==================== Resolving the binary, in one place =====================
#
# A host embedding `RemindersPanel` needs the same answer remtui's own app needs.
# librarian re-derived it and got it wrong: `RemctlClient()` takes the literal
# "remctl", so anyone who set $REMTUI_REMCTL had it ignored inside the embed
# while remtui standalone honoured it.


def test_the_env_var_names_the_binary(monkeypatch):
    monkeypatch.setenv("REMTUI_REMCTL", "/opt/custom/remctl")
    from remtui.client import resolve_remctl

    assert resolve_remctl() == "/opt/custom/remctl"


def test_without_the_env_var_it_is_remctl_on_path(monkeypatch):
    monkeypatch.delenv("REMTUI_REMCTL", raising=False)
    from remtui.client import resolve_remctl

    assert resolve_remctl() == "remctl"


def test_an_empty_env_var_is_not_a_binary_name(monkeypatch):
    """An exported-but-empty variable means "unset", not a command of ""."""
    monkeypatch.setenv("REMTUI_REMCTL", "   ")
    from remtui.client import resolve_remctl

    assert resolve_remctl() == "remctl"


def test_found_reports_on_the_resolved_binary(monkeypatch):
    """So a host can choose a handoff *before* mounting a panel that cannot work."""
    from remtui.client import remctl_found

    monkeypatch.setenv("REMTUI_REMCTL", "/definitely/not/here/remctl")
    assert remctl_found() is False

    monkeypatch.setenv("REMTUI_REMCTL", "sh")  # something certain to exist
    assert remctl_found() is True
