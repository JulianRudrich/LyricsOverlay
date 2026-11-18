// Run with: node --test extensions/browser/test
"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fixture = require("./fixtures/update.json");
const { sanitizeTab, buildUpdate, parseCommands, normalizeCode, endpoint, PORTS } = require("../lib/protocol.js");

const youtubeTab = {
  url: "https://www.youtube.com/watch?v=abc",
  pageTitle: "Example Artist - Paper Lanterns (Official Video) - YouTube",
  title: "Example Artist - Paper Lanterns (Official Video)",
  artist: "Example ArtistVEVO",
  album: "",
  artwork: "https://i.ytimg.com/vi/abc/maxresdefault.jpg",
  currentTime: 30.5,
  duration: 200,
  playbackRate: 1,
  paused: false,
  ended: false,
  sampledAt: 1794986234669,
  controls: { playPause: true, next: true, previous: false, seek: true },
};

test("an update has the shape the app reads", () => {
  // The Rust side parses the same file, so both ends agree on the format.
  assert.deepEqual(buildUpdate([sanitizeTab(12, youtubeTab)], [7]), fixture);
});

test("tabs are cleaned before they leave the browser", () => {
  const tab = sanitizeTab(3, {
    url: "javascript:alert(1)",
    title: `  ${"x".repeat(600)}  `,
    artist: 42,
    artwork: "data:image/png;base64,AAAA",
    currentTime: Number.NaN,
    duration: Number.POSITIVE_INFINITY,
    playbackRate: 100,
    paused: "no",
    sampledAt: -5,
    controls: { next: "yes", seek: true },
  });
  assert.equal(tab.url, "", "only https pages");
  assert.equal(tab.title.length, 512);
  assert.equal(tab.artist, "");
  assert.equal(tab.artwork, "", "no data URLs");
  assert.equal(tab.currentTime, null);
  assert.equal(tab.duration, null);
  assert.equal(tab.playbackRate, null);
  assert.equal(tab.paused, true, "anything but false counts as paused");
  assert.equal(tab.sampledAt, null);
  assert.deepEqual(tab.controls, { playPause: false, next: false, previous: false, seek: true });
  assert.equal(sanitizeTab(-1, youtubeTab), null);
  assert.equal(sanitizeTab(1.5, youtubeTab), null);
  assert.equal(sanitizeTab(1, null), null);
  assert.equal(Array.from(sanitizeTab(1, { title: "😀".repeat(600) }).title).length, 512, "counts characters");
});

test("updates are capped", () => {
  const tabs = Array.from({ length: 40 }, (_, index) => sanitizeTab(index, youtubeTab));
  const update = buildUpdate(tabs, [1, -2, 2.5, 3]);
  assert.equal(update.tabs.length, 32);
  assert.deepEqual(update.closed, [1, 3]);
});

test("only known commands reach a tab", () => {
  const commands = parseCommands({
    commands: [
      { tabId: 12, command: { kind: "playPause" } },
      { tabId: 12, command: { kind: "seekTo", position: 42.5 } },
      { tabId: 12, command: { kind: "seekTo" } },
      { tabId: 12, command: { kind: "openUrl", url: "https://example.com" } },
      { tabId: "12", command: { kind: "next" } },
      { tabId: 4, command: { kind: "pause" } },
    ],
  });
  assert.deepEqual(
    commands.map(({ tabId, command }) => [tabId, command.kind]),
    [
      [12, "playPause"],
      [12, "seekTo"],
      [4, "pause"],
    ],
  );
  assert.deepEqual(parseCommands(null), []);
  assert.deepEqual(parseCommands({ commands: "all" }), []);
});

test("pairing codes may be typed with spaces", () => {
  assert.equal(normalizeCode("482 913"), "482913");
  assert.equal(normalizeCode(" 482-913 "), "482913");
  assert.equal(normalizeCode("48291"), null);
  assert.equal(normalizeCode("48291a"), null);
  assert.equal(normalizeCode(482913), null);
});

test("the app is only ever reached on this computer", () => {
  assert.equal(endpoint(17464, "/v1/hello"), "http://127.0.0.1:17464/v1/hello");
  assert.deepEqual(PORTS, [17464, 17465, 17466]);
});
