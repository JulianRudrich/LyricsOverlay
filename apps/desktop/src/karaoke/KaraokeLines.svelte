<script lang="ts">
  // The karaoke lyrics: the current line large and centred, lighting up word
  // by word, the next ones below it, smaller and dimmer. On a line change the
  // rows move on springs, as in the overlay: the line that is done moves up and
  // fades out, the next one grows into its place. Waits show three large dots
  // that count down to the next line, which brightens meanwhile.
  import { onDestroy, untrack } from "svelte";
  import { prefersReducedMotion } from "svelte/motion";
  import { fade } from "svelte/transition";
  import type { Translate } from "../lib/i18n";
  import { karaokeSlots, type Scene, type Slot } from "../lib/karaoke";
  import { lineFills } from "../lib/lyrics";
  import type { Measure } from "../lib/measure";
  import { Spring } from "../lib/motion";
  import type { LyricLine, SyncLevel } from "../lib/types";
  import BreakDots from "../overlay/BreakDots.svelte";

  interface Props {
    lines: readonly LyricLine[];
    scene: Scene;
    /** Seconds into the lyrics, with the timing corrections applied. */
    position: number;
    sync: SyncLevel;
    fontSize: number;
    width: number;
    height: number;
    /** How many lines show below the current one. */
    upcoming: number;
    playing: boolean;
    measure: Measure;
    t: Translate;
  }

  let { lines, scene, position, sync, fontSize, width, height, upcoming, playing, measure, t }: Props = $props();

  const current = $derived(scene.current);
  const slots = $derived(karaokeSlots(lines, current, { width, height, fontSize, upcoming, measure }));

  // Every row moves to its place on springs; rows keep them while they stay.
  interface Place {
    y: Spring;
    scale: Spring;
    opacity: Spring;
  }
  const places = new Map<number, Place>();
  /** The rows there when the lyrics appear; they come in one after another. */
  const firstRows = new Set(untrack(() => slots.map((slot) => slot.index)));
  const still = $derived(prefersReducedMotion.current);
  let frame = $state(0);
  let animation = 0;
  let last = 0;
  let lastCurrent: number | null = null;

  $effect(() => {
    // Only a line change moves the rows; a new size or font puts them in place at once.
    const moving = current !== lastCurrent && !still;
    lastCurrent = current;
    const shown = new Set<number>();
    for (const slot of slots) {
      shown.add(slot.index);
      let place = places.get(slot.index);
      if (!place) {
        // A row coming in appears in its place and fades in.
        place = {
          y: new Spring(slot.y, { precision: 0.05 }),
          scale: new Spring(slot.scale, { precision: 0.0005 }),
          opacity: new Spring(firstRows.has(slot.index) ? slot.opacity : 0, { precision: 0.002 }),
        };
        places.set(slot.index, place);
      }
      place.y.target = slot.y;
      place.scale.target = slot.scale;
      place.opacity.target = slot.opacity;
      // With reduced motion the rows change places at once and only fade.
      if (!moving) {
        place.y.settle();
        place.scale.settle();
      }
    }
    for (const index of [...places.keys()]) {
      if (!shown.has(index)) places.delete(index);
    }
    if (animation === 0) {
      last = performance.now();
      animation = requestAnimationFrame(step);
    }
  });

  function step(now: number) {
    const dt = Math.min(0.1, Math.max(0, now - last) / 1000);
    last = now;
    let moving = false;
    for (const place of places.values()) {
      for (const spring of [place.y, place.scale, place.opacity]) {
        if (spring.step(dt)) moving = true;
      }
    }
    frame += 1;
    animation = moving ? requestAnimationFrame(step) : 0;
  }

  onDestroy(() => cancelAnimationFrame(animation));

  /** Where a row is now, on its way to its slot. `_frame` makes it follow the animation. */
  function placed(slot: Slot, _frame: number) {
    const place = places.get(slot.index);
    if (!place) return { ...slot, opacity: firstRows.has(slot.index) ? slot.opacity : 0 };
    return { y: place.y.value, scale: place.scale.value, opacity: place.opacity.value };
  }

  const activeLine = $derived(lines[current]);
  const fills = $derived(
    activeLine && scene.dots === null ? lineFills(activeLine, lines[current + 1]?.time ?? null, position, sync) : [],
  );
  const spoken = $derived(
    scene.dots !== null ? (current < 0 ? t("overlay.intro") : t("overlay.instrumentalBreak")) : (activeLine?.text ?? ""),
  );
</script>

<div class="lines" style:font-size="{fontSize}px">
  <p class="spoken" aria-live="polite">{spoken}</p>
  {#each slots as slot (slot.index)}
    {@const place = placed(slot, frame)}
    {@const line = lines[slot.index]}
    {@const opacity = Math.min(1, place.opacity * (1 + (slot.place === 1 ? 0.8 * scene.countdown : 0)))}
    <!-- The next line brightens while the dots count down to it. -->
    <div
      class="row"
      style:width="{width / Math.max(0.05, place.scale)}px"
      style:transform="translate(-50%, {place.y.toFixed(2)}px) scale({place.scale.toFixed(4)})"
      style:opacity={opacity.toFixed(3)}
      out:fade={{ duration: still ? 0 : 200 }}
    >
      <div
        class="line"
        class:reveal={firstRows.has(slot.index)}
        style:animation-delay={firstRows.has(slot.index) ? `${0.12 + 0.07 * Math.max(0, slot.place)}s` : null}
        aria-hidden="true"
      >
        {#if slot.place === 0 && scene.dots !== null}
          <BreakDots progress={scene.dots} size={fontSize} breathing={playing} centered />
        {:else if line && line.text.length === 0}
          <BreakDots progress={0} size={fontSize} breathing={false} centered />
        {:else if line && slot.place === 0 && fills.length > 0}
          {#each fills as word, index (index)}<span style:opacity={(0.3 + 0.7 * word.fill).toFixed(3)}>{word.text}</span>{/each}
        {:else if line}
          {line.text}
        {/if}
      </div>
    </div>
  {/each}
</div>

<style>
  .lines {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    font-weight: 600;
    line-height: 1.25;
  }

  /* Centred on the stage: half its own width to the left, scaled from its top middle. */
  .row {
    position: absolute;
    top: 0;
    left: 50%;
    height: 1.25em;
    transform-origin: 50% 0;
  }

  .line {
    height: 100%;
    overflow: hidden;
    white-space: pre;
    text-overflow: ellipsis;
    text-align: center;
    color: #fff;
  }

  .spoken {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
