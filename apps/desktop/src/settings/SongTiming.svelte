<script lang="ts">
  import { api } from "../lib/api";
  import { formatNumber, formatOffset, type MessageKey } from "../lib/i18n";
  import { reveal } from "../lib/motion";
  import Button from "../lib/ui/Button.svelte";
  import Card from "../lib/ui/Card.svelte";
  import Note from "../lib/ui/Note.svelte";
  import Row from "../lib/ui/Row.svelte";
  import Slider from "../lib/ui/Slider.svelte";
  import Toggle from "../lib/ui/Toggle.svelte";
  import OwnLyrics from "./OwnLyrics.svelte";
  import type { PaneContext } from "./panes";

  let { ctx }: { ctx: PaneContext } = $props();
  const t = $derived(ctx.t);
  const settings = $derived(ctx.settings);
  const app = $derived(ctx.app);
  const language = $derived(ctx.language);

  const track = $derived(ctx.playback?.state === "track" ? ctx.playback.track : null);
  const lyrics = $derived(ctx.lyrics && ctx.lyrics.trackKey === track?.key ? ctx.lyrics : null);
  const songOffset = $derived(lyrics?.songOffset ?? 0);
  const step = $derived(formatNumber(language, 0.25, 2));

  const summary = $derived.by(() => {
    if (!lyrics?.lyrics || lyrics.status !== "ready") return "";
    const origin =
      lyrics.origin === "local" ? t("settings.lyricsLocal") : lyrics.origin === "demo" ? t("settings.lyricsDemo") : t("settings.lyricsOnline");
    return `${origin} · ${t(`sync.${lyrics.lyrics.sync}` as MessageKey)}`;
  });
</script>

<Card>
  {#if app.demo}
    <span class="badge">{t("settings.demo")}</span>
  {/if}
  {#if track}
    {#key track.key}
      <div use:reveal>
        <Row
          card={false}
          icon="note"
          title={track.title}
          hint={[track.artist, ctx.playback?.player?.name].filter(Boolean).join(" · ")}
        />
      </div>
    {/key}
    <div class="field">
      <div class="heading">
        <span class="label">{t("settings.songOffset")}</span>
        <output>{formatOffset(language, songOffset)}</output>
      </div>
      <div class="actions">
        <Button aria-label={t("settings.later")} disabled={songOffset <= -10} onclick={() => ctx.adjustSongOffset(-0.25)}>−{step}</Button>
        <Button aria-label={t("settings.earlier")} disabled={songOffset >= 10} onclick={() => ctx.adjustSongOffset(0.25)}>+{step}</Button>
        <span class="spacer"></span>
        <Button disabled={songOffset === 0} onclick={() => ctx.resetSongOffset()}>{t("settings.reset")}</Button>
      </div>
    </div>
    <div class="actions">
      <span class="summary">{summary}</span>
      <span class="spacer"></span>
      {#if !app.demo}
        <Button onclick={() => api.reloadLyrics()}>{t("settings.searchAgain")}</Button>
      {/if}
    </div>
  {:else if !app.demo}
    <Row card={false} icon="note" title={t("settings.noTrack")} />
  {/if}
</Card>

{#if track && !app.demo}
  <OwnLyrics {track} {lyrics} {t} />
{/if}

<Card>
  <Slider
    label={t("settings.globalOffset")}
    value={settings.globalOffset}
    min={-5}
    max={5}
    step={0.1}
    display={t("unit.seconds", { value: formatNumber(language, settings.globalOffset, 1) })}
    onchange={(globalOffset) => ctx.update({ globalOffset })}
  />
</Card>
<Note>{t("settings.offsetHint")}</Note>

<Row icon="search" title={t("settings.lookupOnline")} hint={t("settings.privacy")}>
  <Toggle
    label={t("settings.lookupOnline")}
    checked={settings.lookupOnline}
    disabled={app.demo}
    onchange={(lookupOnline) => ctx.update({ lookupOnline })}
  />
</Row>

<style>
  .badge {
    align-self: flex-start;
    padding: 3px 8px;
    border-radius: 6px;
    background: rgb(255 255 255 / 0.08);
    font-size: 11px;
    font-weight: 500;
    color: rgb(255 255 255 / 0.75);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .heading {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .label {
    font-size: 13px;
    font-weight: 500;
  }

  output {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.55);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .spacer {
    flex: 1;
  }

  .summary {
    min-width: 0;
    overflow: hidden;
    font-size: 11.5px;
    color: rgb(255 255 255 / 0.55);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
</style>
