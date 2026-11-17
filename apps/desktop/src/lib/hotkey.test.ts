import { describe, expect, it } from "vitest";
import { hotkeyLabel } from "./hotkey";

describe("hotkeyLabel", () => {
  it("uses symbols on macOS", () => {
    expect(hotkeyLabel("Alt+Shift+L", "macos")).toBe("⌥ ⇧ L");
    expect(hotkeyLabel("CmdOrCtrl+Shift+KeyK", "macos")).toBe("⌘ ⇧ K");
  });

  it("uses names elsewhere", () => {
    expect(hotkeyLabel("Alt+Shift+L", "windows")).toBe("Alt + Shift + L");
    expect(hotkeyLabel("CmdOrCtrl+Digit1", "linux")).toBe("Ctrl + 1");
  });
});
