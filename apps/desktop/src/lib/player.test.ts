import { describe, expect, it } from "vitest";
import type { ClockReading } from "./clock";
import { currentIndex } from "./lyrics";
import { formatRemaining, formatTime, lyricSeekTarget, SeekHold } from "./player";
import type { LyricLine } from "./types";

describe("lyricSeekTarget", () => {
  it("lights up exactly the chosen line, whatever the corrections", () => {
    const lines: LyricLine[] = [12.34, 15.6, 17.05, 19.99, 23.1].map((time, index) => ({ time, text: `Line ${index}` }));
    for (const offset of [0, 0.3, -0.7, 1.25, -2.5, 4.9]) {
      lines.forEach((line, index) => {
        const target = lyricSeekTarget(line.time, offset);
        // MPRIS counts whole microseconds and cuts off the rest.
        const reported = Math.trunc(target * 1e6) / 1e6;
        expect(currentIndex(lines, reported + offset), `line ${index}, offset ${offset}`).toBe(index);
      });
    }
  });

  it("goes earlier when the lyrics are shown earlier", () => {
    expect(lyricSeekTarget(30, 0.5)).toBeCloseTo(29.51);
    expect(lyricSeekTarget(30, -0.5)).toBeCloseTo(30.51);
  });

  it("never goes before the start of the song", () => {
    expect(lyricSeekTarget(0.2, 1)).toBe(0);
  });
});

describe("formatTime", () => {
  it("shows minutes and seconds, and hours once there are any", () => {
    expect(formatTime(0)).toBe("0:00");
    expect(formatTime(59.99)).toBe("0:59");
    expect(formatTime(61)).toBe("1:01");
    expect(formatTime(600)).toBe("10:00");
    expect(formatTime(3600)).toBe("1:00:00");
    expect(formatTime(3725)).toBe("1:02:05");
  });

  it("shows nothing below zero", () => {
    expect(formatTime(-3)).toBe("0:00");
    expect(formatTime(Number.NaN)).toBe("0:00");
  });

  it("keeps elapsed and remaining time adding up to the duration", () => {
    expect([formatTime(0.5), formatRemaining(0.5, 200)]).toEqual(["0:00", "3:20"]);
    expect([formatTime(199.5), formatRemaining(199.5, 200)]).toEqual(["3:19", "0:01"]);
    expect(formatRemaining(0, 200.7)).toBe("3:20");
    expect(formatRemaining(200.7, 200.7)).toBe("0:00");
    expect(formatRemaining(250, 200)).toBe("0:00");
  });
});

describe("SeekHold", () => {
  const reading = (position: number | null, trackKey: string | null = "song"): ClockReading => ({
    position,
    playing: true,
    rate: 1,
    duration: 200,
    trackKey,
  });

  it("ignores readings from before the seek until the player arrives", () => {
    const hold = new SeekHold();
    hold.start(60, "song", 1, 1000);
    expect(hold.isStale(reading(20.3), 1100)).toBe(true);
    expect(hold.isStale(reading(20.8), 1500)).toBe(true);
    expect(hold.isStale(reading(60.6), 1600)).toBe(false);
    // Once the player has arrived, every reading counts again.
    expect(hold.isStale(reading(20), 1700)).toBe(false);
  });

  it("works for seeks back as well", () => {
    const hold = new SeekHold();
    hold.start(10, "song", 1, 0);
    expect(hold.isStale(reading(95), 200)).toBe(true);
    expect(hold.isStale(reading(10.2), 300)).toBe(false);
  });

  it("accepts a player that waits at the target while it buffers", () => {
    const hold = new SeekHold();
    hold.start(60, "song", 1, 0);
    expect(hold.isStale(reading(60), 900)).toBe(false);
  });

  it("allows for the playback rate", () => {
    const hold = new SeekHold();
    hold.start(60, "song", 2, 0);
    expect(hold.isStale(reading(61.9), 1000)).toBe(false);
  });

  it("gives up after a second and a half", () => {
    const hold = new SeekHold();
    hold.start(60, "song", 1, 0);
    expect(hold.isStale(reading(21.4), 1400)).toBe(true);
    expect(hold.isStale(reading(21.5), 1500)).toBe(false);
    expect(hold.isStale(reading(21.6), 1600)).toBe(false);
  });

  it("lets a new track and a missing position through", () => {
    const hold = new SeekHold();
    hold.start(60, "song", 1, 0);
    expect(hold.isStale(reading(3, "other"), 100)).toBe(false);
    hold.start(60, "song", 1, 0);
    expect(hold.isStale(reading(null), 100)).toBe(false);
  });

  it("holds nothing without a seek", () => {
    expect(new SeekHold().isStale(reading(42), 0)).toBe(false);
  });
});
