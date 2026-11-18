import type { ClockReading } from "./clock";

/** Lands this far after a line's start, so rounding in the player never stops just before it. */
const LINE_LEAD = 0.01;

/**
 * The player position at which the lyrics show `lyricTime`. The timing
 * corrections show the lyrics earlier, so the player goes there earlier too.
 */
export function lyricSeekTarget(lyricTime: number, offset: number): number {
  return Math.max(0, lyricTime - offset + LINE_LEAD);
}

/** "3:07", or "1:02:07" from an hour on. */
export function formatTime(seconds: number): string {
  const total = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor(total / 60) % 60;
  const rest = String(total % 60).padStart(2, "0");
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${rest}` : `${minutes}:${rest}`;
}

/** Time left, rounded so that it and the elapsed time add up to the duration. */
export function formatRemaining(position: number, duration: number): string {
  return formatTime(Math.floor(duration) - Math.floor(Math.max(0, position)));
}

/** How long the overlay waits for the player to confirm a seek. */
const SEEK_TIMEOUT_MS = 1500;
/** Slack for rounding and for a player that starts a moment early or late. */
const SEEK_SLACK = 0.25;

/**
 * Remembers a seek the overlay asked for. Players can report the old
 * position for a moment afterwards; taken at face value, those readings would
 * throw the lyrics back and then forward again.
 */
export class SeekHold {
  private target: number | null = null;
  private startedAt = 0;
  private rate = 1;
  private trackKey: string | null = null;

  /** Call right after asking the player to go to `target` seconds. `now` is in milliseconds. */
  start(target: number, trackKey: string | null, rate: number, now: number): void {
    this.target = target;
    this.trackKey = trackKey;
    this.rate = Number.isFinite(rate) && rate > 0 ? rate : 1;
    this.startedAt = now;
  }

  /**
   * True while `reading` still shows where the player was before the seek.
   * The first reading that agrees with the seek ends the hold, and so do a
   * new track and a player that has not caught up after a second and a half.
   */
  isStale(reading: ClockReading, now: number): boolean {
    const target = this.target;
    if (target === null) return false;
    const elapsed = Math.max(0, now - this.startedAt);
    const position = reading.position;
    const stale =
      position !== null &&
      reading.trackKey === this.trackKey &&
      elapsed < SEEK_TIMEOUT_MS &&
      (position < target - SEEK_SLACK || position > target + (elapsed / 1000) * this.rate + SEEK_SLACK);
    if (!stale) this.target = null;
    return stale;
  }
}
