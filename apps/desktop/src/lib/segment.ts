import type { LyricLine } from "./types";

/**
 * Splits each line that is wider than `maxWidth` into two rows, once, at the
 * word boundary that gives the most even halves. The second row starts when
 * its first word is sung: from the word timing where the line has it,
 * otherwise from the same estimate the word highlight uses. Lines that cannot
 * be split stay whole, and the lyrics themselves are left untouched.
 */
export function splitLongLines(
  lines: readonly LyricLine[],
  duration: number,
  maxWidth: number,
  measure: (text: string) => number,
): LyricLine[] {
  return lines.flatMap((line, index) => {
    if (measure(line.text) <= maxWidth) return [line];
    const end = lines[index + 1]?.time ?? duration;
    if (!(end > line.time)) return [line];
    const words = line.words && line.words.length > 1 ? line.words : null;
    const pieces = words ? timedPieces(line) : estimatedPieces(line.text, end - line.time);
    let split = -1;
    let evenest = Infinity;
    pieces.forEach((piece, at) => {
      if (at === 0 || piece.offset <= 0 || line.time + piece.offset >= end) return;
      const imbalance = Math.abs(measure(joined(pieces.slice(0, at))) - measure(joined(pieces.slice(at))));
      if (imbalance < evenest) {
        split = at;
        evenest = imbalance;
      }
    });
    const second = pieces[split];
    if (!second) return [line];
    const head: LyricLine = { time: line.time, text: joined(pieces.slice(0, split)) };
    const tail: LyricLine = { time: line.time + second.offset, text: joined(pieces.slice(split)) };
    if (words) {
      head.words = words.slice(0, second.word);
      tail.words = words.slice(second.word);
    }
    if (line.end !== undefined) tail.end = line.end;
    return [head, tail];
  });
}

/** A word that can start a row, and when it is sung, in seconds after the line starts. */
interface Piece {
  text: string;
  offset: number;
  /** Index of its first timed word. */
  word: number;
}

function joined(pieces: readonly Piece[]): string {
  return pieces
    .map((piece) => piece.text)
    .join(" ")
    .trim();
}

/** Whole words from word timing; syllables of one word stay together. */
function timedPieces(line: LyricLine): Piece[] {
  const pieces: Piece[] = [];
  let open = false;
  (line.words ?? []).forEach((word, index) => {
    const last = pieces.at(-1);
    if (open && last) last.text += word.text.trim();
    else pieces.push({ text: word.text.trim(), offset: word.start - line.time, word: index });
    open = !/\s$/.test(word.text);
  });
  return pieces.filter((piece) => piece.text.length > 0);
}

/** Words of a line without word timing, timed like the estimated word highlight. */
function estimatedPieces(text: string, gap: number): Piece[] {
  const words = text.split(/\s+/).filter(Boolean);
  const weights = words.map((word) => Array.from(word).length + 1);
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  const sung = Math.min(gap * 0.9, Math.max(0.6, total * 0.07));
  let offset = 0;
  return words.map((word, index) => {
    const piece = { text: word, offset, word: index };
    offset += (sung * (weights[index] ?? 0)) / total;
    return piece;
  });
}
