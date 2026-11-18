import type { Translate } from "../lib/i18n";
import type { BucketUnit, StatsRange } from "./types";

export type Language = "de" | "en";

const locale = (language: Language) => (language === "de" ? "de-DE" : "en-US");

function number(language: Language, value: number): string {
  return new Intl.NumberFormat(locale(language), { maximumFractionDigits: 0 }).format(value);
}

/** Listening time such as "42 Min.", "1 Std. 5 Min." or "< 1 Min.". */
export function formatDuration(seconds: number, language: Language, t: Translate): string {
  const minutes = Number.isFinite(seconds) && seconds > 0 ? Math.round(seconds / 60) : 0;
  if (minutes === 0 && seconds > 0) return t("stats.underMinute");
  if (minutes < 60) return t("stats.minutes", { minutes });
  return t("stats.hoursMinutes", { hours: number(language, Math.floor(minutes / 60)), minutes: minutes % 60 });
}

/** Splits "6 Std. 47 Min." into numbers and units, so numbers can be set larger. */
export function splitNumbers(text: string): { text: string; number: boolean }[] {
  return text
    .split(/(\d[\d.,]*)/)
    .filter((part) => part.length > 0)
    .map((part) => ({ text: part, number: /^\d/.test(part) }));
}

export function formatCount(count: number, language: Language): string {
  return number(language, count);
}

/** A whole percentage of `part` in `whole`; 0 when there is no whole. */
export function percent(part: number, whole: number): number {
  return whole > 0 ? Math.round((part / whole) * 100) : 0;
}

/** A local date and time as the Rust side sends it, such as "2026-11-17T14:00:00". */
export function parseLocal(value: string): Date {
  const [date = "", time = ""] = value.split("T");
  const [year = 1970, month = 1, day = 1] = date.split("-").map(Number);
  const [hour = 0, minute = 0] = time.split(":").map(Number);
  return new Date(year, month - 1, day, hour, minute);
}

/** One hour of the day, such as "14–15 Uhr" or "2–3 PM". */
export function hourRange(hour: number, language: Language): string {
  // German counts the last hour of the day up to 24.
  if (language === "de") return `${hour}–${hour + 1} Uhr`;
  const format = new Intl.DateTimeFormat(locale(language), { hour: "numeric" });
  const from = format.format(new Date(2026, 0, 5, hour));
  const to = format.format(new Date(2026, 0, 5, (hour + 1) % 24));
  // "14 Uhr" and "15 Uhr" share their unit, which is then written once.
  const parse = (text: string) => /^(\d+)(.*)$/.exec(text);
  const [start, end] = [parse(from), parse(to)];
  if (start && end && start[2] === end[2]) return `${start[1]}–${end[1]}${end[2]}`;
  return `${from}–${to}`;
}

/** Weekday names, Monday first, as short as the language writes them. */
export function weekdays(language: Language): string[] {
  const format = new Intl.DateTimeFormat(locale(language), { weekday: "short" });
  // 5 January 2026 was a Monday.
  return Array.from({ length: 7 }, (_, index) => format.format(new Date(2026, 0, 5 + index)));
}

/** The short label under a bar. */
export function axisLabel(date: Date, unit: BucketUnit, range: StatsRange, language: Language): string {
  const format = (options: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat(locale(language), options).format(date);
  switch (unit) {
    case "hour":
      return format({ hour: "numeric" });
    case "day":
      return range === "week" ? format({ weekday: "short" }) : format({ day: "numeric" });
    case "month":
      return format({ month: "short" });
  }
}

/** The full label of a bar, such as "Di., 17. Nov." or "November 2026". */
export function bucketLabel(date: Date, unit: BucketUnit, language: Language): string {
  switch (unit) {
    case "hour":
      return hourRange(date.getHours(), language);
    case "day":
      return new Intl.DateTimeFormat(locale(language), { weekday: "short", day: "numeric", month: "short" }).format(date);
    case "month":
      return new Intl.DateTimeFormat(locale(language), { month: "long", year: "numeric" }).format(date);
  }
}

export interface Ticks {
  /** In seconds, from zero up to at least the largest value. */
  values: number[];
  unit: "minutes" | "hours";
}

/** Clean values for the y axis: whole minutes, or whole hours from two hours on. */
export function ticks(maxSeconds: number): Ticks {
  const unit = maxSeconds >= 2 * 3_600 ? "hours" : "minutes";
  const scale = unit === "hours" ? 3_600 : 60;
  const max = Math.max(0, maxSeconds) / scale;
  if (!(max > 0)) return { values: [0, scale], unit };
  const rough = max / 3;
  const magnitude = 10 ** Math.floor(Math.log10(rough));
  const step = Math.max(1, [1, 2, 5, 10].map((factor) => factor * magnitude).find((candidate) => candidate >= rough) ?? 10 * magnitude);
  const values: number[] = [];
  for (let value = 0; values.length < 2 || value - step < max - 1e-9; value += step) {
    values.push(value * scale);
  }
  return { values, unit };
}

/** The step of a heatmap cell: 0 for nothing, then 1 to 4 relative to the busiest hour. */
export function heatStep(seconds: number, max: number): number {
  if (!(seconds > 0) || !(max > 0)) return 0;
  return Math.min(4, 1 + Math.floor((seconds / max) * 3.999));
}

/** Labels every how many bars, so the labels keep at least `spacing` pixels apart. */
export function labelEvery(count: number, width: number, spacing: number): number {
  const fit = Math.max(1, Math.floor(width / spacing));
  return Math.max(1, Math.ceil(count / fit));
}
