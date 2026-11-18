<script lang="ts">
  import type { Snippet } from "svelte";

  // One settings row as a card: icon, title and hint on the left, the control on the right.
  // `icon` is the path of a line icon on a 24 × 24 grid.
  let { icon, title, hint, children }: { icon: string; title: string; hint: string; children: Snippet } = $props();
</script>

<div class="row">
  <span class="icon" aria-hidden="true">
    <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
      <path d={icon} />
    </svg>
  </span>
  <span class="text">
    <span class="title">{title}</span>
    <span class="hint">{hint}</span>
  </span>
  <span class="control">{@render children()}</span>
</div>

<style>
  /* Fill and border are drawn in the text color: white 5 % and 8 % on dark
     glass, and still visible on a light window. */
  .row {
    position: relative;
    isolation: isolate;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px;
    border-radius: 12px;
  }

  .row::before,
  .row::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    pointer-events: none;
  }

  .row::before {
    z-index: -1;
    background: currentColor;
    opacity: 0.05;
  }

  .row::after {
    border: 1px solid currentColor;
    opacity: 0.08;
  }

  .icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 26px;
    opacity: 0.85;
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .title {
    font-size: 13px;
    font-weight: 500;
  }

  .hint {
    font-size: 11.5px;
    opacity: 0.55;
  }

  .control {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
