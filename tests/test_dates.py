from datetime import datetime

from remtui.dates import humanize_due, is_due_today, is_overdue, parse_due, sort_key

NOW = datetime(2026, 7, 3, 12, 0, 0)  # a Friday


def test_parse_due_date_only():
    assert parse_due("2026-07-03") == datetime(2026, 7, 3)


def test_parse_due_with_time():
    assert parse_due("2026-07-03 14:30") == datetime(2026, 7, 3, 14, 30)
    assert parse_due("2026-07-03T14:30:00") == datetime(2026, 7, 3, 14, 30)


def test_parse_due_empty_and_garbage():
    assert parse_due(None) is None
    assert parse_due("") is None
    assert parse_due("not a date") is None


def test_humanize_today_tomorrow_yesterday():
    assert humanize_due(datetime(2026, 7, 3, 14, 0), now=NOW) == "Today 14:00"
    assert humanize_due(datetime(2026, 7, 4), all_day=True, now=NOW) == "Tomorrow"
    assert humanize_due(datetime(2026, 7, 2), all_day=True, now=NOW) == "Yesterday"


def test_humanize_weekday_and_far_dates():
    assert humanize_due(datetime(2026, 7, 6, 9, 30), now=NOW) == "Mon 09:30"
    assert humanize_due(datetime(2026, 8, 15), all_day=True, now=NOW) == "Aug 15"
    assert humanize_due(datetime(2027, 1, 2), all_day=True, now=NOW) == "Jan 2, 2027"


def test_overdue_semantics():
    assert is_overdue(datetime(2026, 7, 3, 11, 0), now=NOW)
    assert not is_overdue(datetime(2026, 7, 3, 13, 0), now=NOW)
    # all-day today is not overdue until the day ends
    assert not is_overdue(datetime(2026, 7, 3), all_day=True, now=NOW)
    assert is_overdue(datetime(2026, 7, 2), all_day=True, now=NOW)
    # a *timed* reminder due at midnight today has passed by noon
    assert is_overdue(datetime(2026, 7, 3, 0, 0), all_day=False, now=NOW)
    assert not is_overdue(None, now=NOW)


def test_due_today():
    assert is_due_today(datetime(2026, 7, 3, 23, 0), now=NOW)
    assert not is_due_today(datetime(2026, 7, 4), now=NOW)
    assert not is_due_today(None, now=NOW)


def test_sort_key_orders_undated_last():
    dated = sort_key(datetime(2026, 7, 3))
    undated = sort_key(None)
    assert dated < undated
