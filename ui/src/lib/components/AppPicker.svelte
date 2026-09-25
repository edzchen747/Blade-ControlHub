<script lang="ts">
  import * as ipc from "../ipc";
  import type { IndexedApp } from "../types";
  import Modal from "./Modal.svelte";

  interface Props {
    onpick: (app: { path: string; name: string }) => void;
    onclose: () => void;
  }

  let { onpick, onclose }: Props = $props();

  let apps = $state<IndexedApp[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let query = $state("");

  const matches = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    if (needle === "") return apps;
    return apps.filter(
      (app) =>
        app.name.toLowerCase().includes(needle) ||
        (!app.packaged && app.target.toLowerCase().includes(needle)),
    );
  });

  $effect(() => {
    void load(ipc.listApps);
  });

  async function load(source: () => Promise<IndexedApp[]>) {
    loading = true;
    error = null;
    try {
      apps = await source();
    } catch (cause) {
      error = ipc.errorMessage(cause);
    } finally {
      loading = false;
    }
  }

  /**
   * The system picker is the way out of the Start menu: portable applications
   * and anything installed without a shortcut are only reachable by path.
   */
  async function browse() {
    try {
      const path = await ipc.pickExecutable();
      if (path) onpick({ path, name: fileStem(path) });
    } catch (cause) {
      error = ipc.errorMessage(cause);
    }
  }

  function fileStem(path: string): string {
    const file = path.split(/[\\/]/).pop() ?? path;
    return file.replace(/\.[^.]+$/, "");
  }
</script>

<Modal
  title="Choose an application"
  hint="Your Start menu and installed Windows apps, plus anything else you can point at."
  {onclose}
>
  <div class="row-between">
    <input
      class="input"
      placeholder="Search applications"
      bind:value={query}
      aria-label="Search applications"
    />
    <button type="button" onclick={browse}>Browse…</button>
  </div>

  {#if error}
    <span class="field-error">{error}</span>
  {/if}

  {#if loading}
    <span class="hint">Reading the Start menu…</span>
  {:else if matches.length === 0}
    <span class="hint">
      {apps.length === 0 ? "No applications were found." : "Nothing matches that search."}
    </span>
  {:else}
    <ul class="results">
      {#each matches as app (app.path)}
        <li>
          <button type="button" class="entry" onclick={() => onpick({ path: app.path, name: app.name })}>
            {#if app.icon}
              <img src={app.icon} alt="" width="20" height="20" />
            {:else}
              <span class="no-icon" aria-hidden="true"></span>
            {/if}
            <span class="text">
              <span class="name">{app.name}</span>
              <!-- A packaged application's model ID is not worth reading. -->
              <span class="target">{app.packaged ? "Windows app" : app.target}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="row-between">
    <span class="hint">{apps.length} application{apps.length === 1 ? "" : "s"}</span>
    <button type="button" class="ghost" disabled={loading} onclick={() => load(ipc.refreshApps)}>
      Rescan
    </button>
  </div>
</Modal>

<style>
  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 320px;
    overflow-y: auto;
    overscroll-behavior: contain;
  }

  .entry {
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr);
    align-items: center;
    gap: var(--gap-sm);
    width: 100%;
    text-align: left;
    border-color: transparent;
    background: transparent;
    padding: 5px 8px;
  }

  .no-icon {
    width: 20px;
    height: 20px;
    border-radius: var(--radius-sm);
    background: var(--bg-sunken);
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .target {
    font-size: 11.5px;
    color: var(--fg-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
</style>
