<script lang="ts">
  import { KEY_CATALOG, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN, type Chord } from "../types";

  interface Props {
    chord: Chord;
    onchange: (chord: Chord) => void;
  }

  let { chord, onchange }: Props = $props();

  const MODIFIERS: Array<{ bit: number; label: string }> = [
    { bit: MOD_CTRL, label: "Ctrl" },
    { bit: MOD_ALT, label: "Alt" },
    { bit: MOD_SHIFT, label: "Shift" },
    { bit: MOD_WIN, label: "Win" },
  ];

  function toggle(bit: number) {
    onchange({ ...chord, modifiers: chord.modifiers ^ bit });
  }
</script>

<div class="key-select">
  <div class="modifiers" role="group" aria-label="Modifiers">
    {#each MODIFIERS as modifier (modifier.bit)}
      <button
        type="button"
        class="modifier"
        class:on={(chord.modifiers & modifier.bit) !== 0}
        aria-pressed={(chord.modifiers & modifier.bit) !== 0}
        onclick={() => toggle(modifier.bit)}
      >
        {modifier.label}
      </button>
    {/each}
  </div>

  <select
    class="select"
    class:invalid={chord.key === 0}
    value={chord.key}
    aria-label="Key to send"
    onchange={(event) => onchange({ ...chord, key: Number(event.currentTarget.value) })}
  >
    <option value={0} disabled>Choose a key…</option>
    {#each KEY_CATALOG as group (group.group)}
      <optgroup label={group.group}>
        {#each group.keys as [code, label] (code)}
          <option value={code}>{label}</option>
        {/each}
      </optgroup>
    {/each}
  </select>
</div>

<style>
  .key-select {
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
    min-width: 0;
  }

  .modifiers {
    display: flex;
    gap: var(--gap-xs);
  }

  .modifier {
    flex: 1 1 0;
    font-size: 11.5px;
    padding: 3px 0;
    color: var(--fg-muted);
  }

  .modifier.on {
    border-color: var(--accent);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
</style>
