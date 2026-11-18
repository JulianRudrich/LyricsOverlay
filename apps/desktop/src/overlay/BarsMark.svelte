<script lang="ts">
  // A white three-bar mark for "no lyrics" and instrumental songs. It sways
  // gently while the music plays and settles when it pauses.
  import { onDestroy } from "svelte";
  import { prefersReducedMotion } from "svelte/motion";
  import { smoothstep } from "../lib/motion";

  let { playing }: { playing: boolean } = $props();

  /** Bar heights at rest, as a share of the tallest. */
  const REST = [0.52, 1, 0.7];
  const PERIODS = [1.7, 2.3, 1.9];
  const PHASES = [0, 2.1, 4.2];
  /** Seconds to come alive after resuming and to settle after pausing. */
  const RISE = 0.8;
  const SETTLE = 0.6;

  let heights = $state([...REST]);
  let drift = 0;
  let frame = 0;
  let last = 0;

  const moving = $derived(playing && !prefersReducedMotion.current);

  $effect(() => {
    if (moving && frame === 0) {
      last = performance.now();
      frame = requestAnimationFrame(step);
    }
  });

  function step(now: number) {
    const dt = Math.min(0.1, (now - last) / 1000);
    last = now;
    drift = moving ? Math.min(1, drift + dt / RISE) : Math.max(0, drift - dt / SETTLE);
    const amount = 0.24 * smoothstep(drift);
    heights = REST.map(
      (rest, index) => rest * (1 + amount * Math.sin(((now / 1000) * 2 * Math.PI) / (PERIODS[index] ?? 2) + (PHASES[index] ?? 0))),
    );
    frame = moving || drift > 0 ? requestAnimationFrame(step) : 0;
  }

  onDestroy(() => cancelAnimationFrame(frame));
</script>

<svg width="56" height="42" viewBox="0 0 56 42" aria-hidden="true">
  {#each heights as height, index (index)}
    <rect x={15 + index * 10} y={21 - 17 * height} width="6" height={34 * height} rx="3" />
  {/each}
</svg>

<style>
  svg {
    flex: none;
    fill: rgb(255 255 255 / 0.9);
  }
</style>
