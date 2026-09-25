<script lang="ts">
  import { chordLabel } from "../actions";
  import { keyMap } from "../keymap.svelte";
  import { MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN, type Chord } from "../types";

  interface Props {
    /** Identifies the row this cell belongs to, so two cannot listen at once. */
    owner: string;
    steps: Chord[];
    onchange: (steps: Chord[]) => void;
  }

  let { owner, steps, onchange }: Props = $props();

  /**
   * Which step is listening. The token lives on the shared map rather than
   * here: every cell on the page listens on `window`, so without one owner a
   * single press would land in two rows at once — and the window's own Esc
   * handler needs to know a capture is in progress.
   */
  const listening = $derived.by(() => {
    const token = keyMap.chordCapture;
    if (token === null || !token.startsWith(`${owner}:`)) return null;
    return Number(token.slice(owner.length + 1));
  });

  /**
   * Modifier keys on their own are not a chord — they are the prefix of one —
   * so a press of Ctrl leaves the capture armed and waits for the real key.
   */
  const MODIFIER_KEYS = new Set([
    "Control",
    "Alt",
    "Shift",
    "Meta",
    "AltGraph",
    "CapsLock",
    "OS",
  ]);

  function listen(index: number) {
    keyMap.chordCapture = listening === index ? null : `${owner}:${index}`;
  }

  function onKeyDown(event: KeyboardEvent) {
    if (listening === null) return;

    if (event.key === "Escape") {
      keyMap.chordCapture = null;
      event.preventDefault();
      event.stopPropagation();
      return;
    }

    // Everything is captured, including Tab and the browser's own shortcuts:
    // while a cell is listening the keyboard belongs to it.
    event.preventDefault();
    event.stopPropagation();
    if (MODIFIER_KEYS.has(event.key)) return;

    const key = virtualKeyFrom(event);
    if (key === 0) return;

    let modifiers = 0;
    if (event.ctrlKey) modifiers |= MOD_CTRL;
    if (event.altKey) modifiers |= MOD_ALT;
    if (event.shiftKey) modifiers |= MOD_SHIFT;
    if (event.metaKey) modifiers |= MOD_WIN;

    const index = listening;
    keyMap.chordCapture = null;
    onchange(steps.map((step, at) => (at === index ? { modifiers, key } : step)));
  }

  /**
   * A Windows virtual-key code from a DOM event. `KeyboardEvent.keyCode` is the
   * virtual-key code on Windows and is still what every browser reports, so it
   * is used where it is present and `code` covers the rest.
   */
  function virtualKeyFrom(event: KeyboardEvent): number {
    if (event.keyCode > 0) return event.keyCode;
    if (/^Key[A-Z]$/.test(event.code)) return event.code.charCodeAt(3);
    if (/^Digit[0-9]$/.test(event.code)) return event.code.charCodeAt(5);
    return 0;
  }

  function addStep() {
    keyMap.chordCapture = `${owner}:${steps.length}`;
    onchange([...steps, { modifiers: 0, key: 0 }]);
  }

  function removeStep(index: number) {
    if (listening === index) keyMap.chordCapture = null;
    onchange(steps.filter((_, at) => at !== index));
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="chords">
  {#each steps as step, index (index)}
    <div class="step">
      {#if steps.length > 1}
        <span class="ordinal" aria-hidden="true">{index + 1}</span>
      {/if}
      <button
        type="button"
        class="chord"
        class:listening={listening === index}
        class:invalid={step.key === 0 && listening !== index}
        aria-label={`Step ${index + 1}: ${step.key === 0 ? "not set" : chordLabel(step)}`}
        onclick={() => listen(index)}
      >
        {#if listening === index}
          Press keys…
        {:else if step.key === 0}
          Click to set
        {:else}
          {chordLabel(step)}
        {/if}
      </button>
      {#if steps.length > 1}
        <button
          type="button"
          class="icon danger"
          aria-label={`Remove step ${index + 1}`}
          onclick={() => removeStep(index)}
        >
          ✕
        </button>
      {/if}
    </div>
  {/each}

  <div class="row">
    <button type="button" class="ghost" onclick={addStep}>+ Add step</button>
    <span class="hint">
      {steps.length > 1
        ? "Steps are sent in order."
        : "Add a step to send more than one keystroke."}
    </span>
  </div>
</div>

<style>
  .chords {
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
  }

  .step {
    display: flex;
    align-items: center;
    gap: var(--gap-xs);
  }

  .ordinal {
    font-size: 11.5px;
    color: var(--fg-faint);
    width: 12px;
    text-align: right;
  }

  .chord {
    font-family: var(--mono);
    font-size: 12.5px;
    padding: 5px 8px;
    flex: 1 1 auto;
    text-align: left;
  }

  .chord.listening {
    border-color: var(--accent);
    color: var(--accent);
    animation: pulse 1.2s ease-in-out infinite;
  }

  .chord.invalid {
    border-color: var(--danger);
    color: var(--danger);
  }

  @keyframes pulse {
    0%,
    100% {
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 55%, transparent);
    }
    50% {
      box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 0%, transparent);
    }
  }
</style>
