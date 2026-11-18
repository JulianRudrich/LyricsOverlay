<script lang="ts">
  import { onMount, tick } from "svelte";
  import { api } from "../lib/api";
  import { translator } from "../lib/i18n";
  import { connect } from "../lib/live";
  import SettingRow from "../lib/SettingRow.svelte";
  import Switch from "../lib/Switch.svelte";
  import type { AppInfo, Settings } from "../lib/types";
  import { statsApi } from "../stats/api";

  // Self-contained: reads and changes the settings itself, so it fits into any
  // window or pane. `hasHistory` false turns "Clear history" off.
  let { hasHistory = true }: { hasHistory?: boolean } = $props();

  const CLOCK_BACK = "M3.5 12a8.5 8.5 0 1 0 2.6-6.1M3 4v4.5h4.5M12 7.5V12l3 2";
  /** An unanswered question goes away by itself. */
  const ASK_FOR = 8_000;

  let settings = $state<Settings | null>(null);
  let app = $state<AppInfo | null>(null);
  let asking = $state(false);
  let clearing = $state(false);
  let cancel = $state<HTMLButtonElement>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const t = $derived(translator(app?.language ?? "en"));

  async function keep(keepHistory: boolean) {
    if (settings) settings = { ...settings, keepHistory };
    settings = await api.updateSettings({ keepHistory });
  }

  async function ask() {
    asking = true;
    clearTimeout(timer);
    timer = setTimeout(() => (asking = false), ASK_FOR);
    await tick();
    // Enter on the question must not delete anything.
    cancel?.focus();
  }

  function stopAsking() {
    clearTimeout(timer);
    asking = false;
  }

  function escape(event: KeyboardEvent) {
    if (event.key === "Escape") stopAsking();
  }

  async function clear() {
    clearing = true;
    try {
      await statsApi.clear();
    } finally {
      clearing = false;
      stopAsking();
    }
  }

  onMount(() => {
    const stop = connect(
      { settings: (next) => (settings = next), app: (next) => (app = next) },
      (snapshot) => {
        settings = snapshot.settings;
        app = snapshot.app;
      },
    );
    return () => {
      stop();
      clearTimeout(timer);
    };
  });
</script>

{#if settings && app}
  <SettingRow icon={CLOCK_BACK} title={t("history.keep")} hint={t("history.keepHint")}>
    {#if asking}
      <span class="question">
        <span>{t("history.clearConfirm")}</span>
        <button type="button" class="button strong" onclick={clear} onkeydown={escape} disabled={clearing}>{t("history.clearYes")}</button>
        <button type="button" class="button" bind:this={cancel} onclick={stopAsking} onkeydown={escape}>{t("history.clearNo")}</button>
      </span>
    {:else}
      <button type="button" class="button" onclick={ask} disabled={!hasHistory}>{t("history.clear")}</button>
    {/if}
    <Switch checked={settings.keepHistory} label={t("history.keep")} onchange={keep} />
  </SettingRow>
{/if}

<style>
  .question {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }

  .button {
    padding: 4px 11px;
    border-radius: 7px;
    border: 1px solid rgb(128 128 128 / 0.35);
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: transform 140ms cubic-bezier(0.23, 1, 0.32, 1);
  }

  .button:hover:not(:disabled) {
    border-color: currentColor;
  }

  .button:active:not(:disabled) {
    transform: scale(0.97);
  }

  .button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .button.strong {
    border-color: var(--text, #fff);
    background: var(--text, #fff);
    color: var(--window, #111113);
    font-weight: 600;
  }

  .button:focus-visible {
    outline: 2px solid var(--text, #fff);
    outline-offset: 2px;
  }

  @media (prefers-reduced-motion: reduce) {
    .button {
      transition: none;
    }
  }
</style>
