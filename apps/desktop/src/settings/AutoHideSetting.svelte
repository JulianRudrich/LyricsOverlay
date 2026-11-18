<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { translator } from "../lib/i18n";
  import { connect } from "../lib/live";
  import SettingRow from "../lib/SettingRow.svelte";
  import Switch from "../lib/Switch.svelte";
  import type { AppInfo, Settings } from "../lib/types";

  // Self-contained: reads and changes the setting itself, so it fits into any pane.
  const EYE_OFF =
    "M3 3l18 18M10.6 5.1A9.6 9.6 0 0 1 12 5c5.5 0 9 7 9 7a16.5 16.5 0 0 1-3.2 4.2M6.6 6.6C4.3 8.2 3 12 3 12s3.5 7 9 7a9.3 9.3 0 0 0 4.4-1.1M9.9 9.9a3 3 0 0 0 4.2 4.2";

  let settings = $state<Settings | null>(null);
  let app = $state<AppInfo | null>(null);
  const t = $derived(translator(app?.language ?? "en"));

  async function change(autoHide: boolean) {
    if (settings) settings = { ...settings, autoHide };
    settings = await api.updateSettings({ autoHide });
  }

  onMount(() =>
    connect(
      { settings: (next) => (settings = next), app: (next) => (app = next) },
      (snapshot) => {
        settings = snapshot.settings;
        app = snapshot.app;
      },
    ),
  );
</script>

{#if settings && app}
  <SettingRow icon={EYE_OFF} title={t("settings.autoHide")} hint={t("settings.autoHideHint")}>
    <Switch checked={settings.autoHide} label={t("settings.autoHide")} onchange={change} />
  </SettingRow>
{/if}
