<script lang="ts">
  import { store } from "../store.svelte";
  import type { PageId } from "../pages";

  interface Props {
    pages: Array<{ id: PageId; label: string; experimental?: boolean }>;
    current: PageId;
    onselect: (page: PageId) => void;
  }

  let { pages, current, onselect }: Props = $props();

  const perfMode = $derived(store.state?.[store.live === "Ac" ? "ac_profile" : "battery_profile"].perf_mode ?? "Unknown");
  const perfLabel = $derived(store.state?.meta.perf_mode_labels[perfMode] ?? "—");
  const perfColor = $derived(store.state?.meta.perf_mode_colors[perfMode] ?? "var(--fg-faint)");
</script>

<nav aria-label="Sections">
  <ul>
    {#each pages as page (page.id)}
      {#if !page.experimental || store.state?.advanced_experimental_features}
        <li class:separated={page.experimental}>
          <button
            type="button"
            class:current={page.id === current}
            aria-current={page.id === current ? "page" : undefined}
            onclick={() => onselect(page.id)}
          >
            {page.label}
          </button>
        </li>
      {/if}
    {/each}
  </ul>

  <!-- What the machine is doing right now, kept visible while content scrolls. -->
  <div class="device">
    <span class="model">{store.state?.model_name || "Blade Laptop"}</span>
    <span class="status">
      <span class="dot" style="background: {perfColor}"></span>
      {store.live === "Ac" ? "AC power" : "Battery"} · {perfLabel}
    </span>
  </div>
</nav>

<style>
  nav {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    width: 200px;
    flex: 0 0 auto;
    padding: var(--gap);
    background: var(--bg-sunken);
    border-right: 1px solid var(--line);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1 1 auto;
  }

  li.separated {
    margin-top: var(--gap-sm);
    padding-top: var(--gap-sm);
    border-top: 1px solid var(--line);
  }

  button {
    width: 100%;
    text-align: left;
    border-color: transparent;
    background: transparent;
    color: var(--fg-muted);
    padding: 7px 10px;
    border-radius: var(--radius-sm);
  }

  button.current {
    background: var(--bg-hover);
    color: var(--fg);
    font-weight: 600;
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .device {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--gap-sm) 10px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
  }

  .model {
    font-size: 12.5px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--fg-muted);
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: 0 0 auto;
  }
</style>
