<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  // A small glass button that gives a little while pressed.
  let { children, ...attributes }: Omit<HTMLButtonAttributes, "class"> & { children: Snippet } = $props();
</script>

<button type="button" {...attributes} class="button">{@render children()}</button>

<style>
  .button {
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: rgb(255 255 255 / 0.1);
    box-shadow: inset 0 0 0 0.5px rgb(255 255 255 / 0.14);
    color: #fff;
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    cursor: default;
    transition:
      transform var(--duration-press) var(--ease-out),
      background-color var(--duration-press) var(--ease-out);
  }

  .button:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.14);
  }

  .button:active:not(:disabled) {
    transform: scale(var(--press-scale));
  }

  .button:disabled {
    opacity: 0.4;
  }

  .button:focus-visible {
    outline: 2px solid rgb(255 255 255 / 0.7);
    outline-offset: 2px;
  }
</style>
