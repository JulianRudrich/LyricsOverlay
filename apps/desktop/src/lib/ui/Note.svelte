<script lang="ts">
  import type { Snippet } from "svelte";

  // A short explanation below a card.
  let { children }: { children: Snippet } = $props();
</script>

<p class="note">{@render children()}</p>

<style>
  .note {
    margin: 0;
    padding: 0 4px;
    font-size: 11px;
    line-height: 1.4;
    color: rgb(255 255 255 / 0.55);
  }
</style>
