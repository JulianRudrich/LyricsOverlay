<script lang="ts">
  import type { MessageKey } from "../lib/i18n";
  import { reveal } from "../lib/motion";
  import type { Layout } from "../lib/types";
  import Card from "../lib/ui/Card.svelte";
  import Segmented from "../lib/ui/Segmented.svelte";
  import Slider from "../lib/ui/Slider.svelte";
  import type { PaneContext } from "./panes";

  let { ctx }: { ctx: PaneContext } = $props();
  const t = $derived(ctx.t);
  const settings = $derived(ctx.settings);
  const overlayOpacity = $derived(Math.round(settings.overlayOpacity * 100));
  const backgroundOpacity = $derived(Math.round(settings.backgroundOpacity * 100));

  const layouts: Layout[] = ["caption", "focus", "context"];
</script>

<Card>
  <div class="field">
    <span class="label">{t("settings.layout")}</span>
    <Segmented
      label={t("settings.layout")}
      value={settings.layout}
      options={layouts.map((value) => ({ value, label: t(`layout.${value}` as MessageKey) }))}
      onchange={(layout) => ctx.update({ layout })}
    />
  </div>
</Card>

<Card>
  <Slider
    label={t("settings.fontSize")}
    value={settings.fontSize}
    min={14}
    max={44}
    display={t("unit.points", { value: settings.fontSize })}
    onchange={(fontSize) => ctx.update({ fontSize })}
  />
</Card>

<Card>
  <Slider
    label={t("settings.overlayOpacity")}
    value={overlayOpacity}
    min={15}
    max={100}
    display={t("unit.percent", { value: overlayOpacity })}
    onchange={(value) => ctx.update({ overlayOpacity: value / 100 })}
  />
  <Slider
    label={t("settings.backgroundOpacity")}
    value={backgroundOpacity}
    min={0}
    max={100}
    display={t("unit.percent", { value: backgroundOpacity })}
    onchange={(value) => ctx.update({ backgroundOpacity: value / 100 })}
  />
</Card>

<Card>
  <Slider
    label={t("settings.width")}
    value={settings.width}
    min={260}
    max={800}
    step={10}
    display={t("unit.points", { value: settings.width })}
    onchange={(width) => ctx.update({ width })}
  />
  <!-- Only the Context layout uses the height. -->
  {#if settings.layout === "context"}
    <div use:reveal>
      <Slider
        label={t("settings.height")}
        value={settings.height}
        min={160}
        max={500}
        step={10}
        display={t("unit.points", { value: settings.height })}
        onchange={(height) => ctx.update({ height })}
      />
    </div>
  {/if}
</Card>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .label {
    font-size: 13px;
    font-weight: 500;
  }
</style>
