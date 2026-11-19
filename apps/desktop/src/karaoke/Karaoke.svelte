<script lang="ts">
  // Karaoke: the lyrics fill a big screen, white on black, for singing along.
  // The current line lights up word by word in the middle and the next ones
  // wait below it; breaks count down with three dots, and a long intro shows
  // the song's card. Moving the pointer brings up the controls.
  import { onMount } from "svelte";
  import { prefersReducedMotion } from "svelte/motion";
  import { fade } from "svelte/transition";
  import { api } from "../lib/api";
  import { PlaybackClock } from "../lib/clock";
  import { formatOffset, translator } from "../lib/i18n";
  import Icon from "../lib/Icon.svelte";
  import { karaokeFontSize, karaokeLines, karaokeScene, MAX_WIDTH, SHRINK, upcomingCount } from "../lib/karaoke";
  import { connect } from "../lib/live";
  import { clamp } from "../lib/lyrics";
  import { lyricMeasure } from "../lib/measure";
  import { songExit } from "../lib/motion";
  import { lyricSeekTarget, SeekHold } from "../lib/player";
  import { splitLongLines } from "../lib/segment";
  import { statusText } from "../lib/status";
  import type { AppInfo, KaraokeDisplay, LyricsView, Playback, Settings } from "../lib/types";
  import KaraokeBar from "./KaraokeBar.svelte";
  import KaraokeCard from "./KaraokeCard.svelte";
  import KaraokeLines from "./KaraokeLines.svelte";

  /** Seconds the left and right keys move the song. */
  const SEEK_STEP = 5;
  /** Seconds the up and down keys move the lyrics. */
  const TIMING_STEP = 0.25;
  /** The controls hide once the pointer rests this long, a little longer right after opening. */
  const IDLE_MS = 2500;
  const FIRST_IDLE_MS = 4000;
  /** How long a note about a change stays. */
  const NOTICE_MS = 1600;

  let settings = $state<Settings | null>(null);
  let playback = $state<Playback | null>(null);
  let lyricsView = $state<LyricsView | null>(null);
  let app = $state<AppInfo | null>(null);
  let position = $state(0);
  let displays = $state<KaraokeDisplay[]>([]);

  const language = $derived(app?.language ?? "en");
  const t = $derived(translator(language));

  $effect(() => {
    document.title = t("karaoke.title");
  });

  const track = $derived(playback?.state === "track" ? playback.track : null);
  const lyrics = $derived(
    lyricsView?.status === "ready" && lyricsView.lyrics && lyricsView.trackKey === track?.key ? lyricsView.lyrics : null,
  );
  const songOffset = $derived(lyricsView?.songOffset ?? 0);
  const offset = $derived((settings?.globalOffset ?? 0) + songOffset);
  const effective = $derived(position + offset);

  // The type fits the window, and lines too long for it are split in two, once.
  let width = $state(0);
  let height = $state(0);
  const fontSize = $derived(karaokeFontSize(width, height, settings?.karaokeScale ?? 1));
  const measure = $derived(lyricMeasure(fontSize));
  const lines = $derived(
    lyrics && width > 0
      ? karaokeLines(
          splitLongLines(lyrics.lines, track?.duration ?? 0, (width * MAX_WIDTH) / SHRINK, measure),
          lyrics.sync,
          track?.duration ?? null,
        )
      : [],
  );
  const upcoming = $derived(upcomingCount(height, fontSize));
  const showLyrics = $derived(lines.length > 0 && playback?.position !== null);
  // The song starts at the corrections in the lyrics' time, which matters for the intro.
  const scene = $derived(karaokeScene(lines, effective, offset, lyrics?.sync));
  const status = $derived(statusText(playback, lyricsView, settings, t));

  const capabilities = $derived(playback?.capabilities ?? null);
  const canSeek = $derived(track !== null && !!capabilities?.seek && playback?.position !== null);
  // The estimated times of plain lyrics are too rough to jump to.
  const chorusMarks = $derived(
    canSeek && lyrics !== null && lyrics.sync !== "unsynced"
      ? (lyricsView?.chorus ?? []).map((start) => lyricSeekTarget(start, offset))
      : [],
  );

  // The position moves on between readings, as in the overlay.
  const clock = new PlaybackClock();
  const seekHold = new SeekHold();
  let frame = 0;

  function applyPlayback(next: Playback) {
    playback = next;
    const now = performance.now();
    const reading = {
      position: next.position,
      playing: next.playing,
      rate: next.rate,
      duration: next.track?.duration ?? null,
      trackKey: next.track?.key ?? null,
    };
    // Right after a seek, players may still report where they were.
    if (seekHold.isStale(reading, now)) reading.position = clock.positionAt(now);
    clock.update(reading, now);
    position = clock.positionAt(now);
    startTicking();
  }

  /** Sends the player to `target` seconds and shows the new position at once. */
  function seekTo(target: number) {
    if (!playback || !track || !canSeek) return;
    const duration = track.duration && track.duration > 0 ? track.duration : Infinity;
    const destination = clamp(target, 0, duration);
    void api.playerCommand({ kind: "seekTo", position: destination });
    const now = performance.now();
    seekHold.start(destination, track.key, playback.rate, now);
    clock.seek(destination, now);
    position = clock.positionAt(now);
    startTicking();
  }

  function tick() {
    position = clock.positionAt(performance.now());
    frame = clock.isPlaying ? requestAnimationFrame(tick) : 0;
  }

  function startTicking() {
    if (frame === 0 && clock.isPlaying) frame = requestAnimationFrame(tick);
  }

  // The controls stay hidden while singing; moving the pointer brings them up.
  let controlsShown = $state(true);
  let scrubbing = $state(false);
  let holding = false;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  function wake(wait = IDLE_MS) {
    controlsShown = true;
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      if (!holding && !scrubbing) controlsShown = false;
    }, wait);
  }

  function rest() {
    holding = false;
    if (scrubbing) return;
    clearTimeout(hideTimer);
    controlsShown = false;
  }

  function hold(held: boolean) {
    holding = held;
    if (!held) wake();
  }

  function scrub(held: boolean) {
    scrubbing = held;
    if (!held) wake();
  }

  // The displays to choose from, fresh whenever the controls come up.
  $effect(() => {
    if (!controlsShown) return;
    void api.karaokeDisplays().then(
      (list) => (displays = list),
      () => (displays = []),
    );
  });

  function chooseDisplay(id: string) {
    displays = displays.map((display) => ({ ...display, current: display.id === id }));
    void api.openKaraoke(id);
  }

  let notice = $state<string | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  function say(text: string) {
    notice = text;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = null), NOTICE_MS);
  }

  async function adjustTiming(delta: number) {
    if (!track) return;
    const value = await api.adjustSongOffset(delta);
    if (lyricsView) lyricsView = { ...lyricsView, songOffset: value };
    say(t("karaoke.timing", { value: formatOffset(language, value) }));
  }

  function exit() {
    void api.closeKaraoke();
  }

  function keydown(event: KeyboardEvent) {
    if (event.defaultPrevented || event.altKey) return;
    const command = app?.platform === "macos" ? event.metaKey : event.ctrlKey;
    if (event.key === "Escape" || (command && event.key.toLowerCase() === "w")) exit();
    else if (event.metaKey || event.ctrlKey) return;
    else if (event.key === " ") {
      // A focused button answers the space bar itself.
      if ((event.target as Element | null)?.closest?.("button")) return;
      if (capabilities?.playPause) void api.playerCommand({ kind: "playPause" });
    } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      if (!canSeek) return;
      seekTo(position + (event.key === "ArrowLeft" ? -SEEK_STEP : SEEK_STEP));
      wake();
    } else if (event.key === "ArrowUp") void adjustTiming(TIMING_STEP);
    else if (event.key === "ArrowDown") void adjustTiming(-TIMING_STEP);
    else return;
    event.preventDefault();
  }

  onMount(() => {
    wake(FIRST_IDLE_MS);
    const stop = connect(
      {
        playback: applyPlayback,
        lyrics: (next) => (lyricsView = next),
        settings: (next) => (settings = next),
        app: (next) => (app = next),
      },
      (snapshot) => {
        settings = snapshot.settings;
        lyricsView = snapshot.lyrics;
        app = snapshot.app;
        applyPlayback(snapshot.playback);
      },
    );
    return () => {
      stop();
      cancelAnimationFrame(frame);
      clearTimeout(hideTimer);
      clearTimeout(noticeTimer);
    };
  });

  const fadeMotion = $derived({ duration: prefersReducedMotion.current ? 0 : 200 });
</script>

<svelte:window onkeydown={keydown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<main
  class="karaoke"
  class:resting={!controlsShown}
  onpointermove={() => wake()}
  onpointerdown={() => wake()}
  onpointerleave={rest}
>
  <div class="stage" bind:clientWidth={width} bind:clientHeight={height}>
    {#key track?.key ?? ""}
      <div class="content" out:songExit={{ reduced: prefersReducedMotion.current }}>
        {#if showLyrics && lyrics && !scene.card}
          <div class="layer" out:songExit={{ reduced: prefersReducedMotion.current }}>
            <KaraokeLines
              {lines}
              {scene}
              position={effective}
              sync={lyrics.sync}
              {fontSize}
              {width}
              {height}
              {upcoming}
              playing={playback?.playing ?? false}
              {measure}
              {t}
            />
          </div>
        {:else}
          <div class="layer" out:songExit={{ reduced: prefersReducedMotion.current }}>
            <KaraokeCard {track} status={showLyrics ? "" : status} {fontSize} {t} />
          </div>
        {/if}
      </div>
    {/key}
  </div>

  <div class="notes">
    {#if track && playback && !playback.playing}
      <span class="note" transition:fade={fadeMotion}><Icon name="pause" size={14} />{t("overlay.paused")}</span>
    {/if}
    {#if lyrics?.sync === "unsynced" && showLyrics}
      <span class="note" transition:fade={fadeMotion}>{t("overlay.notSynced")}</span>
    {/if}
    {#if notice}
      <span class="note" transition:fade={fadeMotion}>{notice}</span>
    {/if}
  </div>

  {#if controlsShown}
    <KaraokeBar
      {track}
      {capabilities}
      playing={playback?.playing ?? false}
      {position}
      duration={track?.duration ?? 0}
      {canSeek}
      marks={chorusMarks}
      {songOffset}
      {language}
      {displays}
      {t}
      onseek={seekTo}
      onscrub={scrub}
      ontiming={(delta) => void adjustTiming(delta)}
      ondisplay={chooseDisplay}
      onexit={exit}
      onhold={hold}
    />
  {/if}
</main>

<style>
  /* Black, white type, no colour. */
  .karaoke {
    position: fixed;
    inset: 0;
    overflow: hidden;
    background: #000;
    color: #fff;
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
  }

  /* The pointer hides with the controls. */
  .resting,
  .resting :global(*) {
    cursor: none;
  }

  /* Old and new content share the stage while one leaves and the other arrives. */
  .stage,
  .content,
  .layer {
    display: grid;
    grid-template: minmax(0, 1fr) / minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
  }

  .stage {
    position: absolute;
    inset: 0;
  }

  .content,
  .layer {
    grid-area: 1 / 1;
  }

  .notes {
    position: absolute;
    top: 28px;
    right: 0;
    left: 0;
    display: flex;
    justify-content: center;
    gap: 10px;
    pointer-events: none;
  }

  .note {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 7px 14px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.1);
    font-size: 15px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.8);
  }

  /* Content appearing fades in, rises and sharpens, as in the overlay. */
  .karaoke :global(.reveal) {
    animation: reveal 0.55s cubic-bezier(0.23, 1, 0.32, 1) both;
  }

  @keyframes reveal {
    from {
      opacity: 0;
      transform: translateY(12px);
      filter: blur(8px);
    }
  }

  @keyframes reveal-fade {
    from {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .karaoke :global(.reveal) {
      animation-name: reveal-fade;
    }
  }
</style>
