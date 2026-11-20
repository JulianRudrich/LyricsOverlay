import assert from "node:assert/strict";
import { test } from "node:test";
import { currentIndex, momentAt, nowFile, positionAt, shorten, split, type Now } from "../singline.ts";

function playing(position: number, isPlaying = true): Now {
  return {
    version: 1,
    updatedAt: 10_000,
    player: "Spotify",
    title: "Lady of Namek",
    artist: "Tory Lanez",
    album: null,
    duration: 194,
    playing: isPlaying,
    position,
    at: 10_000,
    rate: 1,
    offset: 0,
    lyrics: {
      sync: "word",
      instrumental: false,
      lines: [
        {
          time: 10,
          text: "So, can we?",
          words: [
            { start: 10, text: "So, " },
            { start: 10.6, text: "can " },
            { start: 11.2, text: "we?" },
          ],
        },
        { time: 12, text: "Play with the stars tonight?" },
        { time: 15, text: "" },
        { time: 18, text: "Can we make them all align?" },
      ],
    },
  };
}

test("finds now.json where Singline keeps it", () => {
  const id = "io.github.julianrudrich.singline";
  assert.equal(nowFile({}, "darwin", "/Users/julian"), `/Users/julian/Library/Caches/${id}/now.json`);
  assert.equal(nowFile({}, "linux", "/home/julian"), `/home/julian/.cache/${id}/now.json`);
  assert.equal(nowFile({ XDG_CACHE_HOME: "/tmp/cache" }, "linux", "/home/julian"), `/tmp/cache/${id}/now.json`);
  assert.equal(nowFile({ XDG_CACHE_HOME: "relative" }, "linux", "/home/julian"), `/home/julian/.cache/${id}/now.json`);
  assert.equal(nowFile({ SINGLINE_DATA_DIR: "/data" }, "linux", "/home/julian"), "/data/now.json");
  assert.equal(nowFile({ SINGLINE_NOW_FILE: "/x/now.json" }, "darwin", "/Users/julian"), "/x/now.json");
});

test("moves the position on while the song plays, and stops at its end", () => {
  assert.equal(positionAt(playing(11), 12_000), 13);
  assert.equal(positionAt(playing(11, false), 12_000), 11);
  assert.equal(positionAt(playing(190), 20_000), 194);
});

test("finds the line that has started", () => {
  const lines = playing(0).lyrics!.lines;
  assert.equal(currentIndex(lines, 9), null);
  assert.equal(currentIndex(lines, 10), 0);
  assert.equal(currentIndex(lines, 13), 1);
  assert.equal(currentIndex(lines, 100), 3);
});

test("lights up timed words one by one", () => {
  const line = playing(0).lyrics!.lines[0];
  assert.deepEqual(split(line, 12, 10.7), { sung: "So, can ", rest: "we?" });
  assert.deepEqual(split(line, 12, 11.5), { sung: "So, can we?", rest: "" });
});

test("estimates the words of a line without word timing", () => {
  const line = playing(0).lyrics!.lines[1];
  assert.deepEqual(split(line, 15, 12), { sung: "", rest: "Play with the stars tonight?" });
  assert.deepEqual(split(line, 15, 13), { sung: "Play with ", rest: "the stars tonight?" });
  assert.deepEqual(split(line, 15, 13.5), { sung: "Play with the ", rest: "stars tonight?" });
  assert.deepEqual(split(line, 15, 15), { sung: "Play with the stars tonight?", rest: "" });
});

test("tells what plays at a moment", () => {
  assert.deepEqual(momentAt(playing(10.7), 10_000), {
    title: "Lady of Namek",
    artist: "Tory Lanez",
    playing: true,
    sung: "So, can ",
    rest: "we?",
  });
  // Before the first line and in a break: only the song.
  assert.equal(momentAt(playing(5), 10_000)?.rest, "");
  assert.equal(momentAt(playing(16), 10_000)?.sung, "");
  // The correction shows lines earlier.
  const early = { ...playing(11.7), offset: 0.5 };
  assert.equal(momentAt(early, 10_000)?.rest, "Play with the stars tonight?");
});

test("says nothing for an old file, a newer format or no song", () => {
  assert.equal(momentAt(playing(11), 31_000), null);
  assert.equal(momentAt({ ...playing(11), version: 2 }, 10_000), null);
  assert.equal(momentAt({ ...playing(11), title: null }, 10_000), null);
});

test("lyrics without timing show only the song", () => {
  const now = playing(11);
  const plain = { ...now, lyrics: { ...now.lyrics!, sync: "unsynced" as const } };
  assert.deepEqual(momentAt(plain, 10_000), { title: "Lady of Namek", artist: "Tory Lanez", playing: true, sung: "", rest: "" });
});

test("shortens long lines with an ellipsis", () => {
  assert.equal(shorten("short", 10), "short");
  assert.equal(shorten("a long line that goes on", 10), "a long li…");
});
