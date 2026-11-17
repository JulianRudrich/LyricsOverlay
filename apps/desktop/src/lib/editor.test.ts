import { describe, expect, it } from "vitest";
import {
  applyEdit,
  canSave,
  caretLine,
  checkDraft,
  countSyncedLines,
  draftStatus,
  isStampShortcut,
  MAX_LYRICS_BYTES,
  normalizeNewlines,
  ownLyricsErrorKey,
  stampErrorKey,
  utf8Length,
  type KeyPress,
} from "./editor";
import { translator } from "./i18n";

const press = (key: string, modifiers: Partial<KeyPress> = {}): KeyPress => ({
  key,
  code: `Key${key.toUpperCase()}`,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ...modifiers,
});

describe("countSyncedLines", () => {
  it("counts timed lines with text", () => {
    expect(countSyncedLines("[00:01.00] One\n[00:02.50]Two\r\n[01:03.10] Three")).toBe(3);
  });

  it("counts each timestamp of a repeated row", () => {
    expect(countSyncedLines("[00:07.00][00:09.10] Again")).toBe(2);
    expect(countSyncedLines("[00:07.00] [00:09.10] Again")).toBe(2);
  });

  it("skips tags, plain lines, breaks and broken timestamps", () => {
    const draft = [
      "[ar:Example Artist]",
      "[offset:+500]",
      "Plain line without a timestamp",
      "[00:05.00]",
      "[00:06.00]   ",
      "[00:07.00]<00:07.00><00:08.00>",
      "[00:75.00] Seconds out of range",
      "Words [00:09.00] before the stamp",
    ].join("\n");
    expect(countSyncedLines(draft)).toBe(0);
  });

  it("reads word tags, padding and every kind of line break", () => {
    expect(countSyncedLines("[00:01.00] <00:01.00>Word <00:01.50>by word")).toBe(1);
    expect(countSyncedLines("\uFEFF  [00:01.00] Indented\u2028[00:02.00] Next\u0085[00:03.00] Last")).toBe(3);
    expect(countSyncedLines("[00:01.00] A line with [00:02.00] inside")).toBe(1);
    expect(countSyncedLines("[999:59.99] Very late")).toBe(1);
  });
});

describe("checkDraft", () => {
  it("tells valid drafts from empty and plain ones", () => {
    expect(checkDraft("[00:01.00] One\n[00:02.00] Two")).toEqual({ kind: "valid", lines: 2 });
    expect(checkDraft("")).toEqual({ kind: "noLyrics" });
    expect(checkDraft("Pasted lyrics\nwithout timestamps")).toEqual({ kind: "noLyrics" });
  });

  it("allows at most 512 KB of UTF-8", () => {
    const stamped = "[00:01.00] ";
    const fits = stamped + "a".repeat(MAX_LYRICS_BYTES - stamped.length);
    expect(checkDraft(fits)).toEqual({ kind: "valid", lines: 1 });
    expect(checkDraft(fits + "a")).toEqual({ kind: "tooLarge" });
    // Umlauts take two bytes each, so fewer characters already reach the limit.
    expect(checkDraft(stamped + "ö".repeat(MAX_LYRICS_BYTES / 2))).toEqual({ kind: "tooLarge" });
  });
});

describe("utf8Length", () => {
  it("counts bytes like Rust", () => {
    expect(utf8Length("abc")).toBe(3);
    expect(utf8Length("ä")).toBe(2);
    expect(utf8Length("€")).toBe(3);
    expect(utf8Length("🌙")).toBe(4);
    expect(utf8Length("\ud800")).toBe(3);
  });
});

describe("draftStatus", () => {
  it("names the number of lines or what is missing", () => {
    const de = translator("de");
    expect(draftStatus({ kind: "valid", lines: 1 }, de, "de")).toBe("1 synchronisierte Zeile");
    expect(draftStatus({ kind: "valid", lines: 1200 }, de, "de")).toBe("1.200 synchronisierte Zeilen");
    expect(draftStatus({ kind: "noLyrics" }, de, "de")).toBe("Mindestens eine Zeile mit Zeitstempel fehlt.");
    expect(draftStatus({ kind: "tooLarge" }, de, "de")).toBe("Maximal 512 KB pro Song.");
    expect(draftStatus({ kind: "valid", lines: 12 }, translator("en"), "en")).toBe("12 synced lines");
  });
});

describe("canSave", () => {
  it("needs a valid change", () => {
    const valid = { kind: "valid", lines: 1 } as const;
    expect(canSave(valid, "[00:01.00] New", "[00:01.00] Old")).toBe(true);
    expect(canSave(valid, "[00:01.00] Same", "[00:01.00] Same")).toBe(false);
    expect(canSave({ kind: "noLyrics" }, "plain", "")).toBe(false);
    expect(canSave({ kind: "tooLarge" }, "huge", "")).toBe(false);
  });
});

describe("editing helpers", () => {
  it("normalizes line breaks like a text field", () => {
    expect(normalizeNewlines("One\r\nTwo\rThree\nFour")).toBe("One\nTwo\nThree\nFour");
  });

  it("applies an edit in UTF-16 units", () => {
    expect(applyEdit("🌙 One\nTwo", { start: 0, length: 0, replacement: "[00:01.00]", nextCaret: 17 })).toBe(
      "[00:01.00]🌙 One\nTwo",
    );
    expect(applyEdit("[00:01.00] One", { start: 0, length: 10, replacement: "[00:02.50]", nextCaret: 14 })).toBe(
      "[00:02.50] One",
    );
  });

  it("finds the caret's line", () => {
    expect(caretLine("One\nTwo\nThree", 0)).toBe(0);
    expect(caretLine("One\nTwo\nThree", 4)).toBe(1);
    expect(caretLine("One\nTwo\nThree", 13)).toBe(2);
  });
});

describe("isStampShortcut", () => {
  it("uses Command on macOS and Ctrl elsewhere", () => {
    expect(isStampShortcut(press("T", { metaKey: true, shiftKey: true }), "macos")).toBe(true);
    expect(isStampShortcut(press("T", { ctrlKey: true, shiftKey: true }), "macos")).toBe(false);
    expect(isStampShortcut(press("T", { ctrlKey: true, shiftKey: true }), "windows")).toBe(true);
    expect(isStampShortcut(press("T", { metaKey: true, shiftKey: true }), "linux")).toBe(false);
  });

  it("needs Shift, no Alt and the letter T", () => {
    expect(isStampShortcut(press("t", { ctrlKey: true }), "linux")).toBe(false);
    expect(isStampShortcut(press("T", { ctrlKey: true, shiftKey: true, altKey: true }), "linux")).toBe(false);
    expect(isStampShortcut(press("Y", { ctrlKey: true, shiftKey: true, code: "KeyT" }), "linux")).toBe(false);
    expect(isStampShortcut(press("Е", { ctrlKey: true, shiftKey: true, code: "KeyT" }), "linux")).toBe(true);
  });
});

describe("error messages", () => {
  it("map stamp refusals to hints", () => {
    expect(stampErrorKey("otherTrack")).toBe("editor.otherTrack");
    expect(stampErrorKey("noPosition")).toBe("editor.noPosition");
    expect(stampErrorKey("notALyricLine")).toBe("editor.notALine");
    expect(stampErrorKey(new Error("ipc"))).toBe("editor.stampFailed");
  });

  it("explain failed imports, saves and removals", () => {
    expect(ownLyricsErrorKey("tooLarge", "import")).toBe("ownLyrics.fileTooLarge");
    expect(ownLyricsErrorKey("tooLarge", "save")).toBe("editor.tooLarge");
    expect(ownLyricsErrorKey("missingTimestamps", "import")).toBe("ownLyrics.noTimestamps");
    expect(ownLyricsErrorKey("notText", "import")).toBe("ownLyrics.notText");
    expect(ownLyricsErrorKey("noTrack", "edit")).toBe("ownLyrics.noTrack");
    expect(ownLyricsErrorKey("storage", "remove")).toBe("ownLyrics.removeFailed");
    expect(ownLyricsErrorKey("something else", "save")).toBe("ownLyrics.saveFailed");
  });
});
