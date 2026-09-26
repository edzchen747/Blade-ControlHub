<script lang="ts">
  import { isActionComplete, isCommandLabAction, strayCommandLabTarget } from "../actions";
  import { store } from "../store.svelte";
  import type { DeviceAction, KeyAction } from "../types";
  import AppPicker from "./AppPicker.svelte";
  import ChordCapture from "./ChordCapture.svelte";
  import KeySelect from "./KeySelect.svelte";

  interface Props {
    /** Identifies the row, so only one chord cell on the page can listen. */
    owner: string;
    action: KeyAction;
    onchange: (action: KeyAction) => void;
  }

  let { owner, action, onchange }: Props = $props();

  let picking = $state(false);

  const deviceActions = $derived(
    Object.entries(store.state?.meta.device_action_labels ?? {}) as Array<[DeviceAction, string]>,
  );

  /** Named Command Lab captures, which only exist once the user records one. */
  const captures = $derived(Object.keys(store.state?.command_lab_commands ?? {}).sort());
  /** Custom controls, which the user builds out of two captures. */
  const controls = $derived(
    (store.state?.custom_toggles ?? [])
      .map((toggle) => toggle.name)
      .filter((name) => name.trim() !== "")
      .sort(),
  );
  const experimentalEnabled = $derived(store.state?.advanced_experimental_features ?? false);

  const complete = $derived(isActionComplete(action));

  // One dropdown offers both, so an option value has to say which list it came
  // from: a capture and a control may share a name, and they are not the same
  // action.
  const commandLabValue = $derived.by(() => {
    if (action.kind === "toggle_custom_control") return `control:${action.name}`;
    if (action.kind === "replay_capture") return `capture:${action.name}`;
    return "";
  });

  /**
   * The row's own target when no option matches it: not chosen yet, or gone.
   * Without an option of its own the browser would show the first entry as if
   * it were selected, so the row would read as bound to something it is not.
   */
  const strayTarget = $derived(strayCommandLabTarget(action, captures, controls));

  function chooseCommandLabTarget(value: string): KeyAction {
    const name = value.slice(value.indexOf(":") + 1);
    return value.startsWith("control:")
      ? { kind: "toggle_custom_control", name }
      : { kind: "replay_capture", name };
  }
</script>

{#if action.kind === "key"}
  <KeySelect chord={action.chord} onchange={(chord) => onchange({ kind: "key", chord })} />
{:else if action.kind === "macro"}
  <ChordCapture
    {owner}
    steps={action.steps}
    onchange={(steps) => onchange({ kind: "macro", steps })}
  />
{:else if action.kind === "device"}
  <select
    class="select"
    value={action.action}
    aria-label="Device control action"
    onchange={(event) =>
      onchange({ kind: "device", action: event.currentTarget.value as DeviceAction })}
  >
    {#each deviceActions as [value, label] (value)}
      <option {value}>{label}</option>
    {/each}
  </select>
{:else if action.kind === "launch_app"}
  <div class="launch">
    <button type="button" class="target" class:invalid={!complete} onclick={() => (picking = true)}>
      {action.name.trim() || action.path.trim() || "Choose an application…"}
    </button>
    <input
      class="input"
      placeholder="Arguments (optional)"
      value={action.args}
      aria-label="Launch arguments"
      oninput={(event) =>
        onchange({
          kind: "launch_app",
          path: action.kind === "launch_app" ? action.path : "",
          name: action.kind === "launch_app" ? action.name : "",
          args: event.currentTarget.value,
        })}
    />
  </div>
{:else if action.kind === "run_command"}
  <input
    class="input"
    class:invalid={!complete}
    placeholder="A command, run through cmd"
    value={action.command}
    aria-label="Command"
    oninput={(event) => onchange({ kind: "run_command", command: event.currentTarget.value })}
  />
{:else if isCommandLabAction(action.kind)}
  {#if captures.length === 0 && controls.length === 0}
    <!-- A row can still be set to this after the flag was turned off, and then
         pointing at the Command Lab page would be pointing at a hidden one. -->
    <span class="hint">
      {experimentalEnabled
        ? "Record a capture on the Command Lab page first."
        : "Turn on advanced experimental features on the System page to record a capture."}
    </span>
  {:else}
    <select
      class="select"
      class:invalid={!complete}
      value={commandLabValue}
      aria-label="Command Lab capture or control"
      onchange={(event) => onchange(chooseCommandLabTarget(event.currentTarget.value))}
    >
      {#if strayTarget}
        <option value={commandLabValue} disabled>{strayTarget}</option>
      {/if}
      {#if controls.length > 0}
        <!-- Controls first: a control is the thing a key is usually bound to,
             and a capture is the raw material it is built from. -->
        <optgroup label="Custom controls">
          {#each controls as name (name)}
            <option value="control:{name}">{name}</option>
          {/each}
        </optgroup>
      {/if}
      <optgroup label="Captures">
        {#each captures as name (name)}
          <option value="capture:{name}">{name}</option>
        {/each}
      </optgroup>
    </select>
  {/if}
{/if}

{#if picking}
  <AppPicker
    onclose={() => (picking = false)}
    onpick={(app) => {
      picking = false;
      onchange({
        kind: "launch_app",
        path: app.path,
        name: app.name,
        args: action.kind === "launch_app" ? action.args : "",
      });
    }}
  />
{/if}

<style>
  /* The application and its arguments read as one setting, so they share the
     row rather than stacking. */
  .launch {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--gap-sm);
  }

  .target {
    text-align: left;
    direction: ltr;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .target.invalid {
    border-color: var(--danger);
    color: var(--danger);
  }
</style>
