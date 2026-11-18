<script lang="ts">
  import type { Snippet } from "svelte";

  // A group of settings on a lightly frosted panel.
  let { children }: { children: Snippet } = $props();
</script>

<div class="card">{@render children()}</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 15px;
    border: 1px solid rgb(255 255 255 / 0.08);
    border-radius: 12px;
    background: rgb(255 255 255 / 0.05);
  }
</style>
