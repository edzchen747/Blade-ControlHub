<script lang="ts">
  interface Props {
    label: string;
    /** The consequence of switching it on, shown under the label. */
    hint?: string;
    checked: boolean;
    disabled?: boolean;
    error?: string;
    onchange: (checked: boolean) => void;
  }

  let { label, hint, checked, disabled = false, error, onchange }: Props = $props();
</script>

<div class="toggle-row" class:disabled>
  <label>
    <span class="text">
      <span class="label">{label}</span>
      {#if hint}<span class="hint">{hint}</span>{/if}
      {#if error}<span class="field-error">{error}</span>{/if}
    </span>
    <input
      type="checkbox"
      role="switch"
      {checked}
      {disabled}
      onchange={(event) => onchange(event.currentTarget.checked)}
    />
    <span class="track" aria-hidden="true"><span class="knob"></span></span>
  </label>
</div>

<style>
  label {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--gap);
    cursor: pointer;
  }

  .disabled label {
    cursor: default;
    opacity: 0.5;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .label {
    line-height: 1.3;
  }

  input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
  }

  .track {
    flex: 0 0 auto;
    position: relative;
    width: 38px;
    height: 21px;
    margin-top: 1px;
    border-radius: 999px;
    background: var(--bg-sunken);
    border: 1px solid var(--line-strong);
    transition:
      background var(--duration) var(--ease),
      border-color var(--duration) var(--ease);
  }

  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    background: var(--fg-muted);
    transition:
      transform var(--duration) var(--ease),
      background var(--duration) var(--ease);
  }

  input:checked + .track {
    background: var(--accent);
    border-color: var(--accent);
  }

  input:checked + .track .knob {
    background: var(--accent-fg);
    transform: translateX(17px);
  }

  input:focus-visible + .track {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
