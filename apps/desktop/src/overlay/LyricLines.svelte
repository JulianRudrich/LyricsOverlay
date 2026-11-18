<script lang="ts">
  // The lyrics: rows around the active line, each keyed by its line, so on a
  // line change the rows scroll up and the next line grows into the active
  // place. Neighbours are the same text, smaller and fainter; long lines shrink
  // to fit. A press on a line jumps there, unless it moves: then it drags the
  // window.
  import { onDestroy, untrack } from "svelte";
  import { prefersReducedMotion } from "svelte/motion";
  import { fade } from "svelte/transition";
  import type { Translate } from "../lib/i18n";
  import { clamp, lineFills, lineText } from "../lib/lyrics";
  import type { Measure } from "../lib/measure";
  import { Spring } from "../lib/motion";
  import type { Layout, LyricLine, SyncLevel } from "../lib/types";
  import BreakDots from "./BreakDots.svelte";

  interface Props {
    lines: readonly LyricLine[];
    /** Index of the active line, -1 before the first. */
    current: number;
    /** Seconds into the lyrics, with the timing corrections applied. */
    position: number;
    sync: SyncLevel;
    layout: Layout;
    fontSize: number;
    width: number;
    height: number;
    playing: boolean;
    measure: Measure;
    /** Jumps the song to a line; null when that is not possible. */
    onjump: ((line: LyricLine) => void) | null;
    /** Starts moving the window. */
    onwindowdrag: () => void;
    t: Translate;
  }

  let { lines, current, position, sync, layout, fontSize, width, height, playing, measure, onjump, onwindowdrag, t }: Props =
    $props();

  /** Neighbouring lines are the active line's text at this size. */
  const NEIGHBOUR = 0.78;
  /** Line height as a share of the font size. */
  const LINE_HEIGHT = 1.19;
  const GAP = 4;
  /** Extra room above and below the active line. */
  const ACTIVE_PADDING = 4;
  /** Pixels a press may move before it drags the window instead of jumping. */
  const DRAG_DISTANCE = 4;

  interface Slot {
    index: number;
    /** Place from the top, for the staggered entrance. */
    order: number;
    active: boolean;
    y: number;
    scale: number;
    opacity: number;
    blur: number;
  }

  const reach = $derived(layout === "context" ? 2 : layout === "focus" ? 1 : 0);

  const slots = $derived.by((): Slot[] => {
    const lineHeight = fontSize * LINE_HEIGHT;
    const rows = [];
    for (let index = current - reach; index <= current + reach; index += 1) {
      const distance = Math.abs(index - current);
      const line = lines[index];
      const active = distance === 0;
      const fit = line ? Math.min(1, (width * 0.97) / Math.max(1, measure(lineText(line)))) : 1;
      const scale = Math.min(active ? 1 : NEIGHBOUR, fit);
      rows.push({
        index,
        active,
        scale,
        opacity: !line && !active ? 0 : active ? 1 : distance === 1 ? 0.42 : 0.14,
        blur: distance >= 2 ? 1.2 : 0,
        extent: lineHeight * scale + (active ? 2 * ACTIVE_PADDING : 0),
      });
    }
    const total = rows.reduce((sum, row) => sum + row.extent, 0) + GAP * (rows.length - 1);
    let top = (height - total) / 2;
    return rows.map(({ extent, ...row }, order) => {
      const y = top + (row.active ? ACTIVE_PADDING : 0);
      top += extent + GAP;
      return { ...row, order, y };
    });
  });

  // Every row moves to its place on springs; rows keep them while they stay.
  interface Place {
    y: Spring;
    scale: Spring;
    opacity: Spring;
    blur: Spring;
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
    // Only a line change moves the rows; a new size, font or layout puts them in place at once.
    const scrolling = current !== lastCurrent && !still;
    lastCurrent = current;
    const shown = new Set<number>();
    for (const slot of slots) {
      shown.add(slot.index);
      let place = places.get(slot.index);
      if (!place) {
        // A row scrolling in appears in its place and fades in.
        place = {
          y: new Spring(slot.y, { precision: 0.05 }),
          scale: new Spring(slot.scale, { precision: 0.0005 }),
          opacity: new Spring(firstRows.has(slot.index) ? slot.opacity : 0, { precision: 0.002 }),
          blur: new Spring(slot.blur, { precision: 0.01 }),
        };
        places.set(slot.index, place);
      }
      place.y.target = slot.y;
      place.scale.target = slot.scale;
      place.opacity.target = slot.opacity;
      place.blur.target = slot.blur;
      // With reduced motion the rows change places at once and only fade.
      if (!scrolling) {
        place.y.settle();
        place.scale.settle();
        place.blur.settle();
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
      for (const spring of [place.y, place.scale, place.opacity, place.blur]) {
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
    return { y: place.y.value, scale: place.scale.value, opacity: place.opacity.value, blur: place.blur.value };
  }

  const activeLine = $derived(lines[current]);
  const fills = $derived(activeLine ? lineFills(activeLine, lines[current + 1]?.time ?? null, position, sync) : []);

  /** Progress through the intro or an instrumental break, or null while a line is sung. */
  const waiting = $derived.by((): number | null => {
    if (current < 0) {
      const first = lines[0];
      if (!first) return null;
      return first.time > 0 ? clamp(position / first.time, 0, 1) : 1;
    }
    if (!activeLine || activeLine.text.length > 0) return null;
    const end = lines[current + 1]?.time ?? activeLine.time + 4;
    return clamp((position - activeLine.time) / Math.max(0.1, end - activeLine.time), 0, 1);
  });

  let pressed: { x: number; y: number } | null = null;
  let dragged = false;

  function press(event: PointerEvent) {
    if (event.button !== 0) return;
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
    pressed = { x: event.clientX, y: event.clientY };
    dragged = false;
  }

  function move(event: PointerEvent) {
    if (!pressed) return;
    if ((event.buttons & 1) === 0) {
      pressed = null;
      return;
    }
    if (Math.hypot(event.clientX - pressed.x, event.clientY - pressed.y) > DRAG_DISTANCE) {
      pressed = null;
      dragged = true;
      (event.currentTarget as Element).releasePointerCapture(event.pointerId);
      onwindowdrag();
    }
  }

  function jump(line: LyricLine) {
    pressed = null;
    if (dragged) {
      dragged = false;
      return;
    }
    onjump?.(line);
  }
</script>

<div class="lyrics" style:font-size="{fontSize}px">
  <p class="spoken" aria-live="polite">
    {activeLine ? activeLine.text || t("overlay.instrumentalBreak") : t("overlay.intro")}
  </p>
  {#each slots as slot (slot.index)}
    {@const place = placed(slot, frame)}
    {@const line = lines[slot.index]}
    <div
      class="row"
      style:width="{width / Math.max(0.05, place.scale)}px"
      style:transform="translateY({place.y.toFixed(2)}px) scale({place.scale.toFixed(4)})"
      style:opacity={place.opacity.toFixed(3)}
      style:filter={place.blur > 0.01 ? `blur(${place.blur.toFixed(2)}px)` : null}
      out:fade={{ duration: still ? 0 : 200 }}
    >
      <div
        class="inner"
        class:reveal={firstRows.has(slot.index)}
        style:animation-delay={firstRows.has(slot.index) ? `${0.12 + 0.05 * slot.order}s` : null}
      >
        {#if line && onjump}
          <button
            class="line"
            aria-label={t("player.jumpToLine", { line: line.text || t("overlay.instrumentalBreak") })}
            onpointerdown={press}
            onpointermove={move}
            onclick={() => jump(line)}
          >
            {@render content(slot, line)}
          </button>
        {:else}
          <div class="line" aria-hidden="true">{@render content(slot, line)}</div>
        {/if}
      </div>
    </div>
  {/each}
</div>

{#snippet content(slot: Slot, line: LyricLine | undefined)}
  {#if slot.active && waiting !== null}
    <BreakDots progress={waiting} size={fontSize} breathing={playing} />
  {:else if slot.active && fills.length > 0}
    {#each fills as word, index (index)}<span style:opacity={(0.3 + 0.7 * word.fill).toFixed(3)}>{word.text}</span>{/each}
  {:else if line}
    {lineText(line)}
  {/if}
{/snippet}

<style>
  /* The top and bottom of the lyrics fade out, so lines never end at a hard edge. */
  .lyrics {
    position: relative;
    height: 100%;
    overflow: hidden;
    font-weight: 600;
    line-height: 1.19;
    -webkit-mask-image: linear-gradient(to bottom, transparent 0, #000 12%, #000 88%, transparent 100%);
    mask-image: linear-gradient(to bottom, transparent 0, #000 12%, #000 88%, transparent 100%);
  }

  .row {
    position: absolute;
    top: 0;
    left: 0;
    height: 1.19em;
    transform-origin: 0 0;
  }

  .inner {
    height: 100%;
  }

  .line {
    display: block;
    width: 100%;
    height: 100%;
    overflow: hidden;
    white-space: pre;
    text-overflow: ellipsis;
    text-align: left;
    color: #fff;
    text-shadow: 0 1px 3px rgb(0 0 0 / 0.45);
  }

  button.line {
    border: 0;
    padding: 0;
    background: none;
    font: inherit;
    cursor: pointer;
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
