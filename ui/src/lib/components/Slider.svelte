<script lang="ts">
  interface Props {
    label?: string;
    min: number;
    max: number;
    step?: number;
    value: number;
    /** Rendered at the end of the row, e.g. "Level 28". */
    readout?: string;
    /** One label per stop, positioned under the stop it belongs to. */
    ticks?: string[];
    disabled?: boolean;
    ariaLabel: string;
    /** Fires on every movement; the caller debounces the write. */
    oninput: (value: number) => void;
    /** Fires when the drag ends, so the caller can commit immediately. */
    oncommit?: (value: number) => void;
  }

  let {
    label,
    min,
    max,
    step = 1,
    value,
    readout,
    ticks,
    disabled = false,
    ariaLabel,
    oninput,
    oncommit,
  }: Props = $props();

  const fill = $derived(max > min ? ((value - min) / (max - min)) * 100 : 0);
</script>

<!-- A three-column grid rather than a flex row, so the tick strip can sit in
     the same column as the track. Laying ticks out across the whole row left
     them pointing at the label and the readout instead of at their stops. -->
<div class="slider" class:disabled>
  <span class="label">{label ?? ""}</span>
  <input
    type="range"
    {min}
    {max}
    {step}
    {value}
    {disabled}
    aria-label={ariaLabel}
    aria-valuetext={readout}
    style="--fill: {fill}%"
    oninput={(event) => oninput(event.currentTarget.valueAsNumber)}
    onchange={(event) => oncommit?.(event.currentTarget.valueAsNumber)}
  />
  <span class="readout">{readout ?? ""}</span>

  {#if ticks?.length}
    <!-- The thumb centre never reaches the track edges, so the strip is inset
         by half a thumb at each end and every label is centred on its stop. -->
    <div class="ticks" aria-hidden="true">
      {#each ticks as tick, index (index)}
        <span
          style="left: {ticks.length > 1 ? (index / (ticks.length - 1)) * 100 : 50}%"
        >
          {tick}
        </span>
      {/each}
    </div>
  {/if}
</div>

<style>
  .slider {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    column-gap: var(--gap);
    row-gap: 2px;
  }

  .label:not(:empty) {
    min-width: 130px;
  }

  .readout:not(:empty) {
    min-width: 62px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--fg-muted);
  }

  .ticks {
    grid-column: 2;
    position: relative;
    height: 14px;
    /* Half the thumb width, so 0% and 100% land under the thumb centres. */
    margin: 0 7px;
  }

  .ticks span {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    font-size: 11px;
    line-height: 14px;
    color: var(--fg-faint);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  input {
    grid-column: 2;
    min-width: 0;
    appearance: none;
    height: 20px;
    background: transparent;
    cursor: pointer;
  }

  input:disabled {
    cursor: default;
  }

  input::-webkit-slider-runnable-track {
    height: 5px;
    border-radius: 999px;
    background: linear-gradient(
      to right,
      var(--accent) 0 var(--fill),
      var(--bg-sunken) var(--fill) 100%
    );
    border: 1px solid var(--line);
  }

  input::-webkit-slider-thumb {
    appearance: none;
    width: 15px;
    height: 15px;
    margin-top: -6px;
    border-radius: 50%;
    background: var(--accent);
    border: 2px solid var(--bg-raised);
    box-shadow: 0 0 0 1px var(--line-strong);
  }

  .disabled input::-webkit-slider-thumb {
    background: var(--fg-faint);
  }
</style>
