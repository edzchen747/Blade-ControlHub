<script lang="ts">
  import EffectTile from "../components/EffectTile.svelte";
  import Section from "../components/Section.svelte";
  import LevelDots from "../components/LevelDots.svelte";
  import Toggle from "../components/Toggle.svelte";
  import * as ipc from "../ipc";
  import { store } from "../store.svelte";
  import type { RGBEffect } from "../types";

  const state = $derived(store.state);
  const profile = $derived(store.profile);

  const brightnessStep = $derived(state?.meta.keyboard_brightness_step ?? 51);
  // The firmware takes 0..=255 in steps of 51: six states, off through full.
  const brightnessSteps = $derived(Math.round(255 / brightnessStep));
  const brightnessLevel = $derived(
    profile ? Math.round(profile.keyboard_brightness / brightnessStep) : 0,
  );
  const brightnessReadout = $derived(
    brightnessLevel === 0 ? "Off" : `${(brightnessLevel / brightnessSteps) * 100}%`,
  );

  function key() {
    return store.editing === "Ac" ? ("ac_profile" as const) : ("battery_profile" as const);
  }

  function setEffect(effect: RGBEffect) {
    const target = store.editing;
    const slot = key();
    store.run(
      "rgb-effect",
      (next) => {
        next[slot].rgb_effect = effect;
      },
      () => ipc.setRgbEffect(target, effect),
    );
  }

  function setBrightnessLevel(level: number) {
    const target = store.editing;
    const slot = key();
    const raw = Math.min(255, level * brightnessStep);
    store.run(
      "keyboard-brightness",
      (next) => {
        next[slot].keyboard_brightness = raw;
      },
      () => ipc.setKeyboardBrightness(target, raw),
    );
  }

  function setUnderGlow(enabled: boolean) {
    const target = store.editing;
    const slot = key();
    store.run(
      "under-glow",
      (next) => {
        next[slot].underglow_enabled = enabled;
      },
      () => ipc.setUnderGlow(target, enabled),
    );
  }
</script>

{#if profile && state}
  <Section title="Keyboard effect">
    <div class="tiles" role="radiogroup" aria-label="Keyboard effect">
      {#each profile.rgb_effects as effect (effect)}
        <EffectTile
          {effect}
          label={state.meta.rgb_effect_labels[effect] ?? effect}
          selected={effect === profile.rgb_effect}
          onselect={() => setEffect(effect)}
        />
      {/each}
    </div>
    {#if store.errors["rgb-effect"]}
      <span class="field-error">{store.errors["rgb-effect"]}</span>
    {/if}
  </Section>

  <Section title="Brightness">
    <LevelDots
      label="Keyboard"
      steps={brightnessSteps}
      value={brightnessLevel}
      readout={brightnessReadout}
      ariaLabel="Keyboard backlight"
      onselect={setBrightnessLevel}
    />
    {#if store.errors["keyboard-brightness"]}
      <span class="field-error">{store.errors["keyboard-brightness"]}</span>
    {/if}

    <Toggle
      label="Vapour chamber light"
      hint="The illuminated vent under the chassis."
      checked={profile.underglow_enabled}
      error={store.errors["under-glow"]}
      onchange={setUnderGlow}
    />
  </Section>
{/if}

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(110px, 1fr));
    gap: var(--gap-sm);
  }
</style>
