"""Headless timings for the Python remtui, printed as the same key=value
lines `remtui-gate --bench` prints for the Rust build.

    cd ~/GitHub/remtui && uv run python ~/GitHub/remtui-rust/tools/bench_python.py
"""

from __future__ import annotations

import asyncio
import time

PROCESS_START = time.perf_counter()

from remtui.app import RemTuiApp  # noqa: E402
from remtui.client import RemctlClient, resolve_remctl  # noqa: E402


def ms(seconds: float) -> int:
    return int(round(seconds * 1000))


async def wait_until(pilot, pred, timeout=60.0):
    deadline = time.perf_counter() + timeout
    while time.perf_counter() < deadline:
        if pred():
            return
        await pilot.pause(0.02)
    raise TimeoutError


async def main() -> None:
    client = RemctlClient(resolve_remctl())
    app = RemTuiApp(client)
    async with app.run_test(size=(120, 40)) as pilot:
        panel = app.panel
        await wait_until(pilot, lambda: bool(panel.lists) and panel.reminders is not None and len(panel.query("ReminderItem")) >= 0)
        # the first view is loaded once the header shows a count
        await wait_until(pilot, lambda: not panel.query_one("#reminders").loading and bool(panel.lists))
        print("implementation=python")
        print(f"lists={len(panel.lists)}")
        print(f"first_frame_ms={ms(time.perf_counter() - PROCESS_START)}")
        print(f"first_frame_epoch_ms={int(time.time() * 1000)}")

        t = time.perf_counter()
        await pilot.press("r")
        await pilot.pause(0.05)
        await wait_until(pilot, lambda: not panel.query_one("#reminders").loading)
        print(f"warm_reload_ms={ms(time.perf_counter() - t)}")

        t = time.perf_counter()
        await client.get_lists()
        print(f"remctl_lists_ms={ms(time.perf_counter() - t)}")
        t = time.perf_counter()
        await client.today()
        print(f"remctl_today_ms={ms(time.perf_counter() - t)}")

        await pilot.press("l")
        samples = []
        for i in range(20):
            t = time.perf_counter()
            await pilot.press("j" if i % 2 == 0 else "k")
            samples.append(int((time.perf_counter() - t) * 1_000_000))
        samples.sort()
        print(f"keypress_frame_us={samples[len(samples) // 2]}")
        import resource
        print(f"rss_bytes={resource.getrusage(resource.RUSAGE_SELF).ru_maxrss}")


asyncio.run(main())
