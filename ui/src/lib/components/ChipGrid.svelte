<script lang="ts" generics="T extends string | number">
  interface Chip {
    value: T;
    label: string;
    /** A chip the device cannot do: visible, but not selectable. */
    unsupported?: boolean;
    title?: string;
  }

  interface Props {
    chips: Chip[];
    value: T | null;
    ariaLabel: string;
    /** Narrowest a chip may get before the grid wraps, in pixels. */
    minWidth?: number;
    onselect: (value: T) => void;
  }

  let { chips, value, ariaLabel, minWidth = 104, onselect }: Props = $props();
</script>

<div
  class="grid"
  role="radiogroup"
  aria-label={ariaLabel}
  style="--min-width: {minWidth}px"
>
  {#each chips as chip (chip.value)}
    <button
      type="button"
      role="radio"
      aria-checked={chip.value === value}
      aria-disabled={chip.unsupported}
      class:selected={chip.value === value}
      class:unsupported={chip.unsupported}
      title={chip.title}
      onclick={() => !chip.unsupported && onselect(chip.value)}
    >
      {chip.label}
    </button>
  {/each}
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(var(--min-width), 1fr));
    gap: var(--gap-sm);
  }

  button {
    padding: 7px 10px;
    text-align: center;
  }

  button.selected {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
    font-weight: 600;
  }

  button.selected:hover {
    background: var(--accent);
  }

  /* Unsupported modes stay visible so the list matches the hardware's own
     documentation, but they read as unavailable and do nothing on click. */
  button.unsupported {
    color: var(--fg-faint);
    border-style: dashed;
    cursor: not-allowed;
  }

  button.unsupported:hover {
    background: var(--bg-raised);
  }
</style>
