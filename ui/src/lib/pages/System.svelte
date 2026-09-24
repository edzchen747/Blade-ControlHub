<script lang="ts">
  import Section from "../components/Section.svelte";
  import Segmented from "../components/Segmented.svelte";
  import Slider from "../components/Slider.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { COLOR_COMMIT_MS, SLIDER_COMMIT_MS, debounce } from "../debounce";
  import * as ipc from "../ipc";
  import { store } from "../store.svelte";
  import type { BatteryLimit, ThemeColor } from "../types";

  /** Matches `ThemeColor::default()` in the runtime. */
  const DEFAULT_ACCENT: ThemeColor = { r: 0xff, g: 0xd7, b: 0x00 };

  const state = $derived(store.state);

  const limitIndex = $derived(
    state ? Math.max(0, state.battery_limits.indexOf(state.battery_limit)) : 0,
  );
  // Bare numbers: the "%" lives in the readout, and eight labels with a suffix
  // each do not fit across the track.
  const limitTicks = $derived(
    state?.battery_limits.map((limit) => {
      const percent = state.meta.battery_limit_percents[limit];
      return percent === null || percent === undefined ? "Off" : `${percent}`;
    }) ?? [],
  );
  const limitPercent = $derived(
    state ? state.meta.battery_limit_percents[state.battery_limit] : null,
  );

  const accentHex = $derived(state ? toHex(state.theme_color) : "#ffd700");

  function toHex({ r, g, b }: ThemeColor): string {
    const part = (value: number) => value.toString(16).padStart(2, "0");
    return `#${part(r)}${part(g)}${part(b)}`;
  }

  function fromHex(hex: string): ThemeColor {
    return {
      r: parseInt(hex.slice(1, 3), 16),
      g: parseInt(hex.slice(3, 5), 16),
      b: parseInt(hex.slice(5, 7), 16),
    };
  }

  function setBatteryLimit(index: number) {
    const limit = state?.battery_limits[index];
    if (!limit) return;
    store.run(
      "battery-limit",
      (next) => {
        next.battery_limit = limit as BatteryLimit;
      },
      () => ipc.setBatteryLimit(limit),
    );
  }

  // Each step is two blocking HID writes, so dragging across the track must not
  // queue one per stop; the write waits for the drag to settle.
  const commitBatteryLimit = debounce(setBatteryLimit, SLIDER_COMMIT_MS);

  /** Moves the control locally at once, and writes once the drag settles. */
  function previewBatteryLimit(index: number) {
    const limit = state?.battery_limits[index];
    if (!limit) return;
    store.preview("battery-limit", (next) => {
      next.battery_limit = limit as BatteryLimit;
    });
    commitBatteryLimit(index);
  }

  function setPrimaryMultimediaKeys(enabled: boolean) {
    store.run(
      "function-keys",
      (next) => {
        next.primary_multimedia_keys = enabled;
      },
      () => ipc.setPrimaryMultimediaKeys(enabled),
    );
  }

  // The picker fires continuously while dragging, and each commit is a HID
  // write, so the colour previews locally and only the write is debounced.
  const commitAccent = debounce(
    (color: ThemeColor) =>
      store.run(
        "accent",
        (next) => {
          next.theme_color = color;
        },
        () => ipc.setThemeColor(color),
      ),
    COLOR_COMMIT_MS,
  );

  function previewAccent(hex: string) {
    const color = fromHex(hex);
    store.preview("accent", (next) => {
      next.theme_color = color;
    });
    commitAccent(color);
  }

  function resetAccent() {
    commitAccent.cancel();
    store.preview("accent", (next) => {
      next.theme_color = DEFAULT_ACCENT;
    });
    commitAccent(DEFAULT_ACCENT);
    commitAccent.flush();
  }
</script>

{#if state}
  <Section title="Battery">
    <Slider
      label="Charge limit"
      min={0}
      max={Math.max(0, state.battery_limits.length - 1)}
      value={limitIndex}
      readout={state.meta.battery_limit_labels[state.battery_limit] ?? ""}
      ticks={limitTicks}
      ariaLabel="Battery charge limit"
      oninput={previewBatteryLimit}
      oncommit={(index) => {
        commitBatteryLimit.cancel();
        setBatteryLimit(index);
      }}
    />
    <p class="hint">
      {#if limitPercent}
        Stops charging at {limitPercent}% to reduce long-term battery wear.
      {:else}
        Charges to 100%. Set a limit if the laptop is usually on mains power.
      {/if}
    </p>
    {#if store.errors["battery-limit"]}
      <span class="field-error">{store.errors["battery-limit"]}</span>
    {/if}
  </Section>

  <Section title="Function keys" hint="Hold Fn for the other behaviour.">
    <div class="row-between">
      <span>Top row sends</span>
      <Segmented
        options={[
          { value: "function", label: "F1–F12" },
          { value: "media", label: "Media keys" },
        ]}
        value={state.primary_multimedia_keys ? "media" : "function"}
        ariaLabel="Primary function key behaviour"
        onselect={(choice) => setPrimaryMultimediaKeys(choice === "media")}
      />
    </div>
    {#if store.errors["function-keys"]}
      <span class="field-error">{store.errors["function-keys"]}</span>
    {/if}
  </Section>

  <Section title="Appearance" hint="Used for the tray icon, the OSD and this window.">
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
    {#if store.errors["accent"]}
      <span class="field-error">{store.errors["accent"]}</span>
    {/if}
  </Section>

  <Section title="Startup">
    <Toggle
      label="Start with Windows"
      hint="Runs ControlHub in the tray when you sign in."
      checked={state.start_with_windows}
      error={store.errors["start-with-windows"]}
      onchange={(enabled) =>
        store.run(
          "start-with-windows",
          (next) => {
            next.start_with_windows = enabled;
          },
          () => ipc.setStartWithWindows(enabled),
        )}
    />
    <Toggle
      label="Start as administrator"
      hint="Avoids repeated UAC prompts. Turning this on restarts the app."
      checked={state.start_with_admin}
      error={store.errors["start-with-admin"]}
      onchange={(enabled) =>
        store.run(
          "start-with-admin",
          (next) => {
            next.start_with_admin = enabled;
          },
          () => ipc.setStartWithAdmin(enabled),
        )}
    />
  </Section>

  <Section title="Advanced">
    <Toggle
      label="Experimental features"
      hint="May not work on every device. Adds the Command Lab page."
      checked={state.advanced_experimental_features}
      error={store.errors["experimental"]}
      onchange={(enabled) =>
        store.run(
          "experimental",
          (next) => {
            next.advanced_experimental_features = enabled;
          },
          () => ipc.setAdvancedExperimentalFeatures(enabled),
        )}
    />
  </Section>

  <Section title="Application">
    <div class="row">
      <button type="button" onclick={() => ipc.closeGpuApps()}>
        Close apps running on dGPU
      </button>
      <button type="button" onclick={() => ipc.restartApp()}>Restart</button>
      <button type="button" class="danger" onclick={() => ipc.quitApp()}>Quit</button>
    </div>
  </Section>
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
