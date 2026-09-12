#!/usr/bin/env bash
# Runs both implementations' headless benchmarks twice under caffeinate and
# records each process's own resident memory. `startup_to_first_frame_ms` is
# wall clock from the launch to the first frame, interpreter start included.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_GATE="$HERE/../target/release/remtui-gate"
PY_DIR="${REMTUI_PY_DIR:-$HOME/GitHub/remtui}"

run() {
    local label="$1"; shift
    echo "== $label"
    local launch
    launch=$(perl -MTime::HiRes -e 'print int(Time::HiRes::time()*1000)')
    local out
    out=$("$@")
    local frame
    frame=$(echo "$out" | sed -n 's/^first_frame_epoch_ms=//p')
    echo "$out" | grep -v '^first_frame_epoch_ms='
    echo "startup_to_first_frame_ms=$((frame - launch))"
}

for i in 1 2; do
    run "rust #$i" caffeinate -dimsu "$RUST_GATE" --bench
    (cd "$PY_DIR" && run "python #$i" caffeinate -dimsu "$PY_DIR/.venv/bin/python" "$HERE/bench_python.py")
done
echo "== sizes"
echo "rust_binary_bytes=$(stat -f %z "$HERE/../target/release/remtui")"
echo "python_venv_bytes=$(du -sk "$PY_DIR/.venv" | awk '{print $1 * 1024}')"
