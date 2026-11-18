<script lang="ts">
  // Cover art in a small rounded square. A note on a faint tile stands in
  // until the picture has loaded, and for players that have none.
  let { url, size = 36 }: { url: string | null; size?: number } = $props();

  // The overlay may only show pictures from the web or sent along as data.
  const source = $derived(url && /^(https:|data:image\/)/.test(url) ? url : null);
  let loaded = $state(false);
  let failed = $state(false);

  $effect(() => {
    void source;
    loaded = false;
    failed = false;
  });
</script>

<div class="cover" style:width="{size}px" style:height="{size}px">
  <svg class="note" width="16" height="16" viewBox="0 0 24 24" aria-hidden="true">
    <path d="M13 17.5V4.5c2.4.4 4.6 2 5.4 4.2" />
    <circle cx="10" cy="17.5" r="3" />
  </svg>
  {#if source && !failed}
    <img src={source} alt="" class:loaded onload={() => (loaded = true)} onerror={() => (failed = true)} />
  {/if}
</div>

<style>
  .cover {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 6px;
    background: rgb(255 255 255 / 0.08);
    box-shadow: 0 2px 4px rgb(0 0 0 / 0.18);
    color: rgb(255 255 255 / 0.6);
    overflow: hidden;
  }

  /* The hairline border sits on top of the picture. */
  .cover::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.08);
    pointer-events: none;
  }

  .note {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .note circle {
    fill: currentColor;
  }

  img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0;
    transition: opacity 0.35s cubic-bezier(0.23, 1, 0.32, 1);
  }

  img.loaded {
    opacity: 1;
  }

  @media (prefers-reduced-motion: reduce) {
    img {
      transition: none;
    }
  }
</style>
