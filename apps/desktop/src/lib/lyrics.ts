import type { LyricLine } from "./types";

/** Index of the line active at `position`, or null before the first line. */
export function currentIndex(lines: readonly LyricLine[], position: number): number | null {
  let lower = 0;
  let upper = lines.length;
  while (lower < upper) {
    const middle = (lower + upper) >>> 1;
    if ((lines[middle]?.time ?? Infinity) <= position) lower = middle + 1;
    else upper = middle;
  }
  return lower === 0 ? null : lower - 1;
}

export interface WordFill {
  text: string;
  /** How much of the word has been sung, from 0 to 1. */
  fill: number;
}

const LONGEST_WORD = 1.5;

/**
 * How far each timed word of a line has been sung. Lines without word timing
 * return an empty list and are highlighted as a whole.
 */
export function wordFills(line: LyricLine, nextLineTime: number | null, position: number): WordFill[] {
  const words = line.words ?? [];
  return words.map((word, index) => {
    const following = words[index + 1]?.start;
    const lineEnd = line.end ?? nextLineTime ?? Infinity;
    const end = word.end ?? following ?? Math.min(lineEnd, word.start + LONGEST_WORD);
    const length = Math.max(0.05, end - word.start);
    return { text: word.text, fill: clamp((position - word.start) / length, 0, 1) };
  });
}

/** Display text for a line; an empty line is an instrumental break. */
export function lineText(line: LyricLine | undefined): string {
  if (!line) return "";
  return line.text.length > 0 ? line.text : "♪";
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
