#!/usr/bin/env bash
# End-to-end check on Linux: starts the real app on a virtual display with a
# pretend Spotify (MPRIS) and a pretend LRCLIB, then checks that the overlay
# shows the line that matches the playback time, and saves a screenshot.
#
# Needs: xvfb-run, dbus-run-session, python3, ImageMagick (import).
# Build first:
#   pnpm --filter lyrics-overlay-desktop tauri build --debug --no-bundle
#   cargo build -p media-session --example fake_mpris_player
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
APP="${APP:-$ROOT/target/debug/lyrics-overlay}"
PLAYER="${PLAYER:-$ROOT/target/debug/examples/fake_mpris_player}"
OUT="${OUT:-$ROOT/target/e2e}"
PORT="${PORT:-18765}"
START=30

rm -rf "$OUT" && mkdir -p "$OUT/data"
export WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1
export LANG=en_US.UTF-8 LYRICS_OVERLAY_DEBUG=1
export LYRICS_OVERLAY_DATA_DIR="$OUT/data" LYRICS_OVERLAY_LRCLIB_URL="http://127.0.0.1:$PORT"
export APP PLAYER OUT PORT START HERE="$ROOT/apps/desktop/e2e"

timeout 120 dbus-run-session -- xvfb-run -a -s "-screen 0 1280x800x24" bash -c '
  set -u
  xsetroot -solid "#3b5a73" 2>/dev/null || true
  python3 "$HERE/mock_lrclib.py" "$PORT" 2> "$OUT/lrclib.log" &
  "$PLAYER" "Paper Lanterns" "Example Artist" 200 "$START" > "$OUT/player.log" 2>&1 &
  date +%s.%N > "$OUT/player-started"
  sleep 1
  "$APP" > "$OUT/app.log" 2>&1 &
  app=$!
  for _ in $(seq 60); do
    grep -q "^overlay-line" "$OUT/app.log" 2>/dev/null && break
    sleep 0.5
  done
  sleep 3.2
  import -window root "$OUT/screen.png" || true
  kill "$app" 2>/dev/null; wait "$app" 2>/dev/null
  kill $(jobs -p) 2>/dev/null || true
' 2>&1 | grep -v "fd limit" || true

python3 - "$OUT" "$START" <<'PY'
import json, math, pathlib, re, sys

out, start = pathlib.Path(sys.argv[1]), int(sys.argv[2])
failures = []

requests = [json.loads(line) for line in (out / "lrclib.log").read_text().splitlines() if line.startswith("{")]
exact = [r for r in requests if r["path"] == "/api/get"]
if not exact:
    failures.append("the app never asked LRCLIB for the song")
elif exact[0]["query"] != {"track_name": "Paper Lanterns", "artist_name": "Example Artist", "album_name": "Example Album", "duration": "200"}:
    failures.append(f"unexpected LRCLIB query: {exact[0]['query']}")

reports = re.findall(r"^overlay-line (-?[0-9.]+) (.*)$", (out / "app.log").read_text(), re.M)
if len(reports) < 2:
    failures.append(f"the overlay showed {len(reports)} line(s); expected at least 2")
for position, text in reports:
    expected = f"Line {math.floor(float(position))}"
    if text != expected:
        failures.append(f"at {position} s the overlay showed {text!r}, expected {expected!r}")
if reports:
    first = math.floor(float(reports[0][0]))
    if not start <= first <= start + 25:
        failures.append(f"the first line came at {first} s; the player started at {start} s")

print(f"LRCLIB requests: {len(requests)}, overlay lines: {len(reports)}")
for position, text in reports[:8]:
    print(f"  {position:>7} s  {text}")
if failures:
    print("FAILED:\n  " + "\n  ".join(failures))
    print("--- app.log ---\n" + (out / "app.log").read_text()[-3000:])
    sys.exit(1)
print(f"OK: the overlay follows the player. Screenshot: {out / 'screen.png'}")
PY
