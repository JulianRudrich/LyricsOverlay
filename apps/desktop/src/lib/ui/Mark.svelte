<script lang="ts">
  // The Singline mark from design/app-icon.svg: a black tile with three white bars, dim, bright, dim.
  let { size = 32 }: { size?: number } = $props();
  const id = $props.id();
</script>

<svg width={size} height={size} viewBox="64 64 896 896" aria-hidden="true">
  <defs>
    <linearGradient id="{id}-tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#1c1c1e" />
      <stop offset="1" stop-color="#000" />
    </linearGradient>
  </defs>
  <rect x="64" y="64" width="896" height="896" rx="208" fill="url(#{id}-tile)" />
  <rect x="76" y="76" width="872" height="872" rx="196" fill="none" stroke="#fff" stroke-opacity="0.14" stroke-width="24" />
  <rect x="224" y="300" width="400" height="60" rx="30" fill="#fff" fill-opacity="0.4" />
  <rect x="224" y="466" width="576" height="92" rx="46" fill="#fff" />
  <rect x="224" y="664" width="472" height="60" rx="30" fill="#fff" fill-opacity="0.4" />
</svg>
