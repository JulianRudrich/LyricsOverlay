<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import type { UpdateView } from "../lib/types";
  import Button from "../lib/ui/Button.svelte";
  import Card from "../lib/ui/Card.svelte";
  import Row from "../lib/ui/Row.svelte";
  import Toggle from "../lib/ui/Toggle.svelte";
  import type { PaneContext } from "./panes";

  // Whether Singline is up to date, the download of a new version, and the
  // restart that installs it.
  let { ctx }: { ctx: PaneContext } = $props();
  const t = $derived(ctx.t);
  let view = $state<UpdateView>({ kind: "idle" });

  const hint = $derived.by(() => {
    switch (view.kind) {
      case "checking":
        return t("update.checking");
      case "upToDate":
        return t("update.upToDate");
      case "downloading":
        return view.progress === null
          ? t("update.downloading", { version: view.version })
          : t("update.downloadingProgress", { version: view.version, percent: Math.round(view.progress * 100) });
      case "ready":
        return t("update.ready", { version: view.version });
      case "failed":
        return t("update.failed");
      case "unavailable":
        return t("update.unavailable");
      default:
        return ctx.settings.autoUpdate ? t("update.idle") : t("update.off");
    }
  });
  const busy = $derived(view.kind === "checking" || view.kind === "downloading");

  onMount(() => {
    let stop: (() => void) | undefined;
    let alive = true;
    void (async () => {
      const unlisten = await listen<UpdateView>("update", (event) => (view = event.payload));
      if (!alive) return unlisten();
      stop = unlisten;
      view = await api.updateView();
    })();
    return () => {
      alive = false;
      stop?.();
    };
  });
</script>

<Card>
  <Row card={false} icon="download" title={t("update.title")} {hint}>
    {#if view.kind === "ready"}
      <Button onclick={() => api.updateInstall()}>{t("update.restart")}</Button>
    {:else if view.kind !== "unavailable"}
      <Button disabled={busy} onclick={async () => (view = await api.updateCheck())}>{t("update.check")}</Button>
    {/if}
  </Row>
  {#if view.kind !== "unavailable"}
    <Row card={false} icon="globe" title={t("update.auto")} hint={t("update.autoHint")}>
      <Toggle label={t("update.auto")} checked={ctx.settings.autoUpdate} onchange={(autoUpdate) => ctx.update({ autoUpdate })} />
    </Row>
  {/if}
</Card>
