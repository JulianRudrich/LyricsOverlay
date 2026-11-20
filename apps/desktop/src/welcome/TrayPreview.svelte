<script lang="ts">
  import type { AppInfo } from "../lib/types";

  // A sketch of the screen: the menu bar (the taskbar on Windows) with the
  // Singline icon pulsing in it, and the lyrics in their corner.
  let { platform, label }: { platform: AppInfo["platform"]; label: string } = $props();
  const bottom = $derived(platform === "windows");
</script>

<figure class="screen" class:bottom aria-label={label}>
  <div class="bar">
    {#if !bottom}
      <span class="menus" aria-hidden="true"><i></i><i></i><i></i></span>
    {/if}
    <span class="tray">
      {#if bottom}<span class="chevron" aria-hidden="true">⌃</span>{/if}
      <span class="singline">
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
          <rect x="2.5" y="3.5" width="7" height="1.6" rx="0.8" />
          <rect x="2.5" y="7.2" width="11" height="1.6" rx="0.8" />
          <rect x="2.5" y="10.9" width="8.5" height="1.6" rx="0.8" />
        </svg>
      </span>
      <svg class="dim" width="15" height="15" viewBox="0 0 24 24" aria-hidden="true">
        <path d="M2 9a15 15 0 0 1 20 0M5.5 12.5a10 10 0 0 1 13 0M9 16a5 5 0 0 1 6 0M12 19.5h.01" />
      </svg>
      <svg class="dim" width="18" height="15" viewBox="0 0 28 24" aria-hidden="true">
        <rect x="2" y="7" width="20" height="10" rx="3" />
        <path d="M25 10.5v3" />
        <rect class="fill" x="4.5" y="9.5" width="13" height="5" rx="1.2" />
      </svg>
      <span class="clock">9:41</span>
    </span>
  </div>
  <div class="lyrics" aria-hidden="true">
    <i class="line dim"></i>
    <i class="line bright"></i>
    <i class="line dim short"></i>
  </div>
</figure>

<style>
  .screen {
    position: relative;
    height: 150px;
    margin: 0;
    overflow: hidden;
    border: 1px solid rgb(255 255 255 / 0.1);
    border-radius: 12px;
    background:
      radial-gradient(120% 90% at 20% 110%, rgb(255 255 255 / 0.1), transparent 60%),
      radial-gradient(90% 80% at 90% 0%, rgb(255 255 255 / 0.08), transparent 55%),
      rgb(0 0 0 / 0.35);
  }

  .bar {
    position: absolute;
    inset: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 26px;
    padding: 0 10px;
    background: rgb(255 255 255 / 0.1);
    color: #fff;
  }

  .bottom .bar {
    inset: auto 0 0;
    height: 32px;
    justify-content: flex-end;
  }

  .menus {
    display: flex;
    gap: 10px;
  }

  .menus i {
    width: 26px;
    height: 6px;
    border-radius: 3px;
    background: rgb(255 255 255 / 0.3);
  }

  .menus i:first-child {
    width: 16px;
    background: rgb(255 255 255 / 0.55);
  }

  .tray {
    display: flex;
    align-items: center;
    gap: 9px;
  }

  .tray svg {
    display: block;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
  }

  .tray .dim {
    opacity: 0.6;
  }

  .tray .fill {
    fill: currentColor;
    stroke: none;
  }

  .chevron {
    font-size: 11px;
    opacity: 0.6;
  }

  /* The icon to find: a soft ring keeps pulsing around it. */
  .singline {
    display: grid;
    place-items: center;
    width: 22px;
    height: 20px;
    border-radius: 6px;
    background: rgb(255 255 255 / 0.22);
    animation: pulse 1.8s var(--ease-out) infinite;
  }

  .singline svg {
    fill: #fff;
    stroke: none;
  }

  @keyframes pulse {
    0% {
      box-shadow: 0 0 0 0 rgb(255 255 255 / 0.55);
    }

    70%,
    100% {
      box-shadow: 0 0 0 9px rgb(255 255 255 / 0);
    }
  }

  .clock {
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    opacity: 0.85;
  }

  /* The overlay in its corner: three lines, the middle one bright. */
  .lyrics {
    position: absolute;
    top: 38px;
    right: 12px;
    display: flex;
    flex-direction: column;
    gap: 7px;
    width: 132px;
    padding: 12px 14px;
    border-radius: 10px;
    background: rgb(0 0 0 / 0.72);
  }

  .bottom .lyrics {
    top: 12px;
  }

  .line {
    height: 6px;
    border-radius: 3px;
    background: rgb(255 255 255 / 0.32);
  }

  .line.bright {
    height: 8px;
    background: #fff;
  }

  .line.short {
    width: 70%;
  }

  @media (prefers-reduced-motion: reduce) {
    .singline {
      animation: none;
      box-shadow: 0 0 0 3px rgb(255 255 255 / 0.35);
    }
  }
</style>
