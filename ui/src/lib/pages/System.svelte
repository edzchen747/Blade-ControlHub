<script lang="ts">
  import AccentPicker from "../components/AccentPicker.svelte";
  import Section from "../components/Section.svelte";
  import Segmented from "../components/Segmented.svelte";
  import Slider from "../components/Slider.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { SLIDER_COMMIT_MS, debounce } from "../debounce";
  import * as ipc from "../ipc";
  import { store } from "../store.svelte";
  import type { BatteryLimit } from "../types";

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

  <Section
    title="Appearance"
    hint="Used for the tray icon, the OSD, this window and the Static and Reactive keyboard effects."
  >
    <AccentPicker />
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
