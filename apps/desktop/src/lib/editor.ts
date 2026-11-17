import { formatNumber, type MessageKey, type Translate } from "./i18n";
import type { StampEdit } from "./types";

/** The largest own lyrics for one song, in UTF-8 bytes; the song database refuses more. */
export const MAX_LYRICS_BYTES = 512 * 1024;

/** Stamps the line at the cursor: Ctrl+Shift+T, or ⌘⇧T on macOS. */
export const STAMP_SHORTCUT = "CmdOrCtrl+Shift+T";

/** What the editor's status line says about a draft. */
export type DraftCheck = { kind: "valid"; lines: number } | { kind: "noLyrics" } | { kind: "tooLarge" };

// The characters lyrics-core treats as line breaks, inline spaces and line padding,
// so a draft is counted here exactly as the song database will read it.
const LINE_BREAK = /[\n\v\f\r\u0085\u2028\u2029]/;
const SPACE = "\\t \\u00A0\\u1680\\u2000-\\u200A\\u202F\\u205F\\u3000";
const PADDING = new RegExp(`^[${SPACE}\\uFEFF]+|[${SPACE}\\uFEFF]+$`, "g");
const OUTER_SPACE = new RegExp(`^[${SPACE}]+|[${SPACE}]+$`, "g");
const ONLY_SPACE = new RegExp(`^[${SPACE}]*$`);
const TIMESTAMP = /\[([0-9]{1,3}):([0-9]{2}(?:\.[0-9]+)?)\]/g;
const WORD_TAG = /<[0-9]{1,3}:[0-9]{2}(?:\.[0-9]+)?>/g;

/** Length of `text` in UTF-8 bytes, as the Rust side counts it. */
export function utf8Length(text: string): number {
  let bytes = 0;
  for (let index = 0; index < text.length; index += 1) {
    const unit = text.charCodeAt(index);
    if (unit < 0x80) {
      bytes += 1;
    } else if (unit < 0x800) {
      bytes += 2;
    } else if (unit >= 0xd800 && unit < 0xdc00 && isLowSurrogate(text.charCodeAt(index + 1))) {
      bytes += 4;
      index += 1;
    } else {
      // Everything else in the BMP; a lone surrogate becomes U+FFFD, also three bytes.
      bytes += 3;
    }
  }
  return bytes;
}

function isLowSurrogate(unit: number): boolean {
  return unit >= 0xdc00 && unit < 0xe000;
}

/**
 * How many lines with text the draft times, counted like lyrics-core's LRC
 * parser: a row with two timestamps counts twice, a timestamp without text is
 * an instrumental break and does not count, tag lines such as `[ar:Artist]` and
 * plain lines are skipped.
 */
export function countSyncedLines(text: string): number {
  let count = 0;
  for (const raw of text.split(LINE_BREAK)) {
    const line = raw.replace(PADDING, "");
    let prefixEnd = 0;
    let times = 0;
    for (const match of line.matchAll(TIMESTAMP)) {
      // A timestamp after the first words belongs to the lyric, not to the timing.
      if (!ONLY_SPACE.test(line.slice(prefixEnd, match.index))) break;
      prefixEnd = match.index + match[0].length;
      if (Number(match[2]) < 60) times += 1;
    }
    if (times === 0) continue;
    const lyric = line.slice(prefixEnd).replace(WORD_TAG, "").replace(OUTER_SPACE, "");
    if (lyric.length > 0) count += times;
  }
  return count;
}

/** Whether the draft can be saved, and what the status line shows for it. */
export function checkDraft(text: string): DraftCheck {
  // Every UTF-16 unit takes at least one byte, so long drafts need no counting.
  if (text.length > MAX_LYRICS_BYTES || utf8Length(text) > MAX_LYRICS_BYTES) return { kind: "tooLarge" };
  const lines = countSyncedLines(text);
  return lines > 0 ? { kind: "valid", lines } : { kind: "noLyrics" };
}

/** The editor's status line: "12 synced lines", or what is missing. */
export function draftStatus(check: DraftCheck, t: Translate, language: "de" | "en"): string {
  switch (check.kind) {
    case "tooLarge":
      return t("editor.tooLarge");
    case "noLyrics":
      return t("editor.missingTimestamp");
    default:
      return check.lines === 1
        ? t("editor.syncedLine")
        : t("editor.syncedLines", { count: formatNumber(language, check.lines) });
  }
}

/** Saving needs a valid draft that differs from the saved lyrics. */
export function canSave(check: DraftCheck, text: string, saved: string): boolean {
  return check.kind === "valid" && text !== saved;
}

/** Line breaks as a text field reports them, so loaded lyrics compare equal to the field. */
export function normalizeNewlines(text: string): string {
  return text.replace(/\r\n?/g, "\n");
}

/** The draft with `edit` applied. */
export function applyEdit(text: string, edit: StampEdit): string {
  return text.slice(0, edit.start) + edit.replacement + text.slice(edit.start + edit.length);
}

/** The zero-based line the caret is on, to scroll it into view. */
export function caretLine(text: string, caret: number): number {
  let line = 0;
  for (let index = text.indexOf("\n"); index !== -1 && index < caret; index = text.indexOf("\n", index + 1)) {
    line += 1;
  }
  return line;
}

/** The parts of a keyboard event the shortcut needs. */
export interface KeyPress {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

/**
 * True for the stamp shortcut, ⌘⇧T on macOS and Ctrl+Shift+T elsewhere. It
 * follows the letter T on Latin keyboard layouts and the T key on others.
 */
export function isStampShortcut(event: KeyPress, platform: string): boolean {
  const command = platform === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  if (!command || !event.shiftKey || event.altKey) return false;
  const key = event.key.toLowerCase();
  return /^[a-z]$/.test(key) ? key === "t" : event.code === "KeyT";
}

/** The hint for a line the Rust side did not stamp. */
export function stampErrorKey(error: unknown): MessageKey {
  switch (error) {
    case "otherTrack":
      return "editor.otherTrack";
    case "noPosition":
      return "editor.noPosition";
    case "notALyricLine":
      return "editor.notALine";
    default:
      return "editor.stampFailed";
  }
}

export type OwnLyricsAction = "edit" | "import" | "save" | "remove";

const FAILED: Record<OwnLyricsAction, MessageKey> = {
  edit: "ownLyrics.editFailed",
  import: "ownLyrics.importFailed",
  save: "ownLyrics.saveFailed",
  remove: "ownLyrics.removeFailed",
};

/** The message for a failed action on own lyrics; `error` is what the Rust side answered. */
export function ownLyricsErrorKey(error: unknown, action: OwnLyricsAction): MessageKey {
  switch (error) {
    case "noTrack":
      return "ownLyrics.noTrack";
    case "tooLarge":
      return action === "import" ? "ownLyrics.fileTooLarge" : "editor.tooLarge";
    case "notText":
      return "ownLyrics.notText";
    case "missingTimestamps":
      return action === "import" ? "ownLyrics.noTimestamps" : "editor.missingTimestamp";
    case "unreadable":
      return "ownLyrics.unreadable";
    default:
      return FAILED[action];
  }
}
