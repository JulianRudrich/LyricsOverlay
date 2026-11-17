#!/usr/bin/env python3
"""A pretend LRCLIB for the Linux end-to-end check.

Answers /api/get and /api/search for one song whose synced lyrics have one line
per second ("Line 0", "Line 1", ...), so the line the overlay shows tells the
playback time it believes in. Every request is logged to stderr.
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


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        url = urlparse(self.path)
        query = {key: values[0] for key, values in parse_qs(url.query).items()}
        print(json.dumps({"path": url.path, "query": query}), file=sys.stderr, flush=True)
        known = query.get("track_name") == TITLE and query.get("artist_name") == ARTIST
        if url.path == "/api/get":
            self.reply(200 if known else 404, RECORD if known else {"code": 404, "name": "TrackNotFound"})
        elif url.path == "/api/search":
            self.reply(200, [RECORD] if known else [])
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
