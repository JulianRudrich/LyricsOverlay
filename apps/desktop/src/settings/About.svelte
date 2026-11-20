<script lang="ts">
  import { api } from "../lib/api";
  import Button from "../lib/ui/Button.svelte";
  import Card from "../lib/ui/Card.svelte";
  import Glyph, { type GlyphName } from "../lib/ui/Glyph.svelte";
  import Mark from "../lib/ui/Mark.svelte";
  import Note from "../lib/ui/Note.svelte";
  import Row from "../lib/ui/Row.svelte";
  import type { PaneContext } from "./panes";
  import Updates from "./Updates.svelte";

  let { ctx }: { ctx: PaneContext } = $props();
  const t = $derived(ctx.t);

  // Opened in the browser by the opener plugin; capabilities/default.json allows exactly these.
  const links = $derived<{ href: string; icon: GlyphName; title: string; hint: string }[]>([
    {
      href: "https://github.com/JulianRudrich/Singline",
      icon: "code",
      title: t("about.project"),
      hint: "github.com/JulianRudrich/Singline",
    },
    {
      href: "https://github.com/JulianRudrich/Singline/blob/main/LICENSE",
      icon: "license",
      title: t("about.license"),
      hint: "GPL-3.0-or-later",
    },
    { href: "https://lrclib.net/", icon: "lyrics", title: t("about.lyrics"), hint: "lrclib.net" },
  ]);
</script>

<Card>
  <div class="identity">
    <Mark size={52} />
    <div>
      <div class="name">{t("app.name")}</div>
      <div class="version">{t("settings.version", { version: ctx.app.version })}</div>
    </div>
  </div>
</Card>
<Updates {ctx} />
<Note>{t("about.privacy")}</Note>

<Card>
  {#each links as link (link.href)}
    <a class="link" href={link.href} target="_blank" rel="noreferrer">
      <Row card={false} icon={link.icon} title={link.title} hint={link.hint}>
        <span class="external"><Glyph name="external" size={14} /></span>
      </Row>
    </a>
  {/each}
</Card>

<Card>
  <Row card={false} icon="about" title={t("about.report")} hint={t("about.reportHint")}>
    <Button onclick={() => api.reportProblem()}>{t("about.reportButton")}</Button>
  </Row>
  <Row card={false} icon="license" title={t("about.log")} hint={t("about.logHint")}>
    <Button onclick={() => api.showLogFile()}>{t("about.logButton")}</Button>
  </Row>
</Card>

<style>
  .identity {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .name {
    font-size: 20px;
    font-weight: 600;
    line-height: 1.2;
  }

  .version {
    margin-top: 2px;
    font-size: 12px;
    color: rgb(255 255 255 / 0.55);
  }

  .link {
    margin: -6px -8px;
    padding: 6px 8px;
    border-radius: 8px;
    color: inherit;
    text-decoration: none;
    transition:
      background-color var(--duration-press) var(--ease-out),
      transform var(--duration-press) var(--ease-out);
  }

  .link:hover {
    background: rgb(255 255 255 / 0.05);
  }

  .link:active {
    transform: scale(var(--press-scale-subtle));
  }

  .link:focus-visible {
    outline: 2px solid rgb(255 255 255 / 0.7);
    outline-offset: 0;
  }

  .external {
    display: grid;
    color: rgb(255 255 255 / 0.55);
  }
</style>
