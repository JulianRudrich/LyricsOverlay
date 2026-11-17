<script lang="ts">
  // Small line icons drawn on a 24 × 24 grid, in the current text color.
  type Name = "waveform" | "document" | "pause" | "retry";
  let { name, size = 14 }: { name: Name; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2.2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === "waveform"}
    <path d="M3 12h1.5M7 8v8M11 4v16M15 7v10M19 10v4M21.5 12H21" />
  {:else if name === "document"}
    <path d="M7 3h7l5 5v13H7z" />
    <path d="M14 3v5h5M10 13h6M10 17h6" />
  {:else if name === "pause"}
    <path d="M9 5v14M15 5v14" />
  {:else}
    <path d="M20 11a8 8 0 1 0-2.3 5.7" />
    <path d="M20 4v7h-7" />
  {/if}
</svg>
