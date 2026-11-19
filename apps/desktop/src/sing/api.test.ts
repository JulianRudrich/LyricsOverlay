import { describe, expect, it } from "vitest";
import { translator } from "../lib/i18n";
import { isActive, needsSettings, percent, problemText, type ModelView, type SingView } from "./api";

const view = (phase: SingView["phase"]): SingView => ({ phase, heard: "", song: null, alternatives: 0, problem: null });

describe("Sing to find in the interface", () => {
  it("is active while it listens or follows", () => {
    expect(isActive(null)).toBe(false);
    expect(["preparing", "listening", "searching", "following"].map((phase) => isActive(view(phase as SingView["phase"])))).toEqual([
      true,
      true,
      true,
      true,
    ]);
    expect(isActive(view("notFound"))).toBe(false);
    expect(isActive(view("problem"))).toBe(false);
  });

  it("counts download progress in whole percent below 100", () => {
    const model = (received: number): ModelView => ({
      state: "downloading",
      received,
      total: 200,
      error: null,
      name: "Whisper base",
      custom: false,
    });
    expect(percent(model(0))).toBe(0);
    expect(percent(model(51))).toBe(25);
    expect(percent(model(200))).toBe(99);
    expect(percent({ ...model(5), total: 0 })).toBe(0);
  });

  it("explains a denied microphone the way each system needs it", () => {
    const en = translator("en");
    expect(problemText({ kind: "denied" }, "macos", en)).toContain("System Settings → Privacy & Security → Microphone");
    expect(problemText({ kind: "denied" }, "windows", en)).toContain("Let desktop apps access your microphone");
    expect(problemText({ kind: "silent" }, "other", en)).toContain("sound settings");
    expect(problemText({ kind: "failed", detail: "boom" }, "linux", en)).toBe("Sing to find stopped: boom");
    expect(problemText({ kind: "noToken" }, "linux", translator("de"))).toContain("Genius-Token");
    expect(needsSettings({ kind: "noToken" })).toBe(true);
    expect(needsSettings({ kind: "offline" })).toBe(false);
  });
});
