<script lang="ts">
  // Three dots that fill one after another while the singing pauses (intro,
  // instrumental break), breathing gently while the music plays.
  import { clamp } from "../lib/lyrics";

  let {
    progress,
    size,
    breathing,
    centered = false,
  }: { progress: number; size: number; breathing: boolean; centered?: boolean } = $props();
</script>

<span class="dots" class:breathing class:centered style:height="{size * 1.2}px" style:gap="{size * 0.3}px" aria-hidden="true">
  {#each [0, 1, 2] as dot (dot)}
    <span
      class="dot"
      style:width="{size * 0.32}px"
      style:height="{size * 0.32}px"
      style:opacity={0.22 + 0.78 * clamp(progress * 3 - dot, 0, 1)}
    ></span>
  {/each}
</span>

<style>
  .dots {
    display: inline-flex;
    align-items: center;
    vertical-align: top;
    transform-origin: left center;
    animation: breathe 1.8s ease-in-out infinite;
    animation-play-state: paused;
  }

  .breathing {
    animation-play-state: running;
  }

  /* Breathes from its middle where it stands centred, as in karaoke. */
  .centered {
    transform-origin: center;
  }

  .dot {
    flex: none;
    border-radius: 50%;
    background: #fff;
  }

  /* A slow sine: 1 ± 0.06 every 1.8 s. */
  @keyframes breathe {
    0%,
    50%,
    100% {
      transform: scale(1);
    }
    25% {
      transform: scale(1.06);
    }
    75% {
      transform: scale(0.94);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dots {
      animation: none;
    }
  }
</style>
