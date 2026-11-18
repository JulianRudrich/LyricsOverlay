import { describe, expect, it } from "vitest";
import { hotkeyLabel, recordKey, releaseKey, type KeyPress } from "./hotkey";

describe("hotkeyLabel", () => {
  it("uses symbols on macOS", () => {
    expect(hotkeyLabel("Alt+Shift+L", "macos")).toBe("⌥ ⇧ L");
    expect(hotkeyLabel("CmdOrCtrl+Shift+KeyK", "macos")).toBe("⌘ ⇧ K");
    expect(hotkeyLabel("Ctrl+Alt+Shift+Super+ArrowUp", "macos")).toBe("⌃ ⌥ ⇧ ⌘ ↑");
    expect(hotkeyLabel("Super+Backspace", "macos")).toBe("⌘ ⌫");
  });

  it("uses names elsewhere", () => {
    expect(hotkeyLabel("Alt+Shift+L", "windows")).toBe("Alt + Shift + L");
    expect(hotkeyLabel("CmdOrCtrl+Digit1", "linux")).toBe("Ctrl + 1");
    expect(hotkeyLabel("Ctrl+Alt+Numpad5", "windows")).toBe("Ctrl + Alt + Num 5");
    expect(hotkeyLabel("Super+Space", "linux")).toBe("Win + Space");
    expect(hotkeyLabel("Ctrl+Backquote", "windows")).toBe("Ctrl + `");
  });
});

const press = (code: string, key: string, held: Partial<KeyPress> = {}): KeyPress => ({
  code,
  key,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...held,
});

describe("recordKey", () => {
  it("writes the modifiers in one order", () => {
    expect(recordKey(press("KeyL", "L", { shiftKey: true, altKey: true }), "windows")).toEqual({
      kind: "done",
      hotkey: "Alt+Shift+L",
    });
    expect(recordKey(press("KeyK", "k", { metaKey: true, shiftKey: true, ctrlKey: true }), "macos")).toEqual({
      kind: "done",
      hotkey: "Ctrl+Shift+Super+K",
    });
  });

  it("finds keys by place on macOS and by what they type elsewhere", () => {
    // ⌥ types a symbol on a Mac; the key's place still counts.
    expect(recordKey(press("KeyL", "¬", { altKey: true, shiftKey: true }), "macos")).toEqual({
      kind: "done",
      hotkey: "Alt+Shift+L",
    });
    // The key labelled A on a French keyboard sits where Q is on an English one.
    const azerty = press("KeyQ", "a", { ctrlKey: true, altKey: true });
    expect(recordKey(azerty, "windows")).toEqual({ kind: "done", hotkey: "Ctrl+Alt+A" });
    expect(recordKey(azerty, "macos")).toEqual({ kind: "done", hotkey: "Ctrl+Alt+Q" });
    // The number pad stays the number pad.
    expect(recordKey(press("Numpad5", "5", { ctrlKey: true }), "linux")).toEqual({ kind: "done", hotkey: "Ctrl+Numpad5" });
    expect(recordKey(press("Digit1", "!", { ctrlKey: true, shiftKey: true }), "windows")).toEqual({
      kind: "done",
      hotkey: "Ctrl+Shift+1",
    });
  });

  it("lets go of a modifier even when the engine still reports it", () => {
    expect(releaseKey(press("ShiftLeft", "Shift", { shiftKey: true, altKey: true }))).toEqual(["Alt"]);
    expect(releaseKey(press("AltLeft", "Alt", { shiftKey: true }))).toEqual(["Shift"]);
    expect(releaseKey(press("KeyK", "k", { ctrlKey: true }))).toEqual(["Ctrl"]);
  });

  it("waits while only modifiers are held", () => {
    expect(recordKey(press("ShiftLeft", "Shift", { shiftKey: true, altKey: true }), "windows")).toEqual({
      kind: "held",
      modifiers: ["Alt", "Shift"],
    });
    expect(recordKey(press("MetaRight", "Meta", { metaKey: true }), "macos")).toEqual({ kind: "held", modifiers: ["Super"] });
  });

  it("cancels with Escape and explains what does not work", () => {
    expect(recordKey(press("Escape", "Escape"), "windows")).toEqual({ kind: "cancel" });
    expect(recordKey(press("Escape", "Escape", { ctrlKey: true }), "windows")).toEqual({
      kind: "problem",
      problem: "unsupported",
    });
    expect(recordKey(press("KeyL", "L", { shiftKey: true }), "linux")).toEqual({ kind: "problem", problem: "needsModifier" });
    expect(recordKey(press("CapsLock", "CapsLock", { ctrlKey: true }), "linux")).toEqual({
      kind: "problem",
      problem: "unsupported",
    });
    expect(recordKey(press("F13", "F13"), "macos")).toEqual({ kind: "done", hotkey: "F13" });
  });
});
