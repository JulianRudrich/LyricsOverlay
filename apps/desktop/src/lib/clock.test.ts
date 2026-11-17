import { describe, expect, it } from "vitest";
import { PlaybackClock, type ClockReading } from "./clock";

const reading = (position: number | null, overrides: Partial<ClockReading> = {}): ClockReading => ({
  position,
  playing: true,
  rate: 1,
  duration: 200,
  trackKey: "a",
  ...overrides,
});

describe("PlaybackClock", () => {
  it("runs between readings and stops at the end", () => {
    const clock = new PlaybackClock();
    clock.update(reading(10), 1_000);
    expect(clock.positionAt(1_500)).toBeCloseTo(10.5);
    clock.update(reading(199.5), 2_000);
    expect(clock.positionAt(5_000)).toBe(200);
  });

  it("blends small corrections and snaps seeks", () => {
    const clock = new PlaybackClock();
    clock.update(reading(10), 0);
    clock.update(reading(11.2), 1_000);
    expect(clock.positionAt(1_000)).toBeCloseTo(11.06);
    clock.update(reading(60), 2_000);
    expect(clock.positionAt(2_000)).toBe(60);
  });

  it("snaps on pause, a new track and unknown positions", () => {
    const clock = new PlaybackClock();
    clock.update(reading(10), 0);
    clock.update(reading(10.9, { playing: false }), 1_000);
    expect(clock.positionAt(5_000)).toBeCloseTo(10.9);
    expect(clock.isPlaying).toBe(false);
    clock.update(reading(0.1, { trackKey: "b" }), 6_000);
    expect(clock.positionAt(6_000)).toBeCloseTo(0.1);
    clock.update(reading(null), 7_000);
    expect(clock.hasPosition).toBe(false);
    expect(clock.positionAt(8_000)).toBe(0);
  });

  it("follows the playback rate", () => {
    const clock = new PlaybackClock();
    clock.update(reading(10, { rate: 2 }), 0);
    expect(clock.positionAt(1_000)).toBeCloseTo(12);
  });
});
