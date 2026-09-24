<script lang="ts">
  import type { RGBEffect } from "../types";

  interface Props {
    effect: RGBEffect;
    label: string;
    selected: boolean;
    onselect: () => void;
  }

  let { effect, label, selected, onselect }: Props = $props();

  /**
   * Scattered twinkle positions for Starlight. Fixed rather than generated so
   * the tile does not reshuffle on every render; the irregular spacing and the
   * staggered delays are what read as random.
   */
  const STARS = [
    { x: 11, y: 30, delay: 0.0, duration: 1.7 },
    { x: 24, y: 68, delay: 0.9, duration: 2.3 },
    { x: 33, y: 18, delay: 1.6, duration: 1.9 },
    { x: 46, y: 52, delay: 0.4, duration: 2.6 },
    { x: 57, y: 79, delay: 2.1, duration: 1.5 },
    { x: 68, y: 26, delay: 1.2, duration: 2.1 },
    { x: 79, y: 61, delay: 0.6, duration: 1.8 },
    { x: 90, y: 36, delay: 1.9, duration: 2.4 },
  ];
</script>

<!-- The user is picking an appearance, so each tile shows one. A dropdown of
     eight visual choices, as the old UI had, hides exactly the information that
     makes the choice. Each preview mirrors what the effect actually does on the
     keyboard, not just "some motion". -->
<button
  type="button"
  role="radio"
  aria-checked={selected}
  class:selected
  onclick={onselect}
>
  <span class="preview preview-{effect.toLowerCase()}" aria-hidden="true">
    {#if effect === "Starlight"}
      {#each STARS as star (star.x)}
        <i
          class="star"
          style="left: {star.x}%; top: {star.y}%; animation-delay: {star.delay}s; animation-duration: {star.duration}s"
        ></i>
      {/each}
    {:else if effect === "AudioBloom"}
      <!-- Two ribbons at different speeds and phases, the way a Siri-style
           waveform reads: one bright band with a softer one trailing it. -->
      <svg viewBox="0 0 100 30" preserveAspectRatio="none">
        <path
          class="ribbon back"
          d="M-50,15 Q-37.5,3 -25,15 T0,15 T25,15 T50,15 T75,15 T100,15 T125,15 T150,15"
        />
        <path
          class="ribbon front"
          d="M-50,15 Q-37.5,3 -25,15 T0,15 T25,15 T50,15 T75,15 T100,15 T125,15 T150,15"
        />
      </svg>
    {/if}
  </span>
  <span class="label">{label}</span>
</button>

<style>
  button {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 9px;
    align-items: stretch;
  }

  button.selected {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .label {
    font-size: 12.5px;
    text-align: center;
  }

  .preview {
    position: relative;
    display: block;
    height: 30px;
    border-radius: var(--radius-sm);
    background: var(--bg-sunken);
    overflow: hidden;
  }

  /* Static: one colour, no motion. */
  .preview-static {
    background: var(--accent);
  }

  /* Cycle: the whole keyboard is one colour at a time, walking the rainbow. */
  .preview-cycle {
    animation: cycle-hue 6s linear infinite;
  }

  /* Wave: a rainbow spread across the keyboard, travelling along it. Same
     gradient as a cycle would show at rest, but it moves and the colours
     travel in the opposite direction to the old preview. */
  .preview-wave {
    background: linear-gradient(90deg, #ff4d4d, #ffb020, #ffe14d, #4dff88, #4dc8ff, #b44dff, #ff4d4d);
    background-size: 200% 100%;
    animation: wave-travel 3s linear infinite;
  }

  /* Breathe: fades in and out, a different rainbow colour each breath. */
  .preview-breathe {
    animation:
      breathe-fade 2.4s ease-in-out infinite,
      breathe-hue 14.4s steps(1, end) infinite;
  }

  /* Starlight: individual keys twinkling at scattered points. */
  .star {
    position: absolute;
    width: 3px;
    height: 3px;
    margin: -1.5px 0 0 -1.5px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0;
    animation-name: twinkle;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
  }

  /* Reactive: a key lights on press, then fades. */
  .preview-reactive {
    background:
      radial-gradient(circle at 50% 50%, var(--accent), transparent 62%),
      var(--bg-sunken);
    animation: reactive-press 2s ease-out infinite;
  }

  /* Ambient: whatever is on screen, so a spread of unrelated colours. */
  .preview-ambient {
    background: linear-gradient(90deg, #2b6cff, #17c3a2, #ffb020, #ff4d6d);
  }

  /* Audio Bloom: a waveform ribbon reacting to sound. */
  .preview-audiobloom svg {
    display: block;
    width: 100%;
    height: 100%;
  }

  .ribbon {
    fill: none;
    stroke-width: 3;
    stroke-linecap: round;
    transform-origin: 50% 50%;
  }

  .ribbon.back {
    stroke: color-mix(in srgb, var(--accent) 35%, transparent);
    animation: ribbon-travel 2.6s linear infinite, ribbon-pulse 1.1s ease-in-out infinite;
  }

  .ribbon.front {
    stroke: var(--accent);
    animation: ribbon-travel 1.7s linear infinite, ribbon-pulse 0.8s ease-in-out infinite;
  }

  @keyframes cycle-hue {
    0% { background-color: #ff4d4d; }
    16.66% { background-color: #ffb020; }
    33.33% { background-color: #ffe14d; }
    50% { background-color: #4dff88; }
    66.66% { background-color: #4dc8ff; }
    83.33% { background-color: #b44dff; }
    100% { background-color: #ff4d4d; }
  }

  /* Decreasing background-position moves the gradient rightwards. */
  @keyframes wave-travel {
    from { background-position: 200% 0; }
    to { background-position: 0 0; }
  }

  @keyframes breathe-fade {
    0%, 100% { opacity: 0.12; }
    50% { opacity: 1; }
  }

  /* Held per step, so the colour swaps while the tile is at its dimmest. */
  @keyframes breathe-hue {
    0% { background-color: #ff4d4d; }
    16.66% { background-color: #ffb020; }
    33.33% { background-color: #ffe14d; }
    50% { background-color: #4dff88; }
    66.66% { background-color: #4dc8ff; }
    83.33% { background-color: #b44dff; }
    100% { background-color: #ff4d4d; }
  }

  @keyframes twinkle {
    0%, 100% { opacity: 0; transform: scale(0.6); }
    50% { opacity: 1; transform: scale(1); }
  }

  @keyframes reactive-press {
    0% { opacity: 0.25; }
    12% { opacity: 1; }
    100% { opacity: 0.25; }
  }

  /* One full period of the path, so the loop is seamless. */
  @keyframes ribbon-travel {
    from { transform: translateX(0); }
    to { transform: translateX(50px); }
  }

  @keyframes ribbon-pulse {
    0%, 100% { stroke-width: 2; }
    50% { stroke-width: 4.5; }
  }
</style>
