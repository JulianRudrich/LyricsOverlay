#!/usr/bin/env python3
"""A pretend Singline browser extension for the Linux end-to-end check.

Waits for the pairing code the app prints with SINGLINE_DEBUG, pairs with the
browser bridge, checks that strangers are turned away, and then reports one
playing tab twice a second, as the real extension does. Every step is logged
to stdout as one JSON object per line.

Usage: fake_extension.py PORT APP_LOG URL TITLE ARTIST ALBUM SECONDS START
"""

import http.client
import json
import re
import sys
import time

port, app_log = int(sys.argv[1]), sys.argv[2]
url, title, artist, album = sys.argv[3:7]
duration, start = float(sys.argv[7]), float(sys.argv[8])


def log(**event):
    print(json.dumps(event), flush=True)


def call(method, path, body=None, token=None, origin="chrome-extension://singline-e2e"):
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    headers = {"Content-Type": "application/json"}
    if origin:
        headers["Origin"] = origin
    if token:
        headers["Authorization"] = f"Bearer {token}"
    connection.request(method, path, None if body is None else json.dumps(body), headers)
    response = connection.getresponse()
    data = response.read()
    connection.close()
    return response.status, json.loads(data) if data else None


code = None
for _ in range(240):
    try:
        found = re.search(r"^bridge-pairing-code (\d{6})$", open(app_log).read(), re.M)
    except FileNotFoundError:
        found = None
    if found:
        code = found.group(1)
        break
    time.sleep(0.25)
if code is None:
    log(event="noCode")
    sys.exit(1)

for _ in range(40):
    try:
        status, hello = call("GET", "/v1/hello")
        break
    except OSError:
        time.sleep(0.25)
log(event="hello", status=status, app=(hello or {}).get("app"))
log(event="webPage", status=call("GET", "/v1/hello", origin="https://evil.example")[0])
log(event="stranger", status=call("POST", "/v1/update", {"tabs": []}, token="0" * 64)[0])
wrong = "000000" if code != "000000" else "111111"
log(event="wrongCode", status=call("POST", "/v1/pair", {"code": wrong, "browser": "Chrome"})[0])
status, answer = call("POST", "/v1/pair", {"code": code, "browser": "Chrome"})
log(event="paired", status=status)
token = answer["token"]

began = time.time()
while True:
    now = time.time()
    tab = {
        "tabId": 12,
        "url": url,
        "title": title,
        "artist": artist,
        "album": album,
        "artwork": "",
        "pageTitle": f"{title} - YouTube",
        "currentTime": start + (now - began),
        "duration": duration,
        "playbackRate": 1,
        "paused": False,
        "ended": False,
        "sampledAt": now * 1000,
        "controls": {"playPause": True, "next": True, "previous": False, "seek": True},
    }
    try:
        status, answer = call("POST", "/v1/update", {"tabs": [tab]}, token)
        log(event="update", status=status, commands=len((answer or {}).get("commands", [])))
    except OSError as error:
        log(event="updateFailed", error=str(error))
    time.sleep(0.5)
