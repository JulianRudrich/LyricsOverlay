import { describe, expect, it } from "vitest";
import { cubicBezier, easeOut, slideIn, smoothstep, songExit, Spring } from "./motion";

const element = {} as Element;

describe("cubicBezier", () => {
  it("matches CSS timing functions", () => {
    expect(cubicBezier(0, 0, 1, 1)(0.3)).toBeCloseTo(0.3, 6);
    const ease = cubicBezier(0.25, 0.1, 0.25, 1);
    expect(ease(0.5)).toBeCloseTo(0.8024, 3);
    expect(ease(0)).toBe(0);
    expect(ease(1)).toBe(1);
  });

  it("eases out strongly and never goes back", () => {
    expect(easeOut(0.25)).toBeGreaterThan(0.7);
    const samples = Array.from({ length: 101 }, (_, index) => easeOut(index / 100));
    expect(samples.every((value, index) => index === 0 || value >= (samples[index - 1] ?? 0))).toBe(true);
  });
});

describe("smoothstep", () => {
  it("starts and ends flat and stays in range", () => {
    expect([smoothstep(-1), smoothstep(0), smoothstep(0.5), smoothstep(1), smoothstep(2)]).toEqual([0, 0, 0.5, 1, 1]);
    expect(smoothstep(0.1)).toBeCloseTo(0.028);
  });
});

describe("Spring", () => {
  const run = (spring: Spring, seconds: number, fps = 60) => {
    let highest = spring.value;
    for (let frame = 0; frame < Math.round(seconds * fps); frame += 1) {
      spring.step(1 / fps);
      highest = Math.max(highest, spring.value);
    }
    return highest;
  };

  it("arrives quickly and barely overshoots", () => {
    const spring = new Spring(0);
    spring.target = 100;
    run(spring, 0.3);
    expect(spring.value).toBeGreaterThan(90);
    expect(spring.value).toBeLessThan(99);
    const highest = run(spring, 1.5);
    expect(highest).toBeLessThan(100.5);
    expect(spring.value).toBe(100);
    expect(spring.moving).toBe(false);
  });

  it("does not depend on the frame rate", () => {
    const fast = new Spring(0);
    const slow = new Spring(0);
    fast.target = slow.target = 40;
    run(fast, 0.2, 120);
    run(slow, 0.2, 30);
    expect(fast.value).toBeCloseTo(slow.value, 6);
  });

  it("keeps its speed when the target moves", () => {
    const spring = new Spring(0);
    spring.target = 100;
    run(spring, 0.1);
    const { value, velocity } = spring;
    spring.target = 200;
    expect([spring.value, spring.velocity]).toEqual([value, velocity]);
    spring.step(1 / 60);
    expect(spring.value).toBeGreaterThan(value);
  });

  it("stays still at its target and can jump there", () => {
    const spring = new Spring(5);
    expect(spring.step(1 / 60)).toBe(false);
    spring.target = 9;
    spring.settle();
    expect([spring.value, spring.moving]).toEqual([9, false]);
  });
});

describe("transitions", () => {
  it("lets the old song drift up, blur and fade", () => {
    const exit = songExit(element);
    expect(exit.duration).toBe(250);
    expect(exit.css?.(0, 1)).toBe("opacity: 0; transform: translateY(-10px); filter: blur(6px)");
    expect(songExit(element, { reduced: true }).css?.(0.5, 0.5)).toBe("opacity: 0.5");
  });

  it("slides controls in from the right", () => {
    expect(slideIn(element).css?.(0, 1)).toBe("opacity: 0; transform: translateX(6px)");
    expect(slideIn(element, { reduced: true }).css?.(0, 1)).toBe("opacity: 0");
  });
});
