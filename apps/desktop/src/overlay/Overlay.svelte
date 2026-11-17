<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, untrack } from "svelte";
  import { fly } from "svelte/transition";
  import { api, subscribe } from "../lib/api";
  import { PlaybackClock } from "../lib/clock";
  import { translator, type MessageKey } from "../lib/i18n";
  import Icon from "../lib/Icon.svelte";
  import { currentIndex, lineText, wordFills } from "../lib/lyrics";
  import type { AppInfo, LyricLine, LyricsView, Playback, Settings } from "../lib/types";

  let settings = $state<Settings | null>(null);
  let playback = $state<Playback | null>(null);
  let lyricsView = $state<LyricsView | null>(null);
  let app = $state<AppInfo | null>(null);
  let position = $state(0);

  const clock = new PlaybackClock();
  const t = $derived(translator(app?.language ?? "en"));

  const track = $derived(playback?.state === "track" ? playback.track : null);
  const lyrics = $derived(
    lyricsView?.status === "ready" && lyricsView.lyrics && lyricsView.trackKey === track?.key
      ? lyricsView.lyrics
      : null,
  );
  const lines: LyricLine[] = $derived(lyrics?.lines ?? []);
  const offset = $derived((settings?.globalOffset ?? 0) + (lyricsView?.songOffset ?? 0));
  const effective = $derived(position + offset);
  const active = $derived(currentIndex(lines, effective));
  const activeLine = $derived(active === null ? undefined : lines[active]);
  const fills = $derived(
    activeLine && active !== null ? wordFills(activeLine, lines[active + 1]?.time ?? null, effective) : [],
  );
  const showLyrics = $derived(lines.length > 0 && playback?.position !== null);
  const layout = $derived(settings?.layout ?? "context");
  const fontSize = $derived(settings?.fontSize ?? 25);
  const progress = $derived(
    track?.duration && track.duration > 0 ? Math.min(1, Math.max(0, position / track.duration)) : null,
  );

  const playerName = $derived(playback?.player?.name ?? "");
  const sourceName = $derived.by(() => {
    const source = settings?.source ?? "automatic";
    return source === "automatic" ? "" : t(`source.${source}` as MessageKey);
  });

  const status = $derived.by((): string => {
    if (!playback || !settings) return "";
    if (playback.state === "idle") {
      return sourceName ? t("status.idleSource", { player: sourceName }) : t("status.idleAutomatic");
    }
    if (playback.state === "problem") {
      const player = playerName;
      switch (playback.problem?.kind) {
        case "permissionDenied":
          return t("status.permission", { player });
        case "noTrackData":
          return t("status.noTrackData", { player });
        default:
          return t("status.readFailed", { player });
      }
    }
    switch (lyricsView?.status) {
      case "loading":
        return t("lyrics.loading");
      case "instrumental":
        return t("lyrics.instrumental");
      case "notFound":
        return t("lyrics.notFound");
      case "offline":
        return t("lyrics.offline");
      case "rateLimited":
        return t("lyrics.rateLimited");
      case "localOnly":
        return t("lyrics.localOnly");
      case "failed":
        return t("lyrics.failed");
      case "ready":
        return playback.position === null ? t("status.noPosition", { player: playerName }) : "";
      default:
        return t("lyrics.loading");
    }
  });

  // With LYRICS_OVERLAY_DEBUG set, every line change is reported for end-to-end checks.
  let reported: number | null | undefined;
  $effect(() => {
    if (!app?.debug || !showLyrics || active === reported) return;
    reported = active;
    const text = activeLine ? activeLine.text : "";
    void api.reportLine(Number(untrack(() => effective).toFixed(2)), text);
  });

  function applyPlayback(next: Playback) {
    playback = next;
    clock.update(
      {
        position: next.position,
        playing: next.playing,
        rate: next.rate,
        duration: next.track?.duration ?? null,
        trackKey: next.track?.key ?? null,
      },
      performance.now(),
    );
    position = clock.positionAt(performance.now());
    startTicking();
  }

  let frame = 0;
  function tick() {
    position = clock.positionAt(performance.now());
    frame = clock.isPlaying ? requestAnimationFrame(tick) : 0;
  }
  function startTicking() {
    if (frame === 0 && clock.isPlaying) frame = requestAnimationFrame(tick);
  }

  function drag(event: PointerEvent) {
    if (event.button !== 0 || (event.target as HTMLElement).closest("button")) return;
    void getCurrentWindow().startDragging();
  }

  onMount(() => {
    let stop: (() => void) | undefined;
    let alive = true;
    void (async () => {
      const unlisten = await subscribe({
        playback: applyPlayback,
        lyrics: (next) => (lyricsView = next),
        settings: (next) => (settings = next),
        app: (next) => (app = next),
      });
      if (!alive) return unlisten();
      stop = unlisten;
      const snapshot = await api.snapshot();
      settings = snapshot.settings;
      lyricsView = snapshot.lyrics;
      app = snapshot.app;
      applyPlayback(snapshot.playback);
    })();
    return () => {
      alive = false;
      stop?.();
      cancelAnimationFrame(frame);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<main
  class="overlay"
  style:--background-opacity={settings?.backgroundOpacity ?? 0.78}
  style:opacity={settings?.overlayOpacity ?? 1}
  onpointerdown={drag}
>
  <header>
    <button class="settings" onclick={() => api.openSettings()} title={t("overlay.openSettings")} aria-label={t("overlay.openSettings")}>
      <Icon name="waveform" size={14} />
    </button>
    <div class="track">
      {#if track}
        <span class="title">{track.title}</span>
        <span class="artist">{[track.artist, playerName].filter(Boolean).join(" · ")}</span>
      {:else}
        <span class="title">{t("app.name")}</span>
      {/if}
    </div>
    {#if lyrics?.sync === "unsynced"}
      <span class="tag">{t("overlay.notSynced")}</span>
    {/if}
    {#if lyricsView?.origin === "local" && lyrics}
      <span class="badge" title={t("overlay.ownLyrics")} aria-label={t("overlay.ownLyrics")}><Icon name="document" size={13} /></span>
    {/if}
    {#if track && !playback?.playing}
      <span class="badge muted" title={t("overlay.paused")} aria-label={t("overlay.paused")}><Icon name="pause" size={13} /></span>
    {/if}
  </header>

  <section class="lines" class:caption={layout === "caption"} aria-live="polite">
    {#if showLyrics}
      {#if layout === "context" && active !== null && active > 0}
        <p class="previous" style:font-size="{Math.max(11, fontSize * 0.55)}px">{lineText(lines[active - 1])}</p>
      {/if}
      {#key active}
        <p
          class="current"
          style:font-size="{fontSize}px"
          aria-label={activeLine ? (activeLine.text || t("overlay.instrumentalBreak")) : t("overlay.intro")}
          in:fly={{ y: 6, duration: 160 }}
        >
          {#if activeLine && fills.length > 0}
            {#each fills as word, index (index)}
              <span class="word" style:--fill="{(word.fill * 100).toFixed(1)}%">{word.text}</span>
            {/each}
          {:else}
            {activeLine ? lineText(activeLine) : "♪"}
          {/if}
        </p>
      {/key}
      {#if layout !== "caption" && lines[(active ?? -1) + 1]}
        <p class="next" style:font-size="{Math.max(12, fontSize * 0.63)}px">{lineText(lines[(active ?? -1) + 1])}</p>
      {/if}
    {:else}
      <div class="status">
        <p>{status}</p>
        {#if lyricsView?.canRetry && playback?.state === "track"}
          <button class="retry" onclick={() => api.reloadLyrics()} title={t("overlay.retry")} aria-label={t("overlay.retry")}>
            <Icon name="retry" size={15} />
          </button>
        {/if}
      </div>
    {/if}
  </section>

  {#if progress !== null}
    <div class="progress" aria-hidden="true"><div style:width="{progress * 100}%"></div></div>
  {/if}
</main>

<style>
  .overlay {
    --accent: #7af2d4;
    --base: 14 19 24;
    box-sizing: border-box;
    height: 100vh;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 17px;
    border-radius: 18px;
    background: rgb(var(--base) / var(--background-opacity));
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / calc(var(--background-opacity) * 0.15));
    color: #fff;
    overflow: hidden;
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
  }

  header {
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 28px;
    font-size: 11px;
    font-weight: 600;
  }

  button {
    border: 0;
    padding: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .settings {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 7px;
    color: var(--accent);
    background: rgb(122 242 212 / 0.13);
  }

  .settings:hover,
  .settings:focus-visible {
    background: rgb(122 242 212 / 0.24);
  }

  .track {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .title,
  .artist {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .title {
    font-size: 12px;
  }

  .artist {
    font-size: 10px;
    font-weight: 500;
    color: rgb(255 255 255 / 0.69);
  }

  .tag {
    flex: none;
    font-size: 10px;
    font-weight: 500;
    color: rgb(255 255 255 / 0.6);
  }

  .badge {
    flex: none;
    display: grid;
    place-items: center;
    color: var(--accent);
  }

  .badge.muted {
    color: rgb(255 255 255 / 0.75);
  }

  .lines {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 5px;
    filter: drop-shadow(0 1px 3px rgb(0 0 0 / 0.75));
  }

  .lines p {
    margin: 0;
    overflow: hidden;
  }

  .previous,
  .next {
    font-weight: 500;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .previous {
    color: rgb(255 255 255 / 0.5);
  }

  .next {
    color: rgb(255 255 255 / 0.7);
  }

  .current {
    font-weight: 650;
    line-height: 1.18;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow-wrap: anywhere;
  }

  .word {
    background: linear-gradient(90deg, #fff var(--fill), rgb(255 255 255 / 0.42) var(--fill));
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    white-space: pre-wrap;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 9px;
  }

  .status p {
    font-size: 14px;
    font-weight: 500;
    line-height: 1.3;
    color: rgb(255 255 255 / 0.84);
  }

  .retry {
    flex: none;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    color: var(--accent);
  }

  .progress {
    flex: none;
    height: 2px;
    border-radius: 1px;
    background: rgb(255 255 255 / 0.16);
    overflow: hidden;
  }

  .progress div {
    height: 100%;
    background: var(--accent);
  }

  @media (prefers-reduced-motion: reduce) {
    .current {
      animation: none !important;
      transition: none !important;
    }
  }
</style>
