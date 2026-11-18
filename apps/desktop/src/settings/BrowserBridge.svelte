<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { translator } from "../lib/i18n";
  import { connect } from "../lib/live";
  import SettingRow from "../lib/SettingRow.svelte";
  import type { AppInfo, BridgeView } from "../lib/types";

  // Self-contained: shows the code that pairs the Singline browser extension and the
  // browsers that paired, so it fits into any pane. While it is shown, pairing stays open.
  const PUZZLE = "M4 4h6a2 2 0 1 1 4 0h6v6a2 2 0 1 1 0 4v6H4z";
  const WINDOW = "M4 5h16v14H4zM4 9h16M7 7h.01M10 7h.01";

  let app = $state<AppInfo | null>(null);
  let bridge = $state<BridgeView | null>(null);
  let busy = $state(false);
  const t = $derived(translator(app?.language ?? "en"));
  const code = $derived(bridge?.pairingCode ? `${bridge.pairingCode.slice(0, 3)} ${bridge.pairingCode.slice(3)}` : "");

  async function refresh() {
    try {
      bridge = await api.browserBridge();
    } catch {
      bridge = null;
    }
  }

  async function forget(id: string) {
    busy = true;
    try {
      bridge = await api.forgetBrowser(id);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    const stop = connect({ app: (next) => (app = next) }, (snapshot) => (app = snapshot.app));
    void refresh();
    const timer = setInterval(refresh, 2000);
    return () => {
      stop();
      clearInterval(timer);
    };
  });
</script>

{#if app && bridge?.available}
  <div class="bridge">
    <SettingRow icon={PUZZLE} title={t("bridge.title")} hint={bridge.port === null ? t("bridge.unavailable") : t("bridge.hint")}>
      {#if code}
        <output class="code" aria-label={t("bridge.codeLabel")}>{code}</output>
      {/if}
    </SettingRow>
    {#each bridge.browsers as browser (browser.id)}
      <SettingRow icon={WINDOW} title={browser.name} hint={browser.connected ? t("bridge.connected") : t("bridge.idle")}>
        <button disabled={busy} onclick={() => forget(browser.id)}>{t("bridge.remove")}</button>
      </SettingRow>
    {/each}
  </div>
{/if}

<style>
  .bridge {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .code {
    font-size: 17px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.08em;
    white-space: nowrap;
    user-select: all;
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
