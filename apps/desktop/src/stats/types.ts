// Mirrors the listening summary the Rust side sends. Field names follow serde's camelCase.

export type StatsRange = "today" | "week" | "month" | "year" | "all";
export type BucketUnit = "hour" | "day" | "month";

export const RANGES: readonly StatsRange[] = ["today", "week", "month", "year", "all"];

export interface Bucket {
  /** Local time the bar starts at, such as "2026-11-17T14:00:00". */
  start: string;
  seconds: number;
}

export interface RankedEntry {
  key: string;
  title: string;
  /** The artist of a song; empty for an artist. */
  subtitle: string;
  seconds: number;
  plays: number;
}

export interface ListeningSummary {
  range: StatsRange;
  /** Whether any song was recorded at all, in any range. */
  hasHistory: boolean;
  seconds: number;
  /** Calendar days the range covers so far. */
  days: number;
  dailyAverage: number;
  /** Songs heard in the range, played or skipped. */
  songs: number;
  plays: number;
  syncedPlays: number;
  skips: number;
  streakDays: number;
  unit: BucketUnit;
  buckets: Bucket[];
  /** Seconds per weekday, Monday first, and hour of the day. */
  clock: number[][];
  topTracks: RankedEntry[];
  topArtists: RankedEntry[];
}
