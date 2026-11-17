import { describe, expect, it } from "vitest";
import de from "../locales/de.json";
import en from "../locales/en.json";
import { allKeys, formatOffset, translator } from "./i18n";

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();

describe("translations", () => {
  it("have the same keys in every language", () => {
    expect(allKeys.de).toEqual(allKeys.en);
  });

  it("use the same placeholders in every language", () => {
    for (const key of allKeys.en) {
      expect(placeholders(de[key as keyof typeof de]), key).toEqual(placeholders(en[key as keyof typeof en]));
    }
  });

  it("fill in values", () => {
    expect(translator("de")("status.idleSource", { player: "Spotify" })).toBe("Spiele einen Song in Spotify ab.");
    expect(translator("en")("status.idleSource")).toBe("Play a song in {player}.");
  });

  it("formats offsets with the right decimal separator", () => {
    expect(formatOffset("de", 0.25)).toBe("+0,25 s");
    expect(formatOffset("en", -0.5)).toBe("−0.50 s");
    expect(formatOffset("en", 0)).toBe("±0.00 s");
  });
});
