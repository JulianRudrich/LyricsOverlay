<script module lang="ts">
  import type { GlyphName } from "./Glyph.svelte";

  export interface SidebarItem {
    id: string;
    icon: GlyphName;
    label: string;
  }
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import Glyph from "./Glyph.svelte";
  import Mark from "./Mark.svelte";
  import { glide } from "./glide";

  // The app mark and name on top, then one item per pane. The selection glides
  // to the clicked item; items give a little while pressed.
  let {
    title,
    caption,
    items,
    selected,
    label,
    panel,
    onselect,
    footer,
  }: {
    title: string;
    caption: string;
    items: SidebarItem[];
    selected: string;
    label: string;
    /** The id of the element the items switch. */
    panel: string;
    onselect: (id: string) => void;
    /** At the bottom, below the items. */
    footer?: Snippet;
  } = $props();

  const index = $derived(items.findIndex((item) => item.id === selected));
  let list = $state<HTMLElement>();

  function keydown(event: KeyboardEvent) {
    const last = items.length - 1;
    const target =
      event.key === "ArrowDown" ? (index + 1) % items.length
      : event.key === "ArrowUp" ? (index + last) % items.length
      : event.key === "Home" ? 0
      : event.key === "End" ? last
      : -1;
    const item = items[target];
    if (!item) return;
    event.preventDefault();
    onselect(item.id);
    list?.querySelectorAll<HTMLElement>("[data-item]")[target]?.focus();
  }
</script>

<nav class="sidebar">
  <div class="brand">
    <Mark size={32} />
    <div>
      <div class="title">{title}</div>
      <div class="caption">{caption}</div>
    </div>
  </div>
  <div class="items" role="tablist" aria-orientation="vertical" aria-label={label} bind:this={list} use:glide={index}>
    <span class="pill" data-pill aria-hidden="true"></span>
    {#each items as item (item.id)}
      {@const current = item.id === selected}
      <button
        type="button"
        role="tab"
        id="tab-{item.id}"
        data-item
        aria-selected={current}
        aria-controls={panel}
        tabindex={current ? 0 : -1}
        class:current
        onclick={() => onselect(item.id)}
        onkeydown={keydown}
      >
        <Glyph name={item.icon} size={16} />
        <span>{item.label}</span>
      </button>
    {/each}
  </div>
  {#if footer}
    <div class="footer">{@render footer()}</div>
  {/if}
</nav>

<style>
  .sidebar {
    flex: none;
    box-sizing: border-box;
    width: 200px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0 12px;
    background: rgb(0 0 0 / 0.15);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: var(--top-inset, 24px) 10px 22px;
  }

  .title {
    font-size: 18px;
    font-weight: 600;
    line-height: 1.2;
  }

  .caption {
    font-size: 11px;
    color: rgb(255 255 255 / 0.5);
  }

  .footer {
    margin-top: auto;
    padding-bottom: 14px;
  }

  .items {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .pill {
    position: absolute;
    top: 0;
    left: 0;
    border-radius: 8px;
    background: rgb(255 255 255 / 0.12);
    pointer-events: none;
    transition:
      transform var(--duration-pane) var(--ease-out),
      width var(--duration-pane) var(--ease-out),
      height var(--duration-pane) var(--ease-out);
  }

  button {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: rgb(255 255 255 / 0.75);
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    text-align: left;
    cursor: default;
    transition:
      color var(--duration-pane) var(--ease-out),
      transform var(--duration-press) var(--ease-out);
  }

  button.current {
    color: #fff;
  }

  button:active {
    transform: scale(var(--press-scale-subtle));
  }

  button:focus-visible {
    outline: 2px solid rgb(255 255 255 / 0.7);
    outline-offset: -2px;
  }

  @media (prefers-reduced-motion: reduce) {
    .pill {
      transition: none;
    }
  }
</style>
