<script lang="ts">
  import { availableActionKinds, defaultActionFor } from "../actions";
  import { store } from "../store.svelte";
  import type { ActionKind, DeviceAction, KeyAction } from "../types";

  interface Props {
    action: KeyAction;
    onchange: (action: KeyAction) => void;
  }

  let { action, onchange }: Props = $props();

  const firstDeviceAction = $derived(
    (Object.keys(store.state?.meta.device_action_labels ?? {})[0] ??
      "cycle_perf_mode") as DeviceAction,
  );
  const firstCapture = $derived(Object.keys(store.state?.command_lab_commands ?? {}).sort()[0]);

  // Replaying a capture is a Command Lab feature, so it is only on offer while
  // advanced experimental features are.
  const kinds = $derived(
    availableActionKinds(store.state?.advanced_experimental_features ?? false, action.kind),
  );
</script>

<select
  class="select"
  value={action.kind}
  aria-label="Action"
  onchange={(event) =>
    onchange(
      defaultActionFor(event.currentTarget.value as ActionKind, firstDeviceAction, firstCapture),
    )}
>
  {#each kinds as kind (kind.value)}
    <option value={kind.value}>{kind.label}</option>
  {/each}
</select>
