#!/usr/bin/env python3
"""A pretend Genius API for the Sing to find end-to-end check.

Answers /search for words of "Morning Harbor" with two songs, a decoy first,
so the app has to pick the song whose lyrics explain what it heard. Requires
the token the check gives the app. Every request is logged to stderr.
"""

import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

TOKEN = "e2e-token"


def hit(title, artist):
    return {
        "type": "song",
        "index": "song",
        "result": {"title": title, "full_title": f"{title} by {artist}", "primary_artist": {"name": artist}},
    }


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        url = urlparse(self.path)
        query = {key: values[0] for key, values in parse_qs(url.query).items()}
        authorization = self.headers.get("Authorization", "")
        print(json.dumps({"path": url.path, "query": query, "authorization": authorization}), file=sys.stderr, flush=True)
        if authorization != f"Bearer {TOKEN}":
            return self.reply(401, {"meta": {"status": 401, "message": "invalid token"}})
        if url.path != "/search":
            return self.reply(404, {"meta": {"status": 404}})
        words = query.get("q", "").lower()
        hits = []
        if any(word in words for word in ("morning", "mountain", "window", "lantern", "swallows", "valley")):
            hits = [hit("City Morning", "Decoy Band"), hit("Morning Harbor", "Example Artist")]
        self.reply(200, {"meta": {"status": 200}, "response": {"hits": hits}})

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
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 18767
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
