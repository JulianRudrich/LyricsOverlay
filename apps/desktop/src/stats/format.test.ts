import { describe, expect, it } from "vitest";
import { translator } from "../lib/i18n";
import {
  axisLabel,
  bucketLabel,
  formatDuration,
  heatStep,
  hourRange,
  labelEvery,
  parseLocal,
  percent,
  splitNumbers,
  ticks,
  weekdays,
} from "./format";

const de = translator("de");
const en = translator("en");

describe("stats formatting", () => {
  it("writes listening time in minutes and hours", () => {
    expect(formatDuration(0, "de", de)).toBe("0 Min.");
    expect(formatDuration(20, "de", de)).toBe("< 1 Min.");
    expect(formatDuration(42 * 60 + 10, "de", de)).toBe("42 Min.");
    expect(formatDuration(65 * 60, "de", de)).toBe("1 Std. 5 Min.");
    expect(formatDuration(1_234 * 3_600, "de", de)).toBe("1.234 Std. 0 Min.");
    expect(formatDuration(65 * 60, "en", en)).toBe("1 h 5 min");
    expect(formatDuration(Number.NaN, "en", en)).toBe("0 min");
  });

  it("splits numbers from their units", () => {
    expect(splitNumbers("6 Std. 47 Min.")).toEqual([
      { text: "6", number: true },
      { text: " Std. ", number: false },
      { text: "47", number: true },
      { text: " Min.", number: false },
    ]);
    expect(splitNumbers("1.234")).toEqual([{ text: "1.234", number: true }]);
    expect(splitNumbers("< 1 min").map((part) => part.number)).toEqual([false, true, false]);
  });

  it("reads local times without moving them to another zone", () => {
    const date = parseLocal("2026-11-17T14:00:00");
    expect([date.getFullYear(), date.getMonth(), date.getDate(), date.getHours()]).toEqual([2026, 10, 17, 14]);
  });

  it("names hours, days and months", () => {
    expect(hourRange(14, "de")).toBe("14–15 Uhr");
    expect(hourRange(23, "de")).toBe("23–24 Uhr");
    expect(hourRange(14, "en")).toBe("2–3 PM");
    expect(hourRange(23, "en")).toBe("11 PM–12 AM");
    const tuesday = parseLocal("2026-11-17T00:00:00");
    expect(bucketLabel(tuesday, "day", "de")).toBe("Di., 17. Nov.");
    expect(bucketLabel(tuesday, "month", "en")).toBe("November 2026");
    // Whether a lone German weekday or day ends with a period depends on the ICU version.
    expect(axisLabel(tuesday, "day", "week", "de")).toMatch(/^Di\.?$/);
    expect(axisLabel(tuesday, "day", "month", "de")).toMatch(/^17\.?$/);
    expect(axisLabel(tuesday, "month", "year", "en")).toBe("Nov");
    expect(weekdays("de")[0]).toMatch(/^Mo\.?$/);
    expect(weekdays("en")).toEqual(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
  });

  it("picks clean axis values", () => {
    expect(ticks(42 * 60)).toEqual({ values: [0, 20, 40, 60].map((m) => m * 60), unit: "minutes" });
    expect(ticks(7 * 60)).toEqual({ values: [0, 5, 10].map((m) => m * 60), unit: "minutes" });
    expect(ticks(30)).toEqual({ values: [0, 60], unit: "minutes" });
    expect(ticks(0)).toEqual({ values: [0, 60], unit: "minutes" });
    expect(ticks(5.5 * 3_600)).toEqual({ values: [0, 2, 4, 6].map((h) => h * 3_600), unit: "hours" });
  });

  it("steps the heatmap and spaces the labels", () => {
    expect(heatStep(0, 100)).toBe(0);
    expect(heatStep(1, 100)).toBe(1);
    expect(heatStep(50, 100)).toBe(2);
    expect(heatStep(100, 100)).toBe(4);
    expect(heatStep(5, 0)).toBe(0);
    expect(labelEvery(7, 600, 40)).toBe(1);
    expect(labelEvery(30, 600, 40)).toBe(2);
    expect(labelEvery(24, 300, 40)).toBe(4);
    expect(percent(1, 3)).toBe(33);
    expect(percent(5, 0)).toBe(0);
  });
});
