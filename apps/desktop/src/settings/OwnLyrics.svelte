<script lang="ts">
  import { api } from "../lib/api";
  import { ownLyricsErrorKey, type OwnLyricsAction } from "../lib/editor";
  import type { MessageKey, Translate } from "../lib/i18n";
  import Icon from "../lib/Icon.svelte";
  import type { LyricsView, Track } from "../lib/types";

  // Your own lyrics for the song that plays: open the editor, import a file, remove them.
  let { track, lyrics, t }: { track: Track; lyrics: LyricsView | null; t: Translate } = $props();

  let failure = $state<{ trackKey: string; message: MessageKey } | null>(null);
  let busy = $state(false);

  const own = $derived(lyrics?.status === "ready" && lyrics.origin === "local");
  // A message belongs to the song it was about and disappears with it.
  const message = $derived(failure?.trackKey === track.key ? failure.message : null);

  async function run(action: OwnLyricsAction, task: (trackKey: string) => Promise<unknown>) {
    const trackKey = track.key;
    failure = null;
    busy = true;
    try {
      await task(trackKey);
    } catch (error) {
      failure = { trackKey, message: ownLyricsErrorKey(error, action) };
    } finally {
      busy = false;
    }
  }
</script>

<section class="card" aria-label={t("ownLyrics.title")}>
  <div class="row">
    <span class="icon"><Icon name="document" size={17} /></span>
    <div class="text">
      <span class="title">{t("ownLyrics.title")}</span>
      <span class="hint">{own ? t("ownLyrics.activeHint") : t("ownLyrics.hint")}</span>
    </div>
  </div>
  <div class="buttons">
    <button disabled={busy} onclick={() => run("edit", () => api.openEditor())}>{t("ownLyrics.edit")}</button>
    <button disabled={busy} onclick={() => run("import", (trackKey) => api.importLyricsFile(trackKey, track.title))}>
      {t("ownLyrics.import")}
    </button>
    {#if own}
      <button disabled={busy} onclick={() => run("remove", (trackKey) => api.removeLocalLyrics(trackKey))}>
        {t("ownLyrics.remove")}
      </button>
    {/if}
  </div>
  {#if message}
    <p class="message" role="alert">{t(message)}</p>
  {/if}
</section>

<style>
  /*
   * Linory's building blocks in black and white. Fills and borders are the text
   * color at low opacity: white on the dark panes, and still legible on a light window.
   */
  .card {
    position: relative;
    isolation: isolate;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    border-radius: 12px;
  }

  .card::before,
  .card::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    pointer-events: none;
  }

  .card::before {
    z-index: -1;
    background: currentColor;
    opacity: 0.05;
  }

  .card::after {
    border: 1px solid currentColor;
    opacity: 0.08;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .icon {
    display: flex;
    justify-content: center;
    width: 26px;
    flex: none;
    opacity: 0.85;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .title {
    font-size: 13px;
    font-weight: 500;
  }

  .hint {
    font-size: 11.5px;
    opacity: 0.55;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  button {
    position: relative;
    isolation: isolate;
    padding: 6px 12px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: transform 140ms cubic-bezier(0.23, 1, 0.32, 1);
  }

  button::before {
    content: "";
    position: absolute;
    inset: 0;
    z-index: -1;
    border-radius: inherit;
    background: currentColor;
    opacity: 0.1;
    transition: opacity 140ms cubic-bezier(0.23, 1, 0.32, 1);
  }

  button:hover:not(:disabled)::before {
    opacity: 0.16;
  }

  button:active:not(:disabled) {
    transform: scale(0.97);
  }

  button:focus-visible {
    outline: 2px solid currentColor;
    outline-offset: 2px;
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .message {
    margin: 0;
    font-size: 11.5px;
    color: var(--warning, #f5b25b);
  }

  @media (prefers-reduced-motion: reduce) {
    button,
    button::before {
      transition: none;
    }

    button:active:not(:disabled) {
      transform: none;
    }
  }
</style>
