<script lang="ts">
  import { isActionComplete } from "../actions";
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
  const experimentalEnabled = $derived(store.state?.advanced_experimental_features ?? false);

  const complete = $derived(isActionComplete(action));
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
{:else if action.kind === "replay_capture"}
  {#if captures.length === 0}
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
      value={action.name}
      aria-label="Capture to replay"
      onchange={(event) => onchange({ kind: "replay_capture", name: event.currentTarget.value })}
    >
      {#each captures as name (name)}
        <option value={name}>{name}</option>
      {/each}
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
