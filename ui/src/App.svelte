<script lang="ts">
  import { onMount } from "svelte";

  import ProfileBar from "./lib/components/ProfileBar.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import { hotkeys } from "./lib/hotkeys.svelte";
  import { keyMap } from "./lib/keymap.svelte";
  import { PAGES, type PageId } from "./lib/pages";
  import CommandLab from "./lib/pages/CommandLab.svelte";
  import Dashboard from "./lib/pages/Dashboard.svelte";
  import Keys from "./lib/pages/Keys.svelte";
  import Lighting from "./lib/pages/Lighting.svelte";
  import System from "./lib/pages/System.svelte";
  import { store } from "./lib/store.svelte";
  import type { ThemeColor } from "./lib/types";

  /**
   * Pages whose controls belong to one power profile. System, Keys and Command
   * Lab hold global settings, so showing a profile switcher on them would imply
   * a per-profile behaviour that does not exist.
   */
  const PROFILE_SCOPED_PAGES: ReadonlySet<PageId> = new Set<PageId>([
    "dashboard",
    "lighting",
  ]);

  let page = $state<PageId>("dashboard");

  onMount(() => {
    void store.start();
    void keyMap.start();
    // The runtime's keyboard hook is not called for keys aimed at this window,
    // so the window feeds them back to the runtime itself.
    void hotkeys.start();

    // Esc closes the window, matching how a tray panel is expected to behave.
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      // Esc belongs to whatever is waiting for a key press on the Keys page.
      if (keyMap.capturing) return;
      void import("./lib/ipc").then((ipc) => ipc.hideWindow());
    };
    window.addEventListener("keydown", onKeyDown);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      store.stop();
      keyMap.stop();
      hotkeys.stop();
    };
  });

  // The accent is the one user-controlled colour; everything else derives from
  // the neutral ramp, so applying it is a two-token write.
  $effect(() => {
    const color = store.state?.theme_color;
    if (!color) return;
    const root = document.documentElement;
    root.style.setProperty("--accent", `rgb(${color.r} ${color.g} ${color.b})`);
    root.style.setProperty("--accent-fg", readableOn(color));
  });

  // Mirrors `theme_text_color` in the runtime, so the window, tray and OSD
  // agree on when the accent needs dark text.
  function readableOn({ r, g, b }: ThemeColor): string {
    const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    return luminance > 145 ? "#000000" : "#ffffff";
  }

  // The mapping tables arrive with every other setting, so they are seeded
  // from the same snapshot rather than fetched separately.
  $effect(() => {
    const bindings = store.state?.key_bindings;
    if (bindings) keyMap.seed(bindings);
  });

  // Command Lab is gated; if the flag is turned off while it is open, fall back.
  $effect(() => {
    if (page === "command-lab" && store.state && !store.state.advanced_experimental_features) {
      page = "dashboard";
    }
  });
</script>

<div class="shell">
  <Sidebar pages={PAGES} current={page} onselect={(next) => (page = next)} />

  <main>
    {#if store.loaded}
      {#if PROFILE_SCOPED_PAGES.has(page)}
        <ProfileBar />
      {/if}

      <div class="content" class:inactive={PROFILE_SCOPED_PAGES.has(page) && !store.editingLive}>
        {#if page === "dashboard"}
          <Dashboard />
        {:else if page === "lighting"}
          <Lighting />
        {:else if page === "keys"}
          <Keys />
        {:else if page === "system"}
          <System />
        {:else if page === "command-lab"}
          <CommandLab />
        {/if}
      </div>
    {:else}
      <div class="loading">
        <span class="spinner" aria-hidden="true"></span>
        <span class="muted">Reading device state…</span>
      </div>
    {/if}
  </main>
</div>

<style>
  .shell {
    display: flex;
    height: 100%;
  }

  main {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .content {
    flex: 1 1 auto;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: var(--gap-lg);
    display: flex;
    flex-direction: column;
    gap: var(--gap);
  }

  /* Editing a profile that is not running gets a standing visual cue, not
     just a line of text that scrolls away. */
  .content.inactive {
    border-left: 2px solid color-mix(in srgb, var(--warn) 60%, transparent);
    padding-left: calc(var(--gap-lg) - 2px);
  }

  .loading {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--gap);
  }

  .spinner {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    border-top-color: var(--accent);
    animation: spin 700ms linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
