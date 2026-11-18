import type { LyricLine, LyricWord, SyncLevel } from "./types";

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

/** Seconds per character of a line without word timing, counting one space per word. */
const SECONDS_PER_CHARACTER = 0.07;
/** Even a short line is sung over at least this many seconds. */
const SHORTEST_LINE = 0.6;
/** A line is sung within this share of the time until the next line starts. */
const SHARE_OF_GAP = 0.9;
/** Time allowed for the last line, which has no next line to end it. */
const LAST_LINE_GAP = 6;

/**
 * Estimated word timing for a line that only has a start time: about 0.07 s
 * per character, at least 0.6 s, at most 90 % of the gap to the next line,
 * shared out by word length. The words keep their spaces, so joined they give
 * the line's text back.
 */
export function estimatedWords(line: LyricLine, nextLineTime: number | null): LyricWord[] {
  const pieces = line.text.match(/\s*\S+\s*/g) ?? [];
  const weights = pieces.map((piece) => Array.from(piece.trim()).length + 1);
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  if (total === 0) return [];
  const end = line.end !== undefined && line.end > line.time ? line.end : (nextLineTime ?? line.time + LAST_LINE_GAP);
  const sung = Math.min(Math.max(0, end - line.time) * SHARE_OF_GAP, Math.max(SHORTEST_LINE, total * SECONDS_PER_CHARACTER));
  let start = line.time;
  return pieces.map((text, index) => {
    const length = (sung * (weights[index] ?? 0)) / total;
    const word = { start, end: start + length, text };
    start += length;
    return word;
  });
}

/**
 * The word highlight for a line: its real word timing when it has some,
 * otherwise an estimate for synced lyrics. Plain lyrics have no timing worth
 * following and get none, so their lines light up as a whole.
 */
export function lineFills(line: LyricLine, nextLineTime: number | null, position: number, sync: SyncLevel): WordFill[] {
  if (line.words && line.words.length > 0) return wordFills(line, nextLineTime, position);
  if (sync === "unsynced") return [];
  return wordFills({ ...line, words: estimatedWords(line, nextLineTime) }, nextLineTime, position);
}

/** Display text for a line; an empty line is an instrumental break. */
export function lineText(line: LyricLine | undefined): string {
  if (!line) return "";
  return line.text.length > 0 ? line.text : "♪";
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
