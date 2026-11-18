#!/usr/bin/env bash
# End-to-end check of YouTube on Linux, with the real app on a virtual display
# and a pretend LRCLIB:
#   1. a pretend Chromium tab (MPRIS) plays a YouTube video titled
#      "Example Artist - Paper Lanterns (Official Video)" on "Example ArtistVEVO";
#   2. a pretend browser extension pairs with the app and reports a YouTube
#      Music tab through the browser bridge.
# Both times the app must ask LRCLIB for the song, not the video, and the
# overlay must show the line that matches the playback time.
#
# Needs: xvfb-run, dbus-run-session, python3.
# Build first:
#   pnpm --filter singline-desktop tauri build --debug --no-bundle
#   cargo build -p media-session --example fake_mpris_player
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
APP="${APP:-$ROOT/target/debug/singline}"
PLAYER="${PLAYER:-$ROOT/target/debug/examples/fake_mpris_player}"
OUT="${OUT:-$ROOT/target/e2e-youtube}"
PORT="${PORT:-18765}"
BRIDGE_PORT="${BRIDGE_PORT:-$((PORT + 1))}"
HERE="$ROOT/apps/desktop/e2e"

rm -rf "$OUT" && mkdir -p "$OUT"
export WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1
export LANG=en_US.UTF-8 SINGLINE_DEBUG=1 SINGLINE_LRCLIB_URL="http://127.0.0.1:$PORT" SINGLINE_BRIDGE_PORT="$BRIDGE_PORT"
export APP PLAYER PORT BRIDGE_PORT HERE

# Runs the app once. $1 names the run; the rest is the pretend source to start.
run() {
  local name="$1"
  shift
  mkdir -p "$OUT/$name/data"
  # A YouTube video is a web player like any other, so it needs this setting.
  [ "$name" = "video" ] && echo '{"includeOtherPlayers": true}' > "$OUT/$name/data/settings.json"
  RUN="$OUT/$name" SINGLINE_DATA_DIR="$OUT/$name/data" timeout 90 dbus-run-session -- xvfb-run -a -s "-screen 0 1280x800x24" bash -c '
    set -u
    python3 "$HERE/mock_lrclib.py" "$PORT" 2> "$RUN/lrclib.log" &
    "$@" > "$RUN/source.log" 2>&1 &
    sleep 1
    "$APP" > "$RUN/app.log" 2>&1 &
    app=$!
    for _ in $(seq 60); do
      grep -q "^overlay-line" "$RUN/app.log" 2>/dev/null && break
      sleep 0.5
    done
    sleep 3.2
    kill "$app" 2>/dev/null; wait "$app" 2>/dev/null
    kill $(jobs -p) 2>/dev/null || true
  ' bash "$@" 2>&1 | grep -v "fd limit" || true
}

VIDEO="Example Artist - Paper Lanterns (Official Video)"
run video "$PLAYER" --bus chromium.instance42 --identity Chromium --url "https://www.youtube.com/watch?v=e2e" "$VIDEO" "Example ArtistVEVO" 200 30
run bridge python3 "$HERE/fake_extension.py" "$BRIDGE_PORT" "$OUT/bridge/app.log" \
  "https://music.youtube.com/watch?v=e2e" "Paper Lanterns" "Example Artist" "Example Album" 200 120

python3 - "$OUT" <<'PY'
import json, math, pathlib, re, sys

out = pathlib.Path(sys.argv[1])
failures = []

def check(name, start):
    run = out / name
    requests = [json.loads(line) for line in (run / "lrclib.log").read_text().splitlines() if line.startswith("{")]
    exact = [r for r in requests if r["path"] == "/api/get"]
    if not exact:
        failures.append(f"{name}: the app never asked LRCLIB for the song")
    else:
        query = exact[0]["query"]
        song = (query.get("track_name"), query.get("artist_name"), query.get("duration"))
        if song != ("Paper Lanterns", "Example Artist", "200"):
            failures.append(f"{name}: unexpected LRCLIB query: {query}")
    reports = re.findall(r"^overlay-line (-?[0-9.]+) (.*)$", (run / "app.log").read_text(), re.M)
    if len(reports) < 2:
        failures.append(f"{name}: the overlay showed {len(reports)} line(s); expected at least 2")
    for position, text in reports:
        if text != f"Line {math.floor(float(position))}":
            failures.append(f"{name}: at {position} s the overlay showed {text!r}")
    if reports and not start <= math.floor(float(reports[0][0])) <= start + 25:
        failures.append(f"{name}: the first line came at {reports[0][0]} s; playback started at {start} s")
    print(f"{name}: LRCLIB query {exact[0]['query'] if exact else None}, overlay lines: {len(reports)}")
    for position, text in reports[:4]:
        print(f"  {position:>7} s  {text}")
    return run

check("video", 30)
bridge = check("bridge", 120)
events = [json.loads(line) for line in (bridge / "source.log").read_text().splitlines() if line.startswith("{")]
status = {event["event"]: event.get("status") for event in events}
expected = {"hello": 200, "webPage": 403, "stranger": 401, "wrongCode": 403, "paired": 200, "update": 200}
for event, code in expected.items():
    if status.get(event) != code:
        failures.append(f"bridge: {event} answered {status.get(event)}, expected {code}")
print(f"bridge: {sum(1 for e in events if e['event'] == 'update')} updates, answers {status}")

if failures:
    print("FAILED:\n  " + "\n  ".join(failures))
    for name in ("video", "bridge"):
        print(f"--- {name}/app.log ---\n" + (out / name / "app.log").read_text()[-2000:])
    sys.exit(1)
print("OK: YouTube videos and YouTube Music tabs get the song's lyrics.")
PY
