<script lang="ts">
  // "On Spotify": Spotify's own ranking of the listener's top artists and
  // tracks for 4 weeks, 6 months or a year. Each row opens in Spotify.
  import type { MessageKey, Translate } from "../lib/i18n";
  import { spotify, type TimeRange, type TopItem, type TopLists } from "./api";

  let { t }: { t: Translate } = $props();

  const ranges: TimeRange[] = ["short", "medium", "long"];
  const rangeKeys: Record<TimeRange, MessageKey> = {
    short: "spotify.rangeShort",
    medium: "spotify.rangeMedium",
    long: "spotify.rangeLong",
  };

  let range = $state<TimeRange>("short");
  let lists = $state<TopLists | null>(null);
  let failed = $state(false);

  $effect(() => {
    const wanted = range;
    let current = true;
    lists = null;
    failed = false;
    spotify
      .top(wanted)
      .then((next) => {
        if (current) lists = next;
      })
      .catch(() => {
        if (current) failed = true;
      });
    return () => {
      current = false;
    };
  });
</script>

<section class="card">
  <div class="head">
    <div class="titles">
      <strong>{t("spotify.onSpotify")}</strong>
      <span class="hint">{t("spotify.topHint")}</span>
    </div>
    <div class="segmented" role="radiogroup" aria-label={t("spotify.onSpotify")}>
      {#each ranges as option (option)}
        <button role="radio" aria-checked={range === option} class:selected={range === option} onclick={() => (range = option)}>
          {t(rangeKeys[option])}
        </button>
      {/each}
    </div>
  </div>
  {#if failed}
    <p class="hint">{t("spotify.topError")}</p>
  {:else if !lists}
    <p class="hint">{t("spotify.loading")}</p>
  {:else if lists.artists.length === 0 && lists.tracks.length === 0}
    <p class="hint">{t("spotify.topEmpty")}</p>
  {:else}
    <div class="columns">
      {@render list(t("spotify.topArtists"), lists.artists)}
      {@render list(t("spotify.topTracks"), lists.tracks)}
    </div>
  {/if}
</section>

{#snippet list(title: string, items: TopItem[])}
  <div class="list">
    <span class="caption">{title}</span>
    <ol>
      {#each items as item, index (index)}
        <li>
          <button onclick={() => spotify.open(item.uri)} title={[item.name, item.artists.join(", ")].filter(Boolean).join(" – ")}>
            <span class="rank">{index + 1}</span>
            <span class="names">
              <span class="name">{item.name}</span>
              {#if item.artists.length > 0}
                <span class="artists">{item.artists.join(", ")}</span>
              {/if}
            </span>
          </button>
        </li>
      {/each}
    </ol>
  </div>
{/snippet}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    border-radius: 12px;
    background: rgb(255 255 255 / 0.05);
    border: 1px solid rgb(255 255 255 / 0.08);
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }

  .titles {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  strong {
    font-weight: 500;
  }

  .hint {
    margin: 0;
    font-size: 12px;
    line-height: 1.4;
    color: rgb(255 255 255 / 0.55);
  }

  .segmented {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 9px;
    background: rgb(255 255 255 / 0.06);
  }

  .segmented button,
  .list button {
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: transform 0.12s ease;
  }

  .segmented button {
    padding: 4px 10px;
    border-radius: 7px;
    font-size: 12px;
    color: rgb(255 255 255 / 0.7);
  }

  .segmented button.selected {
    background: rgb(255 255 255 / 0.14);
    color: #fff;
  }

  .segmented button:active,
  .list button:active {
    transform: scale(0.97);
  }

  .columns {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
  }

  .caption {
    display: block;
    margin-bottom: 6px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: rgb(255 255 255 / 0.4);
  }

  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .list button {
    width: 100%;
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 6px;
    border-radius: 7px;
    text-align: left;
  }

  .list button:hover {
    background: rgb(255 255 255 / 0.08);
  }

  .rank {
    flex: none;
    width: 16px;
    font-size: 11px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.4);
  }

  .names {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .name,
  .artists {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .name {
    font-size: 12.5px;
  }

  .artists {
    font-size: 11px;
    color: rgb(255 255 255 / 0.55);
  }

  @media (prefers-reduced-motion: reduce) {
    .segmented button,
    .list button {
      transition: none;
    }
  }
</style>
