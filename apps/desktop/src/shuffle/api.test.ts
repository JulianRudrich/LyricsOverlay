import { describe, expect, it } from "vitest";
import { translator } from "../lib/i18n";
import { activityText, isError, keptName, mixMembers, noticeText, sourceName, type ShuffleSource } from "./api";

const sources: ShuffleSource[] = [
  { id: "liked", name: null },
  { id: "p1", name: "Road trip" },
  { id: "p2", name: "Abend" },
];

describe("true shuffle", () => {
  it("names sources and mixes", () => {
    const en = translator("en");
    const de = translator("de");
    expect(mixMembers("mix:liked,p1")).toEqual(["liked", "p1"]);
    expect(mixMembers("p1")).toEqual(["p1"]);
    expect(sourceName("liked", sources, en)).toBe("Liked Songs");
    expect(sourceName("liked", sources, de)).toBe("Lieblingssongs");
    expect(sourceName("p1", sources, en)).toBe("Road trip");
    expect(sourceName("mix:liked,p1,p2", sources, en)).toBe("Mix: Abend + Liked Songs + Road trip");
    expect(sourceName("gone", sources, en)).toBe("…");
  });

  it("describes what it does", () => {
    const en = translator("en");
    const de = translator("de");
    expect(activityText({ kind: "loading", loaded: 0, total: 0 }, en, "en")).toBe("Loading tracks…");
    expect(activityText({ kind: "loading", loaded: 1200, total: 2500 }, de, "de")).toBe("Lade Titel … 1.200 von 2.500");
    expect(activityText({ kind: "writing", written: 100, total: 250 }, en, "en")).toBe("Writing 100 of 250 tracks…");
    expect(activityText({ kind: "continuing" }, en, "en")).toBe("Adding the next round…");
    expect(noticeText({ kind: "http", status: 502 }, en)).toBe("Spotify returned an error (502).");
    expect(noticeText({ kind: "kept", name: "Mix" }, de)).toBe("Gespeichert als „Mix“");
    for (const kind of ["empty", "full", "noDevice", "auth", "offline", "premium"] as const) {
      expect(noticeText({ kind }, en)).not.toContain("shuffle.notice");
    }
    expect(isError({ kind: "kept", name: "x" })).toBe(false);
    expect(isError({ kind: "offline" })).toBe(true);
  });

  it("names a kept shuffle by its source and day", () => {
    const date = new Date(2026, 10, 15);
    expect(keptName("Liked Songs", "en", translator("en"), date)).toBe("Liked Songs · Shuffled 15 Nov");
    expect(keptName("Lieblingssongs", "de", translator("de"), date)).toBe("Lieblingssongs · Gemischt 15. Nov.");
  });
});
