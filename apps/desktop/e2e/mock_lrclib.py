#!/usr/bin/env python3
"""A pretend LRCLIB for the Linux end-to-end checks.

Answers /api/get and /api/search for "Paper Lanterns", whose synced lyrics have
one line per second ("Line 0", "Line 1", ...), so the line the overlay shows
tells the playback time it believes in. For Sing to find it also knows
"Morning Harbor", with real lines every four seconds, and a decoy that shares
some of its words. Every request is logged to stderr.
"""

import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

TITLE = "Paper Lanterns"
ARTIST = "Example Artist"
ALBUM = "Example Album"
DURATION = 200

SYNCED = "\n".join(f"[{s // 60:02d}:{s % 60:02d}.00]Line {s}" for s in range(DURATION))
RECORD = {
    "id": 1,
    "trackName": TITLE,
    "artistName": ARTIST,
    "albumName": ALBUM,
    "duration": DURATION,
    "instrumental": False,
    "plainLyrics": None,
    "syncedLyrics": SYNCED,
}

# The song sung in linux-sing.sh, one line every four seconds.
HARBOR_LINES = [
    "",
    "The river carries every name we gave the summer",
    "Paper boats are sailing past the silent harbor",
    "I can hear the morning calling from the mountain",
    "Every window opens when the swallows come home",
    "Hold the lantern higher so the travelers find us",
    "We will sing until the valley answers back",
    "Golden fields are waiting on the other side",
    "Nobody remembers how the story started",
    "But the morning keeps on calling from the mountain",
    "And the river carries every name we gave",
    "",
]
DECOY_LINES = ["Good morning to the city", "The mountain is calling me", "Every morning I can hear the traffic"]


def record(number, title, artist, album, duration, lines, spacing):
    synced = "\n".join(f"[{int(i * spacing) // 60:02d}:{int(i * spacing) % 60:02d}.00]{line}" for i, line in enumerate(lines))
    return {
        "id": number,
        "trackName": title,
        "artistName": artist,
        "albumName": album,
        "duration": duration,
        "instrumental": False,
        "plainLyrics": None,
        "syncedLyrics": synced,
    }


SEARCHABLE = [
    record(2, "Morning Harbor", ARTIST, "Harbor Songs", 60, HARBOR_LINES, 4),
    record(3, "City Morning", "Decoy Band", "Traffic", 30, DECOY_LINES, 4),
]


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        url = urlparse(self.path)
        query = {key: values[0] for key, values in parse_qs(url.query).items()}
        print(json.dumps({"path": url.path, "query": query}), file=sys.stderr, flush=True)
        known = query.get("track_name") == TITLE and query.get("artist_name") == ARTIST
        if url.path == "/api/get":
            self.reply(200 if known else 404, RECORD if known else {"code": 404, "name": "TrackNotFound"})
        elif url.path == "/api/search":
            others = [r for r in SEARCHABLE if (r["trackName"], r["artistName"]) == (query.get("track_name"), query.get("artist_name"))]
            self.reply(200, [RECORD] if known else others)
        else:
            self.reply(404, {"code": 404})

    def reply(self, status, body):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 18765
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
