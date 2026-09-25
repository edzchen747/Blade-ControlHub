<script lang="ts">
  import { ACCENT_CONTROL, previewAccent, resetAccent, toHex } from "../accent";
  import { store } from "../store.svelte";

  // Reads the one shared value, so a change made on either page shows on both.
  const accentHex = $derived(store.state ? toHex(store.state.theme_color) : "#ffd700");
</script>

<div class="row-between">
  <span>Accent colour</span>
  <div class="row">
    <input
      type="color"
      class="swatch"
      value={accentHex}
      aria-label="Accent colour"
      oninput={(event) => previewAccent(event.currentTarget.value)}
    />
    <code class="code">{accentHex.toUpperCase()}</code>
    <button type="button" class="ghost" onclick={resetAccent}>Reset</button>
  </div>
</div>
{#if store.errors[ACCENT_CONTROL]}
  <span class="field-error">{store.errors[ACCENT_CONTROL]}</span>
{/if}

<style>
  .swatch {
    width: 38px;
    height: 28px;
    padding: 2px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    background: var(--bg-sunken);
    cursor: pointer;
  }
</style>
