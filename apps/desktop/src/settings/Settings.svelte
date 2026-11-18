<script lang="ts">
  import { onMount } from "svelte";
  import { api, subscribe } from "../lib/api";
  import { translator } from "../lib/i18n";
  import { paneIn } from "../lib/motion";
  import type { AppInfo, HotkeyError, LyricsView, Playback, Settings } from "../lib/types";
  import Sidebar from "../lib/ui/Sidebar.svelte";
  import { panes, type PaneContext } from "./panes";

  let settings = $state<Settings | null>(null);
  let playback = $state<Playback | null>(null);
  let lyricsView = $state<LyricsView | null>(null);
  let app = $state<AppInfo | null>(null);
  let selected = $state(panes[0]?.id ?? "");
  /** The first pane appears with the window; later ones fade in. */
  let switched = $state(false);
  let content = $state<HTMLElement>();

  const language = $derived(app?.language ?? "en");
  const t = $derived(translator(language));
  const pane = $derived(panes.find((entry) => entry.id === selected) ?? panes[0]);

  $effect(() => {
    document.title = t("settings.title");
  });

  // The panes render only once the snapshot arrived, so settings and app are set.
  const ctx: PaneContext = {
    get settings() {
      return settings as Settings;
    },
    get app() {
      return app as AppInfo;
    },
    get playback() {
      return playback;
    },
    get lyrics() {
      return lyricsView;
    },
    get language() {
      return language;
    },
    get t() {
      return t;
    },
    async update(patch) {
      if (settings) settings = { ...settings, ...patch };
      settings = await api.updateSettings(patch);
    },
    async setHotkey(hotkey) {
      try {
        settings = await api.setHotkey(hotkey);
        return null;
      } catch (error) {
        const known: HotkeyError[] = ["invalid", "unsupported", "needsModifier", "taken"];
        return known.find((name) => name === error) ?? "taken";
      }
    },
    async setLaunchAtLogin(enabled) {
      const actual = await api.setLaunchAtLogin(enabled);
      if (app) app = { ...app, launchAtLogin: actual };
    },
    async adjustSongOffset(delta) {
      const value = await api.adjustSongOffset(delta);
      if (lyricsView) lyricsView = { ...lyricsView, songOffset: value };
    },
    async resetSongOffset() {
      const value = await api.setSongOffset(0);
      if (lyricsView) lyricsView = { ...lyricsView, songOffset: value };
    },
  };

  function select(id: string) {
    if (id === selected) return;
    selected = id;
    switched = true;
    content?.scrollTo({ top: 0 });
  }

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

{#if settings && app && pane}
  <!-- Like the macOS app, the window moves when dragged anywhere but on a control. -->
  <main class="window" data-tauri-drag-region="deep">
    <Sidebar
      title={t("app.name")}
      caption="v{app.version}"
      items={panes.map((entry) => ({ id: entry.id, icon: entry.icon, label: t(entry.title) }))}
      {selected}
      label={t("settings.sections")}
      panel="settings-pane"
      onselect={select}
    />
    <div class="content" id="settings-pane" role="tabpanel" aria-labelledby="tab-{pane.id}" bind:this={content}>
      {#key pane.id}
        <div use:paneIn={switched}>
          <header>
            <h1>{t(pane.title)}</h1>
            <p>{t(pane.subtitle)}</p>
          </header>
          <div class="cards">
            <pane.component {ctx} />
          </div>
        </div>
      {/key}
    </div>
  </main>
{/if}

<style>
  /* Dark glass: a neutral black tint over the window's blur, or over the solid
     background where there is none. */
  .window {
    --top-inset: 24px;
    position: fixed;
    inset: 0;
    display: flex;
    overflow: hidden;
    color: #fff;
    background: linear-gradient(135deg, rgb(48 48 48 / 0.72), rgb(22 22 22 / 0.72));
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
    animation: window-in var(--duration-pane) var(--ease-out) both;
  }

  /* Under a transparent title bar (macOS) the glass is the whole window: round
     corners, light along the top edge and room for the window buttons. */
  :global(html[data-titlebar="overlay"]) .window {
    --top-inset: 44px;
    border-radius: 20px;
  }

  :global(html[data-titlebar="overlay"]) .window::after {
    content: "";
    position: absolute;
    inset: 0;
    padding: 1px;
    border-radius: inherit;
    background: linear-gradient(rgb(255 255 255 / 0.28), rgb(255 255 255 / 0.06));
    /* Keeps only the 1 px ring; the prefixed pair comes last for WebKit. */
    mask:
      linear-gradient(#000 0 0) content-box,
      linear-gradient(#000 0 0);
    mask-composite: exclude;
    -webkit-mask:
      linear-gradient(#000 0 0) content-box,
      linear-gradient(#000 0 0);
    -webkit-mask-composite: xor;
    pointer-events: none;
  }

  @keyframes window-in {
    from {
      opacity: 0;
    }
  }

  .content {
    flex: 1;
    min-width: 0;
    box-sizing: border-box;
    padding: var(--top-inset) 20px 24px 28px;
    border-left: 1px solid rgb(255 255 255 / 0.08);
    overflow-y: scroll;
  }

  /* A slim scroll bar that shows while the pointer is over the pane. */
  .content::-webkit-scrollbar {
    width: 8px;
  }

  .content::-webkit-scrollbar-thumb {
    border: 2px solid transparent;
    border-radius: 4px;
    background-clip: padding-box;
  }

  .content:hover::-webkit-scrollbar-thumb {
    background-color: rgb(255 255 255 / 0.2);
  }

  header {
    margin-bottom: 20px;
  }

  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
    line-height: 1.2;
  }

  header p {
    margin: 4px 0 0;
    font-size: 13px;
    color: rgb(255 255 255 / 0.6);
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
</style>
