"""Integration tests: RemctlClient driving the fake remctl subprocess."""

import pytest

from remtui.client import RemctlClient, RemctlError


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
