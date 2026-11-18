<script lang="ts">
  // Title on the left, value on the right, the slider underneath.
  let {
    label,
    value,
    min,
    max,
    step = 1,
    display,
    disabled = false,
    onchange,
  }: {
    label: string;
    value: number;
    min: number;
    max: number;
    step?: number;
    display: string;
    disabled?: boolean;
    onchange: (value: number) => void;
  } = $props();

  const id = $props.id();
  const fill = $derived(`${((value - min) / (max - min)) * 100}%`);
</script>

<div class="slider">
  <div class="heading">
    <label for={id}>{label}</label>
    <output for={id}>{display}</output>
  </div>
  <input {id} type="range" {min} {max} {step} {value} {disabled} style:--fill={fill} oninput={(event) => onchange(Number(event.currentTarget.value))} />
</div>

<style>
  .slider {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .heading {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  label {
    font-size: 13px;
    font-weight: 500;
  }

  output {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.55);
    white-space: nowrap;
  }

  input {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 18px;
    margin: 0;
    background: transparent;
    cursor: default;
  }

  input::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(to right, #fff var(--fill), rgb(255 255 255 / 0.16) var(--fill));
  }

  input::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 18px;
    height: 18px;
    margin-top: -7px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.4);
  }

  input:focus-visible {
    outline: none;
  }

  input:focus-visible::-webkit-slider-thumb {
    box-shadow:
      0 0 0 3px rgb(255 255 255 / 0.3),
      0 1px 3px rgb(0 0 0 / 0.4);
  }

  input:disabled {
    opacity: 0.4;
  }
</style>
