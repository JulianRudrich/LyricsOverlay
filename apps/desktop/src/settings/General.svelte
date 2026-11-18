<script lang="ts">
  import { hotkeyLabel } from "../lib/hotkey";
  import type { MessageKey } from "../lib/i18n";
  import type { Language, SourcePreference } from "../lib/types";
  import KeyCap from "../lib/ui/KeyCap.svelte";
  import Row from "../lib/ui/Row.svelte";
  import Select from "../lib/ui/Select.svelte";
  import Toggle from "../lib/ui/Toggle.svelte";
  import type { PaneContext } from "./panes";

  let { ctx }: { ctx: PaneContext } = $props();
  const t = $derived(ctx.t);
  const settings = $derived(ctx.settings);
  const app = $derived(ctx.app);
  const hotkey = $derived(hotkeyLabel(settings.hotkey, app.platform));

  const sources: SourcePreference[] = ["automatic", "spotify", "appleMusic", "amazonMusic"];
  const languages: Language[] = ["system", "de", "en"];
</script>

<Row icon="note" title={t("settings.source")} hint={t("settings.sourceHint")}>
  <Select
    label={t("settings.source")}
    value={settings.source}
    disabled={app.demo}
    options={sources.map((value) => ({ value, label: t(`source.${value}` as MessageKey) }))}
    onchange={(source) => ctx.update({ source })}
  />
</Row>

<Row icon="players" title={t("settings.includeOther")} hint={t("settings.includeOtherHint")}>
  <Toggle
    label={t("settings.includeOther")}
    checked={settings.includeOtherPlayers}
    disabled={app.demo || settings.source !== "automatic"}
    onchange={(includeOtherPlayers) => ctx.update({ includeOtherPlayers })}
  />
</Row>

<Row icon="eyeOff" title={t("settings.autoHide")} hint={t("settings.autoHideHint")}>
  <Toggle label={t("settings.autoHide")} checked={settings.autoHide} onchange={(autoHide) => ctx.update({ autoHide })} />
</Row>

<Row icon="power" title={t("settings.launchAtLogin")} hint={t("settings.launchAtLoginHint")}>
  <Toggle label={t("settings.launchAtLogin")} checked={app.launchAtLogin} onchange={(enabled) => ctx.setLaunchAtLogin(enabled)} />
</Row>

<Row icon="globe" title={t("settings.language")} hint={t("settings.languageHint")}>
  <Select
    label={t("settings.language")}
    value={settings.language}
    options={languages.map((value) => ({ value, label: t(`language.${value}` as MessageKey) }))}
    onchange={(language) => ctx.update({ language })}
  />
</Row>

<Row
  icon="keyboard"
  title={t("settings.hotkey")}
  hint={app.hotkeyError ? t("settings.hotkeyError", { hotkey }) : t("settings.hotkeyHint")}
  warning={app.hotkeyError}
>
  <KeyCap label={hotkey} />
</Row>
