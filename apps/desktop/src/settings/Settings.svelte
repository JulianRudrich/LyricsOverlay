<script lang="ts">
  import { onMount } from "svelte";
  import { api, subscribe } from "../lib/api";
  import { hotkeyLabel } from "../lib/hotkey";
  import { formatNumber, formatOffset, translator, type MessageKey } from "../lib/i18n";
  import type { AppInfo, Language, Layout, LyricsView, Playback, Settings, SourcePreference } from "../lib/types";

  let settings = $state<Settings | null>(null);
  let playback = $state<Playback | null>(null);
  let lyricsView = $state<LyricsView | null>(null);
  let app = $state<AppInfo | null>(null);
  let tab = $state<"appearance" | "song">("appearance");

  const language = $derived(app?.language ?? "en");
  const t = $derived(translator(language));
  const track = $derived(playback?.state === "track" ? playback.track : null);
  const lyricsOfTrack = $derived(lyricsView && lyricsView.trackKey === track?.key ? lyricsView : null);

  const layouts: Layout[] = ["caption", "focus", "context"];
  const sources: SourcePreference[] = ["automatic", "spotify", "appleMusic", "amazonMusic"];
  const languages: Language[] = ["system", "de", "en"];

  $effect(() => {
    document.title = t("settings.title");
  });

  async function update(patch: Partial<Settings>) {
    if (settings) settings = { ...settings, ...patch };
    settings = await api.updateSettings(patch);
  }

  async function setLaunchAtLogin(enabled: boolean) {
    const actual = await api.setLaunchAtLogin(enabled);
    if (app) app = { ...app, launchAtLogin: actual };
  }

  async function adjust(delta: number) {
    const value = await api.adjustSongOffset(delta);
    if (lyricsView) lyricsView = { ...lyricsView, songOffset: value };
  }

  async function resetOffset() {
    const value = await api.setSongOffset(0);
    if (lyricsView) lyricsView = { ...lyricsView, songOffset: value };
  }

  const lyricsSummary = $derived.by(() => {
    const view = lyricsOfTrack;
    if (!view?.lyrics || view.status !== "ready") return "";
    const origin =
      view.origin === "local" ? t("settings.lyricsLocal") : view.origin === "demo" ? t("settings.lyricsDemo") : t("settings.lyricsOnline");
    return `${origin} · ${t(`sync.${view.lyrics.sync}` as MessageKey)}`;
  });

  onMount(() => {
    let stop: (() => void) | undefined;
    let alive = true;
    void (async () => {
      const unlisten = await subscribe({
        playback: (next) => (playback = next),
        lyrics: (next) => (lyricsView = next),
        settings: (next) => (settings = next),
        app: (next) => (app = next),
      });
      if (!alive) return unlisten();
      stop = unlisten;
      const snapshot = await api.snapshot();
      settings = snapshot.settings;
      playback = snapshot.playback;
      lyricsView = snapshot.lyrics;
      app = snapshot.app;
    })();
    return () => {
      alive = false;
      stop?.();
    };
  });
</script>

{#if settings && app}
  <main class="settings-window">
    <div class="tabs" role="tablist">
      <button role="tab" aria-selected={tab === "appearance"} class:selected={tab === "appearance"} onclick={() => (tab = "appearance")}>
        {t("settings.tabAppearance")}
      </button>
      <button role="tab" aria-selected={tab === "song"} class:selected={tab === "song"} onclick={() => (tab = "song")}>
        {t("settings.tabSong")}
      </button>
    </div>

    {#if tab === "appearance"}
      <section class="panel" role="tabpanel">
        <div class="field">
          <span class="label">{t("settings.layout")}</span>
          <div class="segmented" role="radiogroup" aria-label={t("settings.layout")}>
            {#each layouts as layout (layout)}
              <button role="radio" aria-checked={settings.layout === layout} class:selected={settings.layout === layout} onclick={() => update({ layout })}>
                {t(`layout.${layout}` as MessageKey)}
              </button>
            {/each}
          </div>
        </div>

        {@render slider(t("settings.fontSize"), settings.fontSize, 14, 44, 1, (fontSize) => update({ fontSize }), t("unit.points", { value: settings.fontSize }))}
        {@render slider(t("settings.overlayOpacity"), Math.round(settings.overlayOpacity * 100), 15, 100, 1, (value) => update({ overlayOpacity: value / 100 }), t("unit.percent", { value: Math.round(settings.overlayOpacity * 100) }))}
        {@render slider(t("settings.backgroundOpacity"), Math.round(settings.backgroundOpacity * 100), 0, 100, 1, (value) => update({ backgroundOpacity: value / 100 }), t("unit.percent", { value: Math.round(settings.backgroundOpacity * 100) }))}
        {@render slider(t("settings.width"), settings.width, 260, 800, 10, (width) => update({ width }), t("unit.points", { value: settings.width }))}
        {#if settings.layout === "context"}
          {@render slider(t("settings.height"), settings.height, 160, 500, 10, (height) => update({ height }), t("unit.points", { value: settings.height }))}
        {/if}

        <p class="hint" class:warning={app.hotkeyError}>
          {app.hotkeyError ? t("settings.hotkeyError", { hotkey: hotkeyLabel(settings.hotkey, app.platform) }) : t("settings.hotkeyHint", { hotkey: hotkeyLabel(settings.hotkey, app.platform) })}
        </p>

        <hr />

        <label class="toggle">
          <input type="checkbox" checked={app.launchAtLogin} onchange={(event) => setLaunchAtLogin(event.currentTarget.checked)} />
          <span>{t("settings.launchAtLogin")}</span>
        </label>

        <label class="field inline">
          <span class="label">{t("settings.language")}</span>
          <select value={settings.language} onchange={(event) => update({ language: event.currentTarget.value as Language })}>
            {#each languages as option (option)}
              <option value={option}>{t(`language.${option}` as MessageKey)}</option>
            {/each}
          </select>
        </label>

        <p class="footer">{t("settings.version", { version: app.version })}</p>
      </section>
    {:else}
      <section class="panel" role="tabpanel">
        <label class="field inline">
          <span class="label">{t("settings.source")}</span>
          <select value={settings.source} disabled={app.demo} onchange={(event) => update({ source: event.currentTarget.value as SourcePreference })}>
            {#each sources as source (source)}
              <option value={source}>{t(`source.${source}` as MessageKey)}</option>
            {/each}
          </select>
        </label>
        <label class="toggle">
          <input type="checkbox" checked={settings.includeOtherPlayers} disabled={app.demo || settings.source !== "automatic"} onchange={(event) => update({ includeOtherPlayers: event.currentTarget.checked })} />
          <span>{t("settings.includeOther")}</span>
        </label>

        <div class="now-playing">
          {#if app.demo}
            <strong>{t("settings.demo")}</strong>
          {/if}
          {#if track}
            <strong class="ellipsis">{track.title}</strong>
            <span class="secondary ellipsis">{[track.artist, playback?.player?.name].filter(Boolean).join(" · ")}</span>

            <div class="offset-row">
              <span>{t("settings.songOffset")}</span>
              <span class="spacer"></span>
              <button onclick={() => adjust(-0.25)} aria-label={t("settings.later")} disabled={(lyricsOfTrack?.songOffset ?? 0) <= -10}>−{formatNumber(language, 0.25, 2)}</button>
              <output class="value">{formatOffset(language, lyricsOfTrack?.songOffset ?? 0)}</output>
              <button onclick={() => adjust(0.25)} aria-label={t("settings.earlier")} disabled={(lyricsOfTrack?.songOffset ?? 0) >= 10}>+{formatNumber(language, 0.25, 2)}</button>
              <button onclick={resetOffset} disabled={(lyricsOfTrack?.songOffset ?? 0) === 0}>{t("settings.reset")}</button>
            </div>

            <div class="offset-row">
              <span class="secondary">{lyricsSummary}</span>
              <span class="spacer"></span>
              {#if !app.demo}
                <button onclick={() => api.reloadLyrics()}>{t("settings.searchAgain")}</button>
              {/if}
            </div>
          {:else if !app.demo}
            <span class="secondary">{t("settings.noTrack")}</span>
          {/if}
        </div>

        <hr />

        {@render slider(t("settings.globalOffset"), settings.globalOffset, -5, 5, 0.1, (globalOffset) => update({ globalOffset }), t("unit.seconds", { value: formatNumber(language, settings.globalOffset, 1) }))}
        <p class="hint">{t("settings.offsetHint")}</p>

        <label class="toggle">
          <input type="checkbox" checked={settings.lookupOnline} disabled={app.demo} onchange={(event) => update({ lookupOnline: event.currentTarget.checked })} />
          <span>{t("settings.lookupOnline")}</span>
        </label>
        <p class="hint">{t("settings.privacy")}</p>
      </section>
    {/if}
  </main>
{/if}

{#snippet slider(label: string, value: number, min: number, max: number, step: number, change: (value: number) => void, display: string)}
  <label class="field">
    <span class="row"><span class="label">{label}</span><span class="value">{display}</span></span>
    <input type="range" {min} {max} {step} {value} oninput={(event) => change(Number(event.currentTarget.value))} />
  </label>
{/snippet}



<style>
  .settings-window {
    padding: 14px 22px 18px;
  }

  .tabs {
    display: flex;
    justify-content: center;
    gap: 4px;
    margin-bottom: 16px;
  }

  .tabs button,
  .segmented button {
    padding: 5px 14px;
    border-radius: 7px;
    border: 1px solid transparent;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .tabs button.selected,
  .segmented button.selected {
    background: var(--control-selected);
    border-color: var(--separator);
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field.inline {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
  }

  .row {
    display: flex;
    justify-content: space-between;
  }

  .value {
    color: var(--secondary);
    font-variant-numeric: tabular-nums;
  }

  .segmented {
    display: flex;
    padding: 2px;
    gap: 2px;
    border-radius: 9px;
    background: var(--control);
  }

  .segmented button {
    flex: 1;
  }

  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .toggle input {
    accent-color: var(--accent);
  }

  .hint {
    margin: -6px 0 0;
    font-size: 12px;
    color: var(--secondary);
  }

  .hint.warning {
    color: var(--warning);
  }

  hr {
    width: 100%;
    margin: 2px 0;
    border: 0;
    border-top: 1px solid var(--separator);
  }

  .now-playing {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 44px;
  }

  .ellipsis {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .secondary {
    color: var(--secondary);
  }

  .offset-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .spacer {
    flex: 1;
  }

  .offset-row button,
  select {
    padding: 4px 10px;
    border-radius: 6px;
    border: 1px solid var(--separator);
    background: var(--control);
    color: inherit;
    font: inherit;
  }

  .offset-row button:disabled {
    opacity: 0.45;
  }

  output.value {
    min-width: 64px;
    text-align: center;
  }

  .footer {
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--secondary);
    text-align: center;
  }
</style>
