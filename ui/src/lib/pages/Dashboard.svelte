<script lang="ts">
  import ChipGrid from "../components/ChipGrid.svelte";
  import Section from "../components/Section.svelte";
  import Segmented from "../components/Segmented.svelte";
  import Slider from "../components/Slider.svelte";
  import LevelDots from "../components/LevelDots.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { SLIDER_COMMIT_MS, debounce } from "../debounce";
  import * as ipc from "../ipc";
  import { store } from "../store.svelte";
  import { FAN_AUTO, FAN_SPEED_FIELDS, type PerfMode, type RGBEffect } from "../types";
  import {
    NOTHING_HIDDEN,
    dashboardItems,
    itemKey,
    itemSource,
    withItemShown,
    type DashboardItem,
  } from "../dashboard";

  // Named `ui` rather than `state`, which would shadow the `$state` rune.
  const ui = $derived(store.state);
  const profile = $derived(store.profile);

  const perfChips = $derived(
    store.perfModes.map(({ mode, supported }) => ({
      value: mode,
      label: ui?.meta.perf_mode_labels[mode] ?? mode,
      unsupported: !supported,
      title: supported
        ? undefined
        : `${ui?.meta.perf_mode_labels[mode] ?? mode} is not supported on this device`,
    })),
  );

  const perfColor = $derived(
    profile ? (ui?.meta.perf_mode_colors[profile.perf_mode] ?? "var(--fg-faint)") : "var(--fg-faint)",
  );

  // Custom CPU/GPU tuning is a firmware feature of the AC profile only.
  const showCustomLevels = $derived(
    store.editing === "Ac" && profile?.perf_mode === "Custom",
  );

  const limits = $derived(ui?.fan_speed_limits ?? { min: 10, max: 46 });
  const fanSpeed = $derived(store.fanSpeed);
  const fanIsManual = $derived(fanSpeed !== FAN_AUTO);

  const brightnessStep = $derived(ui?.meta.keyboard_brightness_step ?? 51);
  // The firmware takes 0..=255 in steps of 51: six states, off through full.
  const brightnessSteps = $derived(Math.round(255 / brightnessStep));
  const brightnessLevel = $derived(
    profile ? Math.round(profile.keyboard_brightness / brightnessStep) : 0,
  );
  const brightnessReadout = $derived(
    brightnessLevel === 0 ? "Off" : `${(brightnessLevel / brightnessSteps) * 100}%`,
  );

  /** While on, the section lists everything and each line can be hidden. */
  let editingControls = $state(false);

  const hidden = $derived(ui?.hidden_dashboard_controls ?? NOTHING_HIDDEN);

  const items = $derived(
    dashboardItems(
      ui?.command_lab_commands ?? {},
      ui?.custom_toggles ?? [],
      hidden,
      store.editing,
    ),
  );
  const visibleItems = $derived(items.filter((item) => !item.hidden));
  const shownItems = $derived(editingControls ? items : visibleItems);

  // A control's side belongs to the profile being edited, like every other
  // control on this page: flipping one while the other profile is live records
  // it for later rather than acting on the machine now.
  function flipCustomControl(name: string, enabled: boolean) {
    const target = store.editing;
    store.run(
      `custom-control:${name}`,
      (next) => {
        const toggle = next.custom_toggles.find((candidate) => candidate.name === name);
        if (!toggle) return;
        if (target === "Ac") toggle.ac_enabled = enabled;
        else toggle.battery_enabled = enabled;
      },
      () => ipc.setCustomToggle(target, name, enabled),
    );
  }

  function replayCapture(name: string) {
    const commands = ui?.command_lab_commands[name];
    if (commands) void ipc.playCommandLabCommands($state.snapshot(commands));
  }

  function setItemShown(item: DashboardItem, shown: boolean) {
    const next = withItemShown(hidden, item, shown);
    if (!next) return;

    store.run(
      "dashboard-controls",
      (snapshot) => (snapshot.hidden_dashboard_controls = next),
      () => ipc.setHiddenDashboardControls(next),
    );
  }

  function setPerfMode(mode: PerfMode) {
    const target = store.editing;
    store.run(
      "perf-mode",
      (next) => {
        next[target === "Ac" ? "ac_profile" : "battery_profile"].perf_mode = mode;
      },
      () => ipc.setPerfMode(target, mode),
    );
  }

  function setFanSpeed(speed: number) {
    const target = store.editing;
    const mode = profile?.perf_mode;
    store.run(
      "fan-speed",
      (next) => {
        const profileState = next[target === "Ac" ? "ac_profile" : "battery_profile"];
        const field = mode ? FAN_SPEED_FIELDS[mode] : null;
        if (field) profileState.fan_speeds[field] = speed;
      },
      () => ipc.setFanSpeed(target, speed),
    );
  }

  const commitFanSpeed = debounce(setFanSpeed, SLIDER_COMMIT_MS);

  function setRefreshRate(hz: number) {
    const target = store.editing;
    store.run(
      "refresh-rate",
      (next) => {
        next[target === "Ac" ? "ac_profile" : "battery_profile"].refresh_rate = hz;
      },
      () => ipc.setRefreshRate(target, hz),
    );
  }

  function setCustomLevel(which: "cpu" | "gpu", level: number) {
    const cpu = which === "cpu" ? level : (ui?.custom_mode_config.cpu_level ?? 0);
    const gpu = which === "gpu" ? level : (ui?.custom_mode_config.gpu_level ?? 0);
    store.run(
      "custom-mode",
      (next) => {
        next.custom_mode_config.cpu_level = cpu;
        next.custom_mode_config.gpu_level = gpu;
      },
      () => ipc.setCustomModeConfig(cpu, gpu),
    );
  }

  const commitCustomLevel = debounce(setCustomLevel, SLIDER_COMMIT_MS);

  function setBrightnessLevel(level: number) {
    const target = store.editing;
    const raw = Math.min(255, level * brightnessStep);
    store.run(
      "keyboard-brightness",
      (next) => {
        next[target === "Ac" ? "ac_profile" : "battery_profile"].keyboard_brightness = raw;
      },
      () => ipc.setKeyboardBrightness(target, raw),
    );
  }

  function setEffect(effect: RGBEffect) {
    const target = store.editing;
    store.run(
      "rgb-effect",
      (next) => {
        next[target === "Ac" ? "ac_profile" : "battery_profile"].rgb_effect = effect;
      },
      () => ipc.setRgbEffect(target, effect),
    );
  }
</script>

{#if profile && ui}
  <Section title="Performance">
    {#snippet trailing()}
      <span class="mode-dot" style="background: {perfColor}"></span>
      <span class="muted">{ui.meta.perf_mode_labels[profile.perf_mode]}</span>
    {/snippet}

    <ChipGrid
      chips={perfChips}
      value={profile.perf_mode}
      ariaLabel="Performance mode"
      onselect={setPerfMode}
    />
    {#if store.errors["perf-mode"]}
      <span class="field-error">{store.errors["perf-mode"]}</span>
    {/if}

    {#if showCustomLevels}
      <div class="stack-sm">
        <Slider
          label="CPU"
          min={0}
          max={3}
          value={ui.custom_mode_config.cpu_level}
          readout={ui.meta.custom_mode_levels[ui.custom_mode_config.cpu_level]}
          ariaLabel="Custom mode CPU level"
          oninput={(level) => commitCustomLevel("cpu", level)}
          oncommit={(level) => {
            commitCustomLevel.cancel();
            setCustomLevel("cpu", level);
          }}
        />
        <Slider
          label="GPU"
          min={0}
          max={3}
          value={ui.custom_mode_config.gpu_level}
          readout={ui.meta.custom_mode_levels[ui.custom_mode_config.gpu_level]}
          ariaLabel="Custom mode GPU level"
          oninput={(level) => commitCustomLevel("gpu", level)}
          oncommit={(level) => {
            commitCustomLevel.cancel();
            setCustomLevel("gpu", level);
          }}
        />
      </div>
    {/if}
  </Section>

  <Section
    title="Cooling"
    hint="Stored per performance mode, so each mode keeps its own fan setting."
  >
    <div class="row-between">
      <Segmented
        options={[
          { value: "auto", label: "Auto" },
          { value: "manual", label: "Manual" },
        ]}
        value={fanIsManual ? "manual" : "auto"}
        ariaLabel="Fan control"
        onselect={(choice) =>
          setFanSpeed(choice === "auto" ? FAN_AUTO : limits.min + Math.floor((limits.max - limits.min) / 2))}
      />
      {#if !fanIsManual}
        <span class="muted">Firmware controlled</span>
      {/if}
    </div>

    {#if fanIsManual}
      <Slider
        min={limits.min}
        max={limits.max}
        value={fanSpeed}
        ariaLabel="Fan speed"
        oninput={commitFanSpeed}
        oncommit={(speed) => {
          commitFanSpeed.cancel();
          setFanSpeed(speed);
        }}
      />
    {/if}
    {#if store.errors["fan-speed"]}
      <span class="field-error">{store.errors["fan-speed"]}</span>
    {/if}
  </Section>

  <Section title="Display">
    {#if profile.supported_refresh_rates.length === 0}
      <p class="hint">No refresh rates were reported for the built-in display.</p>
    {:else}
      <ChipGrid
        chips={profile.supported_refresh_rates.map((hz) => ({ value: hz, label: `${hz} Hz` }))}
        value={profile.refresh_rate}
        ariaLabel="Refresh rate"
        minWidth={88}
        onselect={setRefreshRate}
      />
    {/if}
    {#if store.errors["refresh-rate"]}
      <span class="field-error">{store.errors["refresh-rate"]}</span>
    {/if}
  </Section>

  <Section title="Keyboard">
    <LevelDots
      label="Backlight"
      steps={brightnessSteps}
      value={brightnessLevel}
      readout={brightnessReadout}
      ariaLabel="Keyboard backlight"
      onselect={setBrightnessLevel}
    />
    <div class="row-between">
      <span>Effect</span>
      <select
        class="input select"
        value={profile.rgb_effect}
        onchange={(event) => setEffect(event.currentTarget.value as RGBEffect)}
      >
        {#each profile.rgb_effects as effect (effect)}
          <option value={effect}>{ui.meta.rgb_effect_labels[effect] ?? effect}</option>
        {/each}
      </select>
    </div>
    {#if store.errors["rgb-effect"] || store.errors["keyboard-brightness"]}
      <span class="field-error">
        {store.errors["rgb-effect"] ?? store.errors["keyboard-brightness"]}
      </span>
    {/if}
  </Section>

  <!-- Only once the user has recorded or built something: an empty section
       would advertise a Command Lab feature to someone who has never opened
       the page. -->
  {#if items.length > 0}
    <Section title="Custom Controls" hint="Your Command Lab captures and controls.">
      {#snippet trailing()}
        <button type="button" class="ghost" onclick={() => (editingControls = !editingControls)}>
          {editingControls ? "Done" : "Edit"}
        </button>
      {/snippet}

      {#if !editingControls && visibleItems.length === 0}
        <p class="hint">Everything is hidden. Use Edit to choose what to show.</p>
      {/if}

      <!-- Editing adds a checkbox and says where the line came from. It changes
           nothing else: every row keeps the control it has the rest of the
           time, so the section does not rearrange itself under the user. -->
      {#each shownItems as item (itemKey(item))}
        <div class="control-line" class:dimmed={item.hidden}>
          {#if editingControls}
            <input
              class="show"
              type="checkbox"
              checked={!item.hidden}
              aria-label={`Show ${item.name} on the Dashboard`}
              onchange={(event) => setItemShown(item, event.currentTarget.checked)}
            />
          {/if}

          {#if item.kind === "control"}
            <Toggle
              label={item.name}
              suffix={editingControls ? itemSource(item) : undefined}
              checked={item.enabled}
              error={store.errors[`custom-control:${item.name}`]}
              onchange={(checked) => flipCustomControl(item.name, checked)}
            />
          {:else}
            <!-- A capture has no state to show, so it is a button rather than
                 a switch: pressing it replays the commands once. -->
            <div class="row-between">
              <span class="capture">
                {item.name}
                <!-- Two lines can carry the same name, so while editing this is
                     the only thing telling them apart. -->
                {#if editingControls}<span class="source">{itemSource(item)}</span>{/if}
              </span>
              <button type="button" onclick={() => replayCapture(item.name)}>Replay</button>
            </div>
          {/if}
        </div>
      {/each}

      {#if store.errors["dashboard-controls"]}
        <span class="field-error">{store.errors["dashboard-controls"]}</span>
      {/if}
    </Section>
  {/if}
{/if}

<style>
  /* The row's own control takes the whole width, as it does with no checkbox
     beside it: editing must not move anything the user was just looking at. */
  .control-line {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
  }

  .control-line > :global(:not(.show)) {
    flex: 1 1 auto;
    min-width: 0;
  }

  .show {
    flex: 0 0 auto;
    cursor: pointer;
  }

  /* A hidden line is still listed while editing, so it has to read as off
     without being mistaken for a disabled one. */
  .control-line.dimmed {
    opacity: 0.55;
  }

  .capture .source {
    margin-left: var(--gap-xs);
    color: var(--fg-muted);
    font-size: 12.5px;
  }

  .mode-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }

  .select {
    width: auto;
    min-width: 170px;
  }
</style>
