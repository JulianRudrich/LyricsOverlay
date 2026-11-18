import type { TransitionConfig } from "svelte/transition";

/** A timing function like CSS `cubic-bezier()`. */
export function cubicBezier(x1: number, y1: number, x2: number, y2: number): (t: number) => number {
  const cx = 3 * x1;
  const bx = 3 * (x2 - x1) - cx;
  const ax = 1 - cx - bx;
  const cy = 3 * y1;
  const by = 3 * (y2 - y1) - cy;
  const ay = 1 - cy - by;
  const x = (s: number) => ((ax * s + bx) * s + cx) * s;
  const y = (s: number) => ((ay * s + by) * s + cy) * s;
  const slope = (s: number) => (3 * ax * s + 2 * bx) * s + cx;
  return (t) => {
    if (t <= 0) return 0;
    if (t >= 1) return 1;
    let s = t;
    for (let step = 0; step < 8; step += 1) {
      const error = x(s) - t;
      if (Math.abs(error) < 1e-7) return y(s);
      const d = slope(s);
      if (Math.abs(d) < 1e-7) break;
      s -= error / d;
    }
    let low = 0;
    let high = 1;
    s = t;
    while (high - low > 1e-7) {
      if (x(s) < t) low = s;
      else high = s;
      s = (low + high) / 2;
    }
    return y(s);
  };
}

/** The strong ease-out everything that appears uses. */
export const easeOut = cubicBezier(0.23, 1, 0.32, 1);

/** Smoothstep: eases a value driven by a clock rather than by an animation. */
export function smoothstep(x: number): number {
  const t = Math.min(1, Math.max(0, x));
  return t * t * (3 - 2 * t);
}

/**
 * Leaving content when the song changes: it drifts up, blurs and fades. With
 * reduced motion it only fades.
 */
export function songExit(_node: Element, { reduced = false }: { reduced?: boolean } = {}): TransitionConfig {
  return {
    duration: 250,
    easing: easeOut,
    css: reduced
      ? (t) => `opacity: ${t}`
      : (t, u) => `opacity: ${t}; transform: translateY(${-10 * u}px); filter: blur(${6 * u}px)`,
  };
}

/** Controls that slide in from the right and fade. With reduced motion they only fade. */
export function slideIn(_node: Element, { reduced = false }: { reduced?: boolean } = {}): TransitionConfig {
  return {
    duration: 200,
    easing: easeOut,
    css: reduced ? (t) => `opacity: ${t}` : (t, u) => `opacity: ${t}; transform: translateX(${6 * u}px)`,
  };
}

/**
 * One number moving towards a target like a damped spring: `response` is the
 * period of the undamped spring in seconds, `damping` the share of critical
 * damping. A new target keeps the current speed, so quick changes flow into
 * each other. Steps are exact for any frame time.
 */
export class Spring {
  value: number;
  target: number;
  velocity = 0;
  private readonly omega: number;
  private readonly zeta: number;
  private readonly precision: number;

  constructor(value: number, { response = 0.48, damping = 0.88, precision = 0.001 } = {}) {
    this.value = value;
    this.target = value;
    this.omega = (2 * Math.PI) / Math.max(0.01, response);
    this.zeta = Math.min(0.999, Math.max(0.01, damping));
    this.precision = precision;
  }

  get moving(): boolean {
    return this.value !== this.target || this.velocity !== 0;
  }

  /** Moves `dt` seconds on; returns whether the spring is still moving. */
  step(dt: number): boolean {
    if (!this.moving) return false;
    const x0 = this.value - this.target;
    const v0 = this.velocity;
    const decay = this.zeta * this.omega;
    const damped = this.omega * Math.sqrt(1 - this.zeta * this.zeta);
    const fade = Math.exp(-decay * Math.max(0, dt));
    const cos = Math.cos(damped * dt);
    const sin = Math.sin(damped * dt);
    const b = (v0 + decay * x0) / damped;
    const x = fade * (x0 * cos + b * sin);
    const v = fade * (v0 * cos - (decay * b + damped * x0) * sin);
    if (Math.abs(x) < this.precision && Math.abs(v) < this.precision * 10) {
      this.settle();
      return false;
    }
    this.value = this.target + x;
    this.velocity = v;
    return true;
  }

  /** Jumps to the target and stops. */
  settle(): void {
    this.value = this.target;
    this.velocity = 0;
  }
}
