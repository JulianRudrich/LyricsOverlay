<script lang="ts">
  // The header, like the Now Playing widget: cover art, title and artist,
  // entering one after another for each song. While the pointer is over the
  // overlay the playback controls slide in; otherwise short notes about the
  // lyrics sit there.
  import { prefersReducedMotion } from "svelte/motion";
  import { fade } from "svelte/transition";
  import type { Translate } from "../lib/i18n";
  import Icon from "../lib/Icon.svelte";
  import { songExit } from "../lib/motion";
  import type { Capabilities, Track } from "../lib/types";
  import Controls from "./Controls.svelte";
  import Cover from "./Cover.svelte";

  interface Props {
    track: Track | null;
    playerName: string;
    /** The controls the player offers, while they are shown. */
    controls: Capabilities | null;
    playing: boolean;
    notSynced: boolean;
    ownLyrics: boolean;
    paused: boolean;
    t: Translate;
    onsettings: () => void;
  }

  let { track, playerName, controls, playing, notSynced, ownLyrics, paused, t, onsettings }: Props = $props();

  const details = $derived(track ? [track.title, track.artist, playerName].filter(Boolean).join(" · ") : undefined);
  const fadeMotion = $derived({ duration: prefersReducedMotion.current ? 0 : 200 });
</script>

<header>
  <div class="now-playing" title={details}>
    {#key track?.key ?? ""}
      <div class="song" out:songExit={{ reduced: prefersReducedMotion.current }}>
        <div class="reveal"><Cover url={track?.artworkUrl ?? null} /></div>
        <div class="names">
          <span class="title reveal" style:animation-delay="0.05s">{track?.title ?? t("app.name")}</span>
          {#if track?.artist}
            <span class="artist reveal" style:animation-delay="0.1s">{track.artist}</span>
          {/if}
        </div>
      </div>
    {/key}
  </div>
  {#if controls}
    <Controls capabilities={controls} {playing} {t} />
  {:else}
    {#if notSynced}
      <span class="caption" transition:fade={fadeMotion}>{t("overlay.notSynced")}</span>
    {/if}
    {#if ownLyrics}
      <span class="badge" title={t("overlay.ownLyrics")} aria-label={t("overlay.ownLyrics")} transition:fade={fadeMotion}>
        <Icon name="document" size={12} />
      </span>
    {/if}
    {#if paused}
      <span class="badge" title={t("overlay.paused")} aria-label={t("overlay.paused")} transition:fade={fadeMotion}>
        <Icon name="pause" size={12} />
      </span>
    {/if}
  {/if}
  <button class="settings" onclick={onsettings} title={t("overlay.openSettings")} aria-label={t("overlay.openSettings")}>
    <svg width="19" height="19" viewBox="0 0 24 24" aria-hidden="true">
      <circle class="ring" cx="12" cy="12" r="9.5" />
      <circle cx="7.6" cy="12" r="1.35" />
      <circle cx="12" cy="12" r="1.35" />
      <circle cx="16.4" cy="12" r="1.35" />
    </svg>
  </button>
</header>

<style>
  header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 36px;
  }

  .now-playing {
    flex: 1;
    min-width: 0;
    display: grid;
  }

  /* An old and a new song share the place while one leaves and the other arrives. */
  .song {
    grid-area: 1 / 1;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .names {
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
    font-size: 13px;
    font-weight: 600;
    line-height: 1.3;
  }

  .artist {
    font-size: 11px;
    line-height: 1.3;
    color: rgb(255 255 255 / 0.6);
  }

  .caption {
    flex: none;
    font-size: 10px;
    color: rgb(255 255 255 / 0.35);
  }

  .badge {
    flex: none;
    display: grid;
    place-items: center;
    color: rgb(255 255 255 / 0.35);
  }

  .settings {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: 0;
    padding: 0;
    background: none;
    color: rgb(255 255 255 / 0.6);
    cursor: pointer;
    transition:
      color 0.15s,
      transform 0.14s cubic-bezier(0.23, 1, 0.32, 1);
  }

  .settings:hover,
  .settings:focus-visible {
    color: #fff;
  }

  .settings:active {
    transform: scale(0.94);
  }

  .settings svg {
    fill: currentColor;
  }

  .settings .ring {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
  }

  @media (prefers-reduced-motion: reduce) {
    .settings {
      transition: color 0.15s;
    }
  }
</style>
