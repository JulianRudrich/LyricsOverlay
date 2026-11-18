<script lang="ts" module>
  import type { HotkeyError } from "../lib/types";

  /** What the row's hint explains. */
  export type HotkeyStatus =
    | { kind: "idle" }
    | { kind: "recording"; problem: "unsupported" | "needsModifier" | null }
    | { kind: "failed"; hotkey: string; error: HotkeyError };
</script>

<script lang="ts">
  // The shortcut on a key cap. A click records the next key combination;
  // Escape or a click elsewhere cancels, and Reset brings the default back.
  import { hotkeyLabel, recordKey, releaseKey } from "../lib/hotkey";
  import type { Translate } from "../lib/i18n";
  import Button from "../lib/ui/Button.svelte";
  import KeyCap from "../lib/ui/KeyCap.svelte";

  let {
    hotkey,
    defaultHotkey,
    platform,
    t,
    onchange,
    status = $bindable({ kind: "idle" }),
  }: {
    hotkey: string;
    defaultHotkey: string;
    platform: string;
    t: Translate;
    /** Resolves to why the new shortcut was not taken, or to null. */
    onchange: (hotkey: string) => Promise<HotkeyError | null>;
    status?: HotkeyStatus;
  } = $props();

  /** The modifiers held while recording, shown before the key comes. */
  let held = $state<string[]>([]);
  let saving = $state(false);

  const recording = $derived(status.kind === "recording");
  const label = $derived.by(() => {
    if (!recording) return hotkeyLabel(hotkey, platform);
    return held.length > 0 ? `${hotkeyLabel(held.join("+"), platform)} …` : t("settings.hotkeyPrompt");
  });

  function stop() {
    held = [];
    if (status.kind === "recording") status = { kind: "idle" };
  }

  async function save(next: string) {
    held = [];
    saving = true;
    status = { kind: "idle" };
    const error = await onchange(next);
    saving = false;
    status = error ? { kind: "failed", hotkey: next, error } : { kind: "idle" };
  }

  function click() {
    if (recording) stop();
    else if (!saving) status = { kind: "recording", problem: null };
  }

  function keydown(event: KeyboardEvent) {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat) return;
    const result = recordKey(event, platform);
    if (result.kind === "cancel") stop();
    else if (result.kind === "held") held = result.modifiers;
    else if (result.kind === "problem") {
      held = [];
      status = { kind: "recording", problem: result.problem };
    } else void save(result.hotkey);
  }

  function keyup(event: KeyboardEvent) {
    if (!recording) return;
    event.preventDefault();
    held = releaseKey(event);
  }
</script>

<span class="field">
  {#if hotkey !== defaultHotkey && !recording}
    <Button
      title={t("settings.hotkeyReset", { hotkey: hotkeyLabel(defaultHotkey, platform) })}
      disabled={saving}
      onclick={() => save(defaultHotkey)}
    >
      {t("settings.reset")}
    </Button>
  {/if}
  <button
    type="button"
    class="cap"
    class:recording
    aria-pressed={recording}
    aria-label="{t('settings.hotkey')}: {label}"
    title={t("settings.hotkeyChange")}
    disabled={saving}
    onclick={click}
    onkeydown={keydown}
    onkeyup={keyup}
    onblur={stop}
  >
    <KeyCap {label} />
  </button>
</span>

<style>
  .field {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .cap {
    display: inline-flex;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: default;
    transition:
      box-shadow var(--duration-press) var(--ease-out),
      transform var(--duration-press) var(--ease-out);
  }

  .cap:hover:not(:disabled) {
    box-shadow: 0 0 0 1px rgb(255 255 255 / 0.18);
  }

  .cap:active:not(:disabled) {
    transform: scale(var(--press-scale-subtle));
  }

  /* Listening: a white ring, and the key cap lights up. */
  .cap.recording {
    box-shadow: 0 0 0 1.5px rgb(255 255 255 / 0.85);
  }

  .cap.recording :global(.keycap) {
    background: rgb(255 255 255 / 0.16);
  }

  .cap:focus-visible {
    outline: 2px solid rgb(255 255 255 / 0.7);
    outline-offset: 2px;
  }

  .cap:disabled {
    opacity: 0.6;
  }
</style>
