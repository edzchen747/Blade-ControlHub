<script lang="ts">
  interface Props {
    label: string;
    /** Number of lit levels above off. The firmware exposes five. */
    steps: number;
    /** 0 is off; `steps` is full brightness. */
    value: number;
    /** Text for the current value, e.g. "Off" or "60%". */
    readout: string;
    ariaLabel: string;
    onselect: (level: number) => void;
  }

  let { label, steps, value, readout, ariaLabel, onselect }: Props = $props();

  const indexes = $derived(Array.from({ length: steps }, (_, index) => index));

  function onKeyDown(event: KeyboardEvent) {
    const delta =
      event.key === "ArrowRight" || event.key === "ArrowUp"
        ? 1
        : event.key === "ArrowLeft" || event.key === "ArrowDown"
          ? -1
          : 0;
    if (delta === 0) return;
    event.preventDefault();
    onselect(Math.min(steps, Math.max(0, value + delta)));
  }
</script>

<!-- The firmware has six discrete backlight states, not a continuous range: off
     plus five levels. Off is a target of its own rather than a hidden state at
     the bottom of a slider, so turning the backlight off is one click from any
     level. -->
<div class="row-between">
  <span>{label}</span>
  <div class="control">
    <button
      type="button"
      class="off"
      class:selected={value === 0}
      aria-pressed={value === 0}
      onclick={() => onselect(0)}
    >
      Off
    </button>
    <div
      class="dots"
      role="slider"
      tabindex="0"
      aria-label={ariaLabel}
      aria-valuemin={0}
      aria-valuemax={steps}
      aria-valuenow={value}
      aria-valuetext={readout}
      onkeydown={onKeyDown}
    >
      {#each indexes as index (index)}
        <button
          type="button"
          class:on={index < value}
          aria-label="{ariaLabel} level {index + 1}"
          onclick={() => onselect(index + 1)}
        ></button>
      {/each}
    </div>
    <span class="readout">{readout}</span>
  </div>
</div>

<style>
  .control {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
  }

  .off {
    font-size: 12px;
    padding: 3px 10px;
  }

  .off.selected {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
    font-weight: 600;
  }

  .off.selected:hover {
    background: var(--accent);
  }

  .dots {
    display: flex;
    gap: 7px;
    padding: 3px;
    border-radius: var(--radius-sm);
  }

  .dots button {
    width: 15px;
    height: 15px;
    padding: 0;
    border-radius: 50%;
    border: 1px solid var(--line-strong);
    background: var(--bg-sunken);
    transition:
      background var(--duration) var(--ease),
      border-color var(--duration) var(--ease);
  }

  .dots button.on {
    background: var(--accent);
    border-color: var(--accent);
  }

  .readout {
    min-width: 42px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--fg-muted);
    font-size: 12.5px;
  }
</style>
