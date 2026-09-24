<script lang="ts">
  import Section from "../components/Section.svelte";
  import { keyMap, normalKeyLabel, razerKeyLabel } from "../keymap.svelte";

  // Razer special keys never reach Windows as virtual key codes, so capture
  // runs in the runtime's HID reader; ordinary keys are captured right here.
  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      if (keyMap.listeningRazer !== null) keyMap.cancelRazer();
      if (keyMap.listeningHypershift !== null) keyMap.cancelHypershift();
      return;
    }
    if (keyMap.listeningHypershift !== null && keyMap.applyHypershiftKey(event.key)) {
      event.preventDefault();
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<Section
  title="Razer special keys"
  hint="Remap M1, M2, mic mute, trackpad and the performance key. Press a key on the laptop while a row is listening; Esc cancels."
>
  <div class="table" role="table" aria-label="Razer special key mappings">
    <div class="head" role="row">
      <span role="columnheader">Label</span>
      <span role="columnheader">Key</span>
      <span role="columnheader">Action</span>
      <span role="columnheader"><span class="sr">Remove</span></span>
    </div>

    {#each keyMap.razer as row (row.id)}
      <div class="line" role="row">
        <div class="cells">
          <input
            class="input"
            placeholder="Label"
            bind:value={row.name}
            onchange={() => keyMap.save()}
            aria-label="Mapping label"
          />
          <button
            type="button"
            class="key"
            class:listening={keyMap.listeningRazer === row.id}
            class:invalid={keyMap.duplicateRow === row.id}
            onclick={() =>
              keyMap.listeningRazer === row.id
                ? keyMap.cancelRazer()
                : keyMap.listenRazer(row.id)}
          >
            {keyMap.listeningRazer === row.id ? "Press a key…" : razerKeyLabel(row.keyCode)}
          </button>
          <input
            class="input"
            placeholder="Action"
            bind:value={row.action}
            onchange={() => keyMap.save()}
            aria-label="Mapping action"
          />
          <button
            type="button"
            class="icon danger"
            aria-label="Remove mapping"
            onclick={() => keyMap.removeRazer(row.id)}
          >
            ✕
          </button>
        </div>
        {#if keyMap.duplicateRow === row.id}
          <!-- An error about one row belongs on that row, and stays until the
               user resolves it rather than expiring on a timer. -->
          <span class="field-error">That key is already assigned to another row.</span>
        {/if}
      </div>
    {/each}
  </div>

  <div class="row">
    <button type="button" disabled={!keyMap.canAddRazer} onclick={() => keyMap.addRazer()}>
      + Add mapping
    </button>
    {#if !keyMap.canAddRazer}
      <span class="hint">Finish the current row first.</span>
    {/if}
  </div>
</Section>

<Section
  title="Hypershift"
  hint="Keys held with Fn for secondary actions. Click a Key cell and press a letter or digit."
>
  <div class="table" role="table" aria-label="Hypershift mappings">
    <div class="head hypershift" role="row">
      <span role="columnheader">Key</span>
      <span role="columnheader">Action</span>
      <span role="columnheader"><span class="sr">Remove</span></span>
    </div>

    {#each keyMap.hypershift as row (row.id)}
      <div class="line" role="row">
        <div class="cells hypershift">
          <button
            type="button"
            class="key"
            class:listening={keyMap.listeningHypershift === row.id}
            class:invalid={keyMap.duplicateRow === row.id}
            onclick={() =>
              keyMap.listeningHypershift === row.id
                ? keyMap.cancelHypershift()
                : keyMap.listenHypershift(row.id)}
          >
            {keyMap.listeningHypershift === row.id
              ? "Press a key…"
              : normalKeyLabel(row.keyCode)}
          </button>
          <input class="input" placeholder="Not yet available" disabled aria-label="Hypershift action" />
          <button
            type="button"
            class="icon danger"
            aria-label="Remove mapping"
            onclick={() => keyMap.removeHypershift(row.id)}
          >
            ✕
          </button>
        </div>
        {#if keyMap.duplicateRow === row.id}
          <span class="field-error">That key is already assigned to another row.</span>
        {/if}
      </div>
    {/each}
  </div>

  <div class="row">
    <button
      type="button"
      disabled={!keyMap.canAddHypershift}
      onclick={() => keyMap.addHypershift()}
    >
      + Add mapping
    </button>
    {#if !keyMap.canAddHypershift}
      <span class="hint">Finish the current row first.</span>
    {/if}
  </div>
</Section>

<style>
  .table {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
  }

  .head,
  .cells {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 118px minmax(0, 1fr) 28px;
    gap: var(--gap-sm);
    align-items: center;
  }

  .head.hypershift,
  .cells.hypershift {
    grid-template-columns: 118px minmax(0, 1fr) 28px;
  }

  .head {
    font-size: 11.5px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-faint);
  }

  .line {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .key {
    font-family: var(--mono);
    font-size: 12.5px;
    padding: 5px 8px;
  }

  .key.listening {
    border-color: var(--accent);
    color: var(--accent);
    animation: pulse 1.2s ease-in-out infinite;
  }

  .key.invalid {
    border-color: var(--danger);
    color: var(--danger);
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  @keyframes pulse {
    0%,
    100% {
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 55%, transparent);
    }
    50% {
      box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 0%, transparent);
    }
  }
</style>
