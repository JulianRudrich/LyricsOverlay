<script lang="ts">
  // The progress line under the lyrics. While the pointer is over the overlay
  // it grows into a scrubber with the elapsed and remaining time: click or drag
  // to seek, click a dot to jump to a chorus. It keeps its place in the layout,
  // so the lyrics never move.
  import { prefersReducedMotion } from "svelte/motion";
  import { fade } from "svelte/transition";
  import type { Translate } from "../lib/i18n";
  import { clamp } from "../lib/lyrics";
  import { formatRemaining, formatTime } from "../lib/player";

  interface Props {
    /** Seconds into the track. */
    position: number;
    /** Length of the track in seconds, more than zero. */
    duration: number;
    /** The pointer is over the overlay. */
    expanded: boolean;
    /** The player can seek. */
    seekable: boolean;
    /** Player positions where a chorus starts. */
    marks: readonly number[];
    t: Translate;
    onseek: (position: number) => void;
    /** Reports a drag starting and ending, so the overlay stays open meanwhile. */
    onscrub: (held: boolean) => void;
    /** Larger, for the karaoke bar. */
    large?: boolean;
  }

  let { position, duration, expanded, seekable, marks, t, onseek, onscrub, large = false }: Props = $props();

  /** Seconds one arrow key moves. */
  const STEP = 5;
  const STEPS: Record<string, number> = { ArrowLeft: -STEP, ArrowDown: -STEP, ArrowRight: STEP, ArrowUp: STEP };

  let rail = $state<HTMLElement>();
  /** Where the pointer holds the line while dragging, as a share of the track. */
  let held = $state<number | null>(null);

  const interactive = $derived(expanded && seekable);
  const shown = $derived(held === null ? clamp(position, 0, duration) : held * duration);
  const fraction = $derived(clamp(shown / duration, 0, 1));
  const fadeMotion = $derived({ duration: prefersReducedMotion.current ? 0 : 150 });

  function fractionAt(event: PointerEvent): number {
    const box = rail?.getBoundingClientRect();
    if (!box || box.width <= 0) return fraction;
    return clamp((event.clientX - box.left) / box.width, 0, 1);
  }

  function press(event: PointerEvent) {
    if (!interactive || event.button !== 0) return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    held = fractionAt(event);
    onscrub(true);
  }

  function move(event: PointerEvent) {
    if (held !== null) held = fractionAt(event);
  }

  // One seek when the pointer lets go, wherever it is by then.
  function release(event: PointerEvent) {
    if (held === null) return;
    const target = fractionAt(event) * duration;
    held = null;
    onscrub(false);
    onseek(target);
  }

  function cancel() {
    if (held === null) return;
    held = null;
    onscrub(false);
  }

  function key(event: KeyboardEvent) {
    const step = STEPS[event.key];
    if (event.key === "Home") onseek(0);
    else if (step !== undefined) onseek(clamp(shown + step, 0, duration));
    else return;
    event.preventDefault();
  }
</script>

<div class="scrubber" class:large class:expanded class:interactive class:held={held !== null} data-no-drag={interactive ? "" : undefined}>
  {#if expanded}
    <span class="time" transition:fade={fadeMotion}>{formatTime(shown)}</span>
  {/if}
  <div class="rail" bind:this={rail}>
    <div
      class="bar"
      role="slider"
      tabindex={interactive ? 0 : -1}
      aria-hidden={interactive ? undefined : "true"}
      aria-label={t("player.position")}
      aria-valuemin={0}
      aria-valuemax={Math.floor(duration)}
      aria-valuenow={Math.floor(shown)}
      aria-valuetext={t("player.positionValue", { elapsed: formatTime(shown), duration: formatTime(duration) })}
      onpointerdown={press}
      onpointermove={move}
      onpointerup={release}
      onpointercancel={cancel}
      onlostpointercapture={cancel}
      onkeydown={key}
    >
      <div class="track"><div class="fill" style:width="{fraction * 100}%"></div></div>
    </div>
    {#if interactive}
      {#each marks as mark, index (index)}
        {@const label = t("player.chorus", { time: formatTime(mark) })}
        <button
          class="mark"
          style:left="{clamp(mark / duration, 0, 1) * 100}%"
          title={label}
          aria-label={label}
          onclick={() => onseek(mark)}
          transition:fade={fadeMotion}
        ></button>
      {/each}
    {/if}
  </div>
  {#if expanded}
    <span class="time" transition:fade={fadeMotion}>−{formatRemaining(shown, duration)}</span>
  {/if}
</div>

<style>
  /* Sits close to the faded bottom edge of the lyrics; its height never changes. */
  .scrubber {
    flex: none;
    height: 14px;
    margin-top: -4px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .time {
    flex: none;
    font-size: 10px;
    font-weight: 500;
    line-height: 1;
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.62);
  }

  /* The whole height of the row, so the thin line is easy to hit. */
  .rail {
    flex: 1;
    position: relative;
    height: 100%;
    pointer-events: none;
  }

  .interactive .rail {
    pointer-events: auto;
  }

  .bar {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    outline: none;
  }

  .interactive .bar {
    cursor: pointer;
  }

  .track {
    flex: 1;
    height: 2px;
    border-radius: 1px;
    background: rgb(255 255 255 / 0.1);
    overflow: hidden;
    transition:
      height 0.15s cubic-bezier(0.23, 1, 0.32, 1),
      background-color 0.15s;
  }

  .expanded .track,
  .held .track {
    height: 4px;
    border-radius: 2px;
    background: rgb(255 255 255 / 0.16);
  }

  .bar:focus-visible .track {
    box-shadow: 0 0 0 2px rgb(255 255 255 / 0.45);
  }

  .fill {
    min-width: 2px;
    height: 100%;
    border-radius: inherit;
    background: rgb(255 255 255 / 0.35);
    transition: background-color 0.15s;
  }

  .expanded .fill,
  .held .fill {
    min-width: 4px;
    background: rgb(255 255 255 / 0.85);
  }

  .mark {
    position: absolute;
    top: 50%;
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    margin: -7px 0 0 -7px;
    border: 0;
    padding: 0;
    border-radius: 50%;
    background: none;
    cursor: pointer;
  }

  .mark::before {
    content: "";
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #fff;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.25);
    transition: transform 0.14s cubic-bezier(0.23, 1, 0.32, 1);
  }

  .mark:hover::before,
  .mark:focus-visible::before {
    transform: scale(1.25);
  }

  .mark:active::before {
    transform: scale(0.8);
  }

  /* The karaoke bar is used from further away. */
  .large {
    height: 24px;
    margin-top: 0;
    gap: 14px;
  }

  .large .time {
    font-size: 13px;
  }

  .large.expanded .track,
  .large.held .track {
    height: 6px;
    border-radius: 3px;
  }

  .large .mark {
    width: 20px;
    height: 20px;
    margin: -10px 0 0 -10px;
  }

  .large .mark::before {
    width: 10px;
    height: 10px;
  }

  @media (prefers-reduced-motion: reduce) {
    .track,
    .fill,
    .mark::before {
      transition: none;
    }
  }
</style>
