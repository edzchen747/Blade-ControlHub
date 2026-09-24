<script lang="ts">
  import ChipGrid from "../components/ChipGrid.svelte";
  import Section from "../components/Section.svelte";
  import Segmented from "../components/Segmented.svelte";
  import Slider from "../components/Slider.svelte";
  import LevelDots from "../components/LevelDots.svelte";
  import { SLIDER_COMMIT_MS, debounce } from "../debounce";
  import * as ipc from "../ipc";
  import { store } from "../store.svelte";
  import { FAN_AUTO, FAN_SPEED_FIELDS, type PerfMode, type RGBEffect } from "../types";

  const state = $derived(store.state);
  const profile = $derived(store.profile);

  const perfChips = $derived(
    store.perfModes.map(({ mode, supported }) => ({
      value: mode,
      label: state?.meta.perf_mode_labels[mode] ?? mode,
      unsupported: !supported,
      title: supported
        ? undefined
        : `${state?.meta.perf_mode_labels[mode] ?? mode} is not supported on this device`,
    })),
  );

  const perfColor = $derived(
    profile ? (state?.meta.perf_mode_colors[profile.perf_mode] ?? "var(--fg-faint)") : "var(--fg-faint)",
  );

  // Custom CPU/GPU tuning is a firmware feature of the AC profile only.
  const showCustomLevels = $derived(
    store.editing === "Ac" && profile?.perf_mode === "Custom",
  );

  const limits = $derived(state?.fan_speed_limits ?? { min: 10, max: 46 });
  const fanSpeed = $derived(store.fanSpeed);
  const fanIsManual = $derived(fanSpeed !== FAN_AUTO);

  const brightnessStep = $derived(state?.meta.keyboard_brightness_step ?? 51);
  // The firmware takes 0..=255 in steps of 51: six states, off through full.
  const brightnessSteps = $derived(Math.round(255 / brightnessStep));
  const brightnessLevel = $derived(
    profile ? Math.round(profile.keyboard_brightness / brightnessStep) : 0,
  );
  const brightnessReadout = $derived(
    brightnessLevel === 0 ? "Off" : `${(brightnessLevel / brightnessSteps) * 100}%`,
  );

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
    const cpu = which === "cpu" ? level : (state?.custom_mode_config.cpu_level ?? 0);
    const gpu = which === "gpu" ? level : (state?.custom_mode_config.gpu_level ?? 0);
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

{#if profile && state}
  <Section title="Performance">
    {#snippet trailing()}
      <span class="mode-dot" style="background: {perfColor}"></span>
      <span class="muted">{state.meta.perf_mode_labels[profile.perf_mode]}</span>
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
          value={state.custom_mode_config.cpu_level}
          readout={state.meta.custom_mode_levels[state.custom_mode_config.cpu_level]}
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
          value={state.custom_mode_config.gpu_level}
          readout={state.meta.custom_mode_levels[state.custom_mode_config.gpu_level]}
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
          <option value={effect}>{state.meta.rgb_effect_labels[effect] ?? effect}</option>
        {/each}
      </select>
    </div>
    {#if store.errors["rgb-effect"] || store.errors["keyboard-brightness"]}
      <span class="field-error">
        {store.errors["rgb-effect"] ?? store.errors["keyboard-brightness"]}
      </span>
    {/if}
  </Section>
{/if}

<style>
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
