<script lang="ts" generics="T extends string">
  import Glyph from "./Glyph.svelte";

  // A pop-up picker: the native menu, dressed like the rest of the glass.
  let {
    value,
    options,
    label,
    disabled = false,
    onchange,
  }: {
    value: T;
    options: { value: T; label: string }[];
    label: string;
    disabled?: boolean;
    onchange: (value: T) => void;
  } = $props();
</script>

<span class="select" class:disabled>
  <select {value} {disabled} aria-label={label} onchange={(event) => onchange(event.currentTarget.value as T)}>
    {#each options as option (option.value)}
      <option value={option.value}>{option.label}</option>
    {/each}
  </select>
  <span class="chevrons"><Glyph name="chevrons" size={12} /></span>
</span>

<style>
  .select {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  select {
    -webkit-appearance: none;
    appearance: none;
    height: 24px;
    padding: 0 26px 0 10px;
    border: 0;
    border-radius: 6px;
    background: rgb(255 255 255 / 0.1);
    box-shadow:
      inset 0 0 0 0.5px rgb(255 255 255 / 0.14),
      0 1px 1px rgb(0 0 0 / 0.15);
    color: #fff;
    font: inherit;
    font-size: 13px;
    cursor: default;
    transition: background-color var(--duration-press) var(--ease-out);
  }

  select:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.14);
  }

  select:focus-visible {
    outline: 2px solid rgb(255 255 255 / 0.7);
    outline-offset: 2px;
  }

  option {
    background: #1c1c1e;
    color: #fff;
  }

  .chevrons {
    position: absolute;
    right: 7px;
    display: grid;
    color: rgb(255 255 255 / 0.75);
    pointer-events: none;
  }

  .disabled {
    opacity: 0.4;
  }
</style>
