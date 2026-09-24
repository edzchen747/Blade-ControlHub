<script lang="ts">
  import { store } from "../store.svelte";
  import Segmented from "./Segmented.svelte";
  import type { PowerProfile } from "../types";

  const options = $derived([
    { value: "Ac" as PowerProfile, label: "AC Power", badge: store.live === "Ac" ? "LIVE" : undefined },
    { value: "Battery" as PowerProfile, label: "Battery", badge: store.live === "Battery" ? "LIVE" : undefined },
  ]);

  const otherName = $derived(store.editing === "Ac" ? "AC power" : "Battery");
</script>

<div class="bar">
  <span class="caption">Editing</span>
  <Segmented
    {options}
    value={store.editing}
    ariaLabel="Power profile being edited"
    onselect={(profile) => store.selectProfile(profile)}
  />
  {#if !store.editingLive}
    <!-- The single clearest failure of the old UI was letting the user believe
         an edit to the inactive profile had taken effect. Say so plainly. -->
    <span class="notice">Changes apply when the laptop switches to {otherName}.</span>
  {/if}
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: var(--gap);
    padding: var(--gap-sm) var(--gap-lg);
    border-bottom: 1px solid var(--line);
    background: var(--bg);
    flex: 0 0 auto;
  }

  .caption {
    font-size: 12px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-faint);
  }

  .notice {
    font-size: 12.5px;
    color: var(--warn);
  }
</style>
