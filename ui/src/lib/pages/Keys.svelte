<script lang="ts">
  import ActionDetail from "../components/ActionDetail.svelte";
  import ActionKind from "../components/ActionKind.svelte";
  import Section from "../components/Section.svelte";
  import { actionSummary, hasActionDetail, normalKeyLabel, razerKeyLabel } from "../actions";
  import { keyMap } from "../keymap.svelte";
  import { jumpTo } from "../scroll";
  import { store } from "../store.svelte";
  import type { Row } from "../keymap.svelte";

  const HYPERSHIFT_ID = "hypershift";

  const deviceLabels = $derived(store.state?.meta.device_action_labels ?? {});
  const builtInHypershift = $derived(store.state?.meta.built_in_hypershift ?? {});

  /** Cleared once the Hypershift section has scrolled out of view. */
  let hypershiftVisible = $state(true);

  // The two tables share one scroll page, so on a short window Hypershift sits
  // below the fold with nothing to say it is there. The jump only appears while
  // it is actually out of sight.
  $effect(() => {
    // The section renders its own anchor id, so it is found rather than bound:
    // a wrapper around it would either change the page spacing or, laid out as
    // `display: contents`, have no box for the observer to watch.
    const section = document.getElementById(HYPERSHIFT_ID);
    if (!section) return;

    const observer = new IntersectionObserver(
      ([entry]) => (hypershiftVisible = entry.isIntersecting),
      { root: section.closest(".content"), threshold: 0.12 },
    );
    observer.observe(section);
    return () => observer.disconnect();
  });

  function jumpToHypershift() {
    jumpTo(HYPERSHIFT_ID);
  }

  // Razer special keys never reach Windows as virtual key codes, so capture
  // runs in the runtime's HID reader; ordinary keys are captured right here.
  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      // A chord cell cancels itself and has already stopped the event; the key
      // cells are this page's to cancel.
      if (keyMap.listeningRazer !== null) keyMap.cancelRazer();
      if (keyMap.listeningHypershift !== null) keyMap.cancelHypershift();
      return;
    }
    if (keyMap.listeningHypershift !== null && keyMap.applyHypershiftKey(event.key)) {
      event.preventDefault();
    }
  }

  /**
   * What this Fn-layer key did before the row claimed it, if anything. Razer's
   * special keys have no built-in action to override, so only Hypershift rows
   * can replace something.
   */
  function overridden(row: Row): string | null {
    if (row.keyCode === null) return null;
    return builtInHypershift[row.keyCode] ?? null;
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<Section
  title="Razer special keys"
  hint="Every special key on the laptop — M1–M4, Copilot, mic, trackpad, performance, and whatever else this model has — does what you map it to here, and nothing until then. Press a key on the laptop while a row is listening; Esc cancels."
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
          <div class="key-cell">
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
          </div>
          <ActionKind action={row.action} onchange={(action) => keyMap.setAction(row, action)} />
          <button
            type="button"
            class="icon danger"
            aria-label="Remove mapping"
            onclick={() => keyMap.removeRazer(row.id)}
          >
            ✕
          </button>
        </div>
        {#if hasActionDetail(row.action)}
          <div class="detail">
            <ActionDetail
              owner={`razer-${row.id}`}
              action={row.action}
              onchange={(action) => keyMap.setAction(row, action)}
            />
          </div>
        {/if}
        {#if keyMap.duplicateRow === row.id}
          <!-- An error about one row belongs on that row, and stays until the
               user resolves it rather than expiring on a timer. -->
          <span class="field-error">That key is already assigned to another row.</span>
        {/if}
        {#if row.keyCode !== null && !keyMap.rowComplete(row, true)}
          <span class="field-error">
            {row.name.trim() === ""
              ? "Give this row a label and finish its action."
              : `Finish this row's action — ${actionSummary(row.action, deviceLabels)}.`}
          </span>
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
  id={HYPERSHIFT_ID}
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
      {@const replaces = overridden(row)}
      <div class="line" role="row">
        <div class="cells hypershift">
          <div class="key-cell">
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
            {#if replaces}
              <span class="badge" title={`Overrides the built-in action: ${replaces}`}>
                <span class="sr">Overrides the built-in action: {replaces}</span>
                <span aria-hidden="true">!</span>
              </span>
            {/if}
          </div>
          <ActionKind action={row.action} onchange={(action) => keyMap.setAction(row, action)} />
          <button
            type="button"
            class="icon danger"
            aria-label="Remove mapping"
            onclick={() => keyMap.removeHypershift(row.id)}
          >
            ✕
          </button>
        </div>
        {#if hasActionDetail(row.action)}
          <div class="detail">
            <ActionDetail
              owner={`hypershift-${row.id}`}
              action={row.action}
              onchange={(action) => keyMap.setAction(row, action)}
            />
          </div>
        {/if}
        {#if keyMap.duplicateRow === row.id}
          <span class="field-error">That key is already assigned to another row.</span>
        {/if}
        {#if row.keyCode !== null && !keyMap.rowComplete(row, false)}
          <span class="field-error">
            Finish this row's action — {actionSummary(row.action, deviceLabels)}.
          </span>
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

{#if keyMap.saveError}
  <span class="field-error">Mappings could not be saved: {keyMap.saveError}</span>
{/if}

{#if !hypershiftVisible}
  <button type="button" class="jump" onclick={jumpToHypershift}>Jump to Hypershift ↓</button>
{/if}

<style>
  .table {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
  }

  .head,
  .cells {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 150px minmax(0, 1fr) 28px;
    gap: var(--gap-sm);
    align-items: center;
  }

  .head.hypershift,
  .cells.hypershift {
    grid-template-columns: 150px minmax(0, 1fr) 28px;
  }

  /* An action's own control gets the width of the whole list: a macro's steps
     or an application and its arguments do not fit in one column. */
  .detail {
    padding: 2px 0 var(--gap-xs);
  }

  .head {
    font-size: 11.5px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-faint);
    align-items: center;
  }

  .line {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  /* A row is two lines tall once its action has a control of its own, so where
     one row ends and the next begins has to be drawn rather than inferred. */
  .line + .line {
    border-top: 1px solid var(--line);
    padding-top: var(--gap-sm);
  }

  .key-cell {
    display: flex;
    align-items: center;
    gap: var(--gap-xs);
  }

  .key {
    font-family: var(--mono);
    font-size: 12.5px;
    padding: 5px 8px;
    flex: 1 1 auto;
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

  /* A key that already did something keeps saying so, rather than silently
     losing its stock behaviour. */
  .badge {
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    font-size: 10px;
    font-weight: 700;
    color: var(--bg);
    background: var(--warn);
    cursor: help;
  }

  .jump {
    position: sticky;
    bottom: 0;
    align-self: center;
    background: var(--bg-raised);
    border-color: var(--accent);
    color: var(--accent);
    box-shadow: var(--shadow);
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
