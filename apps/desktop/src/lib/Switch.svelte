<script lang="ts">
  // A black-and-white switch: on is the window's text color, the knob its background.
  let {
    checked,
    label,
    disabled = false,
    onchange,
  }: { checked: boolean; label: string; disabled?: boolean; onchange: (checked: boolean) => void } = $props();
</script>

<button type="button" class="switch" role="switch" aria-checked={checked} aria-label={label} {disabled} onclick={() => onchange(!checked)}>
  <span class="knob"></span>
</button>

<style>
  .switch {
    position: relative;
    flex: none;
    width: 34px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 10px;
    background: rgb(128 128 128 / 0.38);
    cursor: pointer;
    transition:
      background 160ms ease-out,
      transform 140ms cubic-bezier(0.23, 1, 0.32, 1);
  }

  .switch[aria-checked="true"] {
    background: var(--text, #fff);
  }

  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.3);
    transition:
      transform 180ms cubic-bezier(0.23, 1, 0.32, 1),
      background 160ms ease-out;
  }

  .switch[aria-checked="true"] .knob {
    transform: translateX(14px);
    background: var(--window, #111113);
  }

  .switch:active:not(:disabled) {
    transform: scale(0.97);
  }

  .switch:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .switch:focus-visible {
    outline: 2px solid var(--text, #fff);
    outline-offset: 2px;
  }

  @media (prefers-reduced-motion: reduce) {
    .switch,
    .knob {
      transition: none;
    }
  }
</style>
