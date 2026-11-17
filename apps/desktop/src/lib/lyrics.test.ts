import { describe, expect, it } from "vitest";
import { currentIndex, lineText, wordFills } from "./lyrics";
import type { LyricLine } from "./types";

const lines: LyricLine[] = [
  { time: 1, text: "First" },
  { time: 3.5, text: "Second" },
  { time: 7, text: "Again" },
  { time: 9.1, text: "Again" },
  { time: 12, text: "" },
];

describe("currentIndex", () => {
  it("matches the Rust core", () => {
    expect(currentIndex(lines, 0.99)).toBeNull();
    expect(currentIndex(lines, 1)).toBe(0);
    expect(currentIndex(lines, 8)).toBe(2);
    expect(currentIndex(lines, 12)).toBe(4);
    expect(currentIndex([], 0)).toBeNull();
  });

  it("takes the last of equal timestamps", () => {
    expect(currentIndex([{ time: 1, text: "A" }, { time: 1, text: "B" }], 1)).toBe(1);
  });
});

describe("wordFills", () => {
  const line: LyricLine = {
    time: 10,
    text: "Hello bright world",
    words: [
      { start: 10, end: 10.6, text: "Hello " },
      { start: 10.6, text: "bright " },
      { start: 11.2, text: "world" },
    ],
  };

  it("fills words as they are sung", () => {
    const fills = wordFills(line, 14, 10.9).map((word) => Number(word.fill.toFixed(2)));
    expect(fills).toEqual([1, 0.5, 0]);
  });

  it("ends the last word within a second and a half", () => {
    const [, , last] = wordFills(line, 20, 11.95);
    expect(last?.fill).toBeCloseTo(0.5);
  });

  it("returns nothing for lines without word timing", () => {
    expect(wordFills({ time: 1, text: "Plain" }, null, 2)).toEqual([]);
  });

  it("shows a note for breaks", () => {
    expect(lineText({ time: 1, text: "" })).toBe("♪");
    expect(lineText(undefined)).toBe("");
  });
});
