import { describe, expect, it } from "vitest";
import { splitLongLines } from "./segment";
import type { LyricLine } from "./types";

/** Ten pixels per character. */
const measure = (text: string) => text.length * 10;

describe("splitLongLines", () => {
  it("leaves lines that fit alone", () => {
    const lines: LyricLine[] = [
      { time: 1, text: "Short line" },
      { time: 4, text: "" },
    ];
    const result = splitLongLines(lines, 60, 300, measure);
    expect(result).toEqual(lines);
    expect(result[0]).toBe(lines[0]);
  });

  it("splits a long line at the most even word boundary", () => {
    const lines: LyricLine[] = [
      { time: 10, text: "one two three four five six seven" },
      { time: 20, text: "Next" },
    ];
    const [head, tail, next] = splitLongLines(lines, 60, 300, measure);
    expect(head).toEqual({ time: 10, text: "one two three four" });
    expect(tail?.text).toBe("five six seven");
    // The estimate gives 0.07 s per character and space, like the word highlight.
    expect(tail?.time).toBeCloseTo(10 + (34 * 0.07 * 19) / 34);
    expect(next).toBe(lines[1]);
  });

  it("follows word timing and keeps syllables of one word together", () => {
    const line: LyricLine = {
      time: 10,
      text: "Leise zieht die Nacht vorbei",
      words: [
        { start: 10, text: "Leise " },
        { start: 10.7, text: "zieht " },
        { start: 11.3, text: "die " },
        { start: 11.8, text: "Nacht " },
        { start: 12.4, text: "vor" },
        { start: 12.9, text: "bei" },
      ],
    };
    const [head, tail] = splitLongLines([line], 20, 200, measure);
    expect(head?.text).toBe("Leise zieht die");
    expect(head?.words?.map((word) => word.text)).toEqual(["Leise ", "zieht ", "die "]);
    expect(tail?.time).toBe(11.8);
    expect(tail?.text).toBe("Nacht vorbei");
    expect(tail?.words?.map((word) => word.start)).toEqual([11.8, 12.4, 12.9]);
  });

  it("keeps the end of the line on its second row", () => {
    const [, tail] = splitLongLines([{ time: 0, end: 5, text: "aaaa bbbb cccc dddd" }], 10, 100, measure);
    expect(tail?.end).toBe(5);
  });

  it("cannot split one word, a line without time, or words that all start together", () => {
    expect(splitLongLines([{ time: 0, text: "Supercalifragilistic" }], 10, 100, measure)).toHaveLength(1);
    expect(splitLongLines([{ time: 10, text: "aaaa bbbb cccc dddd" }], 10, 100, measure)).toHaveLength(1);
    const together: LyricLine = {
      time: 0,
      text: "aaaa bbbb cccc",
      words: [
        { start: 0, text: "aaaa " },
        { start: 0, text: "bbbb " },
        { start: 0, text: "cccc" },
      ],
    };
    expect(splitLongLines([together], 10, 100, measure)).toEqual([together]);
  });
});
