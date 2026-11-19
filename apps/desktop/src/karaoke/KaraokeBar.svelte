<script lang="ts">
  // The bar along the bottom of karaoke, shown while the pointer moves: the
  // seek bar with the times, the song, the player's controls, the timing
  // correction for the song, the displays to choose from and the way out.
  import { prefersReducedMotion } from "svelte/motion";
  import { fade } from "svelte/transition";
  import { formatNumber, formatOffset, type Translate } from "../lib/i18n";
  import Glyph from "../lib/ui/Glyph.svelte";
  import type { Capabilities, KaraokeDisplay, Track } from "../lib/types";
  import Controls from "../overlay/Controls.svelte";
  import Scrubber from "../overlay/Scrubber.svelte";

  interface Props {
    track: Track | null;
    capabilities: Capabilities | null;
    playing: boolean;
    /** Seconds into the track. */
    position: number;
    duration: number;
    canSeek: boolean;
    /** Player positions where a chorus starts. */
    marks: readonly number[];
    songOffset: number;
    language: "de" | "en";
    displays: readonly KaraokeDisplay[];
    t: Translate;
    onseek: (position: number) => void;
    onscrub: (held: boolean) => void;
    ontiming: (delta: number) => void;
    ondisplay: (id: string) => void;
    onexit: () => void;
    /** The pointer is over the bar, so it stays. */
    onhold: (held: boolean) => void;
  }

  let {
    track,
    capabilities,
    playing,
    position,
    duration,
    canSeek,
    marks,
    songOffset,
    language,
    displays,
    t,
    onseek,
    onscrub,
    ontiming,
    ondisplay,
    onexit,
    onhold,
  }: Props = $props();

  const step = $derived(formatNumber(language, 0.25, 2));
  const transport = $derived(
    track !== null && !!capabilities && (capabilities.previous || capabilities.playPause || capabilities.next),
  );

  function displayLabel(display: KaraokeDisplay, index: number): string {
    const name = display.name ?? t("karaoke.displayNumber", { number: index + 1 });
    return `${name} · ${Math.round(display.width)} × ${Math.round(display.height)}`;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="bar"
  transition:fade={{ duration: prefersReducedMotion.current ? 0 : 200 }}
  onpointerenter={() => onhold(true)}
  onpointerleave={() => onhold(false)}
>
  {#if track && duration > 0}
    <Scrubber large {position} {duration} expanded={true} seekable={canSeek} {marks} {t} {onseek} {onscrub} />
  {/if}
  <div class="row">
    <div class="song">
      {#if track}
        <span class="title">{track.title}</span>
        {#if track.artist}<span class="artist">{track.artist}</span>{/if}
      {/if}
    </div>
    <div class="transport">
      {#if transport && capabilities}
        <Controls {capabilities} {playing} {t} large />
      {/if}
    </div>
    <div class="actions">
      {#if track}
        <div class="group" role="group" aria-label={t("settings.songOffset")} title={t("settings.songOffset")}>
          <button type="button" aria-label={t("settings.later")} disabled={songOffset <= -10} onclick={() => ontiming(-0.25)}>
            −{step}
          </button>
          <output>{formatOffset(language, songOffset)}</output>
          <button type="button" aria-label={t("settings.earlier")} disabled={songOffset >= 10} onclick={() => ontiming(0.25)}>
            +{step}
          </button>
        </div>
      {/if}
      {#if displays.length > 1}
        <div class="group" role="group" aria-label={t("karaoke.display")} title={t("karaoke.display")}>
          <span class="icon"><Glyph name="display" size={18} /></span>
          {#each displays as display, index (display.id)}
            {@const label = displayLabel(display, index)}
            <button
              type="button"
              class:selected={display.current}
              aria-pressed={display.current}
              aria-label={label}
              title={label}
              onclick={() => ondisplay(display.id)}
            >
              {index + 1}
            </button>
          {/each}
        </div>
      {/if}
      <button type="button" class="exit" aria-label={t("karaoke.exit")} title={t("karaoke.exit")} onclick={onexit}>
        <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M6 6l12 12M18 6 6 18" />
        </svg>
      </button>
    </div>
  </div>
</div>

<style>
  /* Fades up from the black stage, so lines behind it stay out of the way. */
  .bar {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 56px 40px 28px;
    background: linear-gradient(to top, rgb(0 0 0 / 0.92) 40%, rgb(0 0 0 / 0));
  }

  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    gap: 24px;
  }

  .song {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .title,
  .artist {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .title {
    font-size: 16px;
    font-weight: 600;
    line-height: 1.3;
  }

  .artist {
    font-size: 14px;
    line-height: 1.3;
    color: rgb(255 255 255 / 0.6);
  }

  .transport {
    display: flex;
    justify-content: center;
  }

  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 14px;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 3px;
    border-radius: 12px;
    background: rgb(255 255 255 / 0.08);
    box-shadow: inset 0 0 0 0.5px rgb(255 255 255 / 0.12);
  }

  .icon {
    display: grid;
    padding: 0 6px 0 8px;
    color: rgb(255 255 255 / 0.6);
  }

  button,
  output {
    min-width: 40px;
    height: 34px;
    padding: 0 10px;
    border-radius: 9px;
    font: inherit;
    font-size: 14px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    line-height: 34px;
    text-align: center;
    white-space: nowrap;
  }

  output {
    min-width: 72px;
    padding: 0 4px;
    color: rgb(255 255 255 / 0.75);
  }

  button {
    border: 0;
    background: none;
    color: rgb(255 255 255 / 0.85);
    cursor: pointer;
    transition:
      background-color var(--duration-press) var(--ease-out),
      color var(--duration-press) var(--ease-out),
      transform var(--duration-press) var(--ease-out);
  }

  button:hover:not(:disabled, .selected) {
    background: rgb(255 255 255 / 0.12);
    color: #fff;
  }

  button:active:not(:disabled) {
    transform: scale(var(--press-scale));
  }

  button:disabled {
    opacity: 0.35;
    cursor: default;
  }

  button:focus-visible {
    outline: 2px solid rgb(255 255 255 / 0.7);
    outline-offset: 1px;
  }

  button.selected {
    background: #fff;
    color: #000;
  }

  .exit {
    display: grid;
    place-items: center;
    width: 40px;
    min-width: 40px;
    height: 40px;
    padding: 0;
    border-radius: 50%;
    background: rgb(255 255 255 / 0.08);
    box-shadow: inset 0 0 0 0.5px rgb(255 255 255 / 0.12);
  }

  .exit svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
  }
</style>
