<script lang="ts">
  // A keyboard shortcut on a key cap, such as "⌥ ⇧ L".
  let { label }: { label: string } = $props();
</script>

<kbd class="keycap">{label}</kbd>

<style>
  .keycap {
    display: inline-block;
    padding: 6px 12px;
    border-radius: 8px;
    background: rgb(255 255 255 / 0.08);
    color: #fff;
    font-family: ui-rounded, -apple-system, BlinkMacSystemFont, "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
    font-size: 13px;
    font-weight: 500;
    line-height: 1.2;
    white-space: nowrap;
  }
</style>
