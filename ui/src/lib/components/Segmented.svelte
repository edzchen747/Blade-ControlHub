<script lang="ts" generics="T extends string | number">
  interface Option {
    value: T;
    label: string;
    /** Shown as a small badge inside the segment, e.g. "LIVE". */
    badge?: string;
  }

  interface Props {
    options: Option[];
    value: T;
    ariaLabel: string;
    onselect: (value: T) => void;
  }

  let { options, value, ariaLabel, onselect }: Props = $props();
</script>

<div class="segmented" role="radiogroup" aria-label={ariaLabel}>
  {#each options as option (option.value)}
    <button
      type="button"
      role="radio"
      aria-checked={option.value === value}
      class:selected={option.value === value}
      onclick={() => onselect(option.value)}
    >
      {option.label}
      {#if option.badge}<span class="badge">{option.badge}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .segmented {
    display: inline-flex;
    padding: 3px;
    gap: 3px;
    background: var(--bg-sunken);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }

  button {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    border: 1px solid transparent;
    background: transparent;
    border-radius: calc(var(--radius) - 4px);
    padding: 4px 14px;
    color: var(--fg-muted);
    white-space: nowrap;
  }

  button.selected {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
  }

  button.selected:hover {
    background: var(--accent);
  }

  .badge {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 1px 5px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent-fg) 18%, transparent);
  }
</style>
