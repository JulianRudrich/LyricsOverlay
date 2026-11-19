<script lang="ts">
  // The song's card, like the title screen of a karaoke machine: cover, title
  // and artist, large and centred. It opens a song with a long intro and fades
  // into the first line, comes back once the singing is over, and stands in
  // while there are no lyrics to sing, with the reason underneath.
  import type { Translate } from "../lib/i18n";
  import Mark from "../lib/ui/Mark.svelte";
  import Cover from "../overlay/Cover.svelte";
  import type { Track } from "../lib/types";

  let { track, status, fontSize, t }: { track: Track | null; status: string; fontSize: number; t: Translate } = $props();

  const cover = $derived(Math.round(fontSize * 2.6));
</script>

<div class="card" style:font-size="{fontSize}px">
  <div class="reveal">
    {#if track}
      <Cover url={track.artworkUrl} size={cover} />
    {:else}
      <Mark size={cover} />
    {/if}
  </div>
  <p class="title reveal" style:animation-delay="0.06s">{track?.title ?? t("app.name")}</p>
  {#if track?.artist}
    <p class="artist reveal" style:animation-delay="0.12s">{track.artist}</p>
  {/if}
  {#if status}
    <p class="status reveal" style:animation-delay="0.18s" aria-live="polite">{status}</p>
  {/if}
</div>

<style>
  .card {
    box-sizing: border-box;
    height: 100%;
    padding: 0 5%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
  }

  p {
    max-width: 100%;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    margin-top: 0.42em;
    font-size: 0.72em;
    font-weight: 700;
    line-height: 1.2;
  }

  .artist {
    margin-top: 0.3em;
    font-size: 0.44em;
    font-weight: 500;
    line-height: 1.3;
    color: rgb(255 255 255 / 0.6);
  }

  .status {
    margin-top: 1.2em;
    font-size: 0.3em;
    line-height: 1.4;
    white-space: normal;
    color: rgb(255 255 255 / 0.5);
  }
</style>
