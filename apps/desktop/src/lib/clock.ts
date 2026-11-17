/** A position reading from the player, as the Rust side sends it. */
export interface ClockReading {
  /** Seconds into the track at the moment the reading was sent, or null. */
  position: number | null;
  playing: boolean;
  rate: number;
  duration: number | null;
  /** Identifies the track, so a new song always starts fresh. */
  trackKey: string | null;
}

/** Corrections up to this size are blended in; larger ones are seeks. */
const SNAP_THRESHOLD = 0.35;
/** Share of a small correction applied per reading. */
const BLEND = 0.3;

/**
 * Moves the position forward between two readings so the highlight runs
 * smoothly, and absorbs the jitter of readings that arrive a little early or
 * late. Seeks, pauses and new tracks snap immediately.
 */
export class PlaybackClock {
  private anchorPosition = 0;
  private anchorTime = 0;
  private playing = false;
  private rate = 1;
  private duration: number | null = null;
  private trackKey: string | null = null;
  private known = false;

  /** `now` is a monotonic time in milliseconds, such as performance.now(). */
  update(reading: ClockReading, now: number): void {
    const rate = Number.isFinite(reading.rate) && reading.rate > 0 ? reading.rate : 1;
    if (reading.position === null || !Number.isFinite(reading.position)) {
      this.known = false;
      this.playing = false;
      this.trackKey = reading.trackKey;
      return;
    }
    const target = Math.max(0, reading.position);
    const predicted = this.positionAt(now);
    const sameTrack = this.known && reading.trackKey === this.trackKey;
    const small = Math.abs(target - predicted) <= SNAP_THRESHOLD;
    const steady = sameTrack && this.playing && reading.playing && rate === this.rate;

    this.anchorPosition = steady && small ? predicted + (target - predicted) * BLEND : target;
    this.anchorTime = now;
    this.playing = reading.playing;
    this.rate = rate;
    this.duration = reading.duration;
    this.trackKey = reading.trackKey;
    this.known = true;
  }

  get hasPosition(): boolean {
    return this.known;
  }

  get isPlaying(): boolean {
    return this.known && this.playing;
  }

  positionAt(now: number): number {
    if (!this.known) return 0;
    const elapsed = Math.max(0, now - this.anchorTime) / 1000;
    const position = this.anchorPosition + (this.playing ? elapsed * this.rate : 0);
    return this.duration && this.duration > 0 ? Math.min(position, this.duration) : position;
  }
}
