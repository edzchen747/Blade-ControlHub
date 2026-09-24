<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";

  import Section from "../components/Section.svelte";
  import * as ipc from "../ipc";
  import { store } from "../store.svelte";
  import type {
    CapturedCommand,
    CommandLabRecordingState,
    CommandLabStatus,
    UsbpcapInfo,
  } from "../types";

  const TOTAL_STEPS = 5;

  interface Row {
    id: number;
    name: string;
    /** The name this row is saved under, so a rename can remove the old one. */
    savedAs: string | null;
    commands: CapturedCommand[];
    status: CommandLabStatus;
    step: number;
    capturedCount: number;
  }

  let rows = $state<Row[]>([]);
  let recordingId = $state<number | null>(null);
  let usbpcap = $state<UsbpcapInfo | null>(null);
  let showHelp = $state(false);
  let nextId = 1;
  let seeded = false;

  const HELP = [
    {
      question: 'What does "Failed, too many commands" mean?',
      answer:
        "Too many commands were captured in the five-second window. Set the keyboard RGB to static, or drive the backlight with dynamic lighting instead of Chroma, and record again.",
    },
    {
      question: "Does the capture include ControlHub's own commands?",
      answer:
        "No. ControlHub pauses its own device traffic while recording, notes anything it does send, and removes it from the result — so a capture only contains what another application sent.",
    },
    {
      question: "Commands are captured but replay does nothing?",
      answer:
        "Some Razer features — Snap Tap, game mode — are not plain USB commands; Synapse implements them itself, so there is nothing to replay.",
    },
  ];

  const duplicateNames = $derived(
    new Set(
      rows
        .map((row) => row.name.trim().toLowerCase())
        .filter((name, index, all) => name !== "" && all.indexOf(name) !== index),
    ),
  );

  onMount(() => {
    void refreshUsbpcap();
    const unlisten = ipc.onCommandLab(applyRecordingState);
    return () => {
      void unlisten.then((stop) => stop());
      if (recordingId !== null) void ipc.cancelCommandLabRecord();
    };
  });

  // Seed once from the saved config; after that the rows are the user's
  // working copy and are only reconciled when they save or delete.
  $effect(() => {
    const saved = store.state?.command_lab_commands;
    if (!saved || seeded) return;
    seeded = true;
    rows = Object.entries(saved).map(([name, commands]) => ({
      id: nextId++,
      name,
      savedAs: name,
      commands,
      status: "Idle" as CommandLabStatus,
      step: 0,
      capturedCount: commands.length,
    }));
    if (rows.length === 0) addRow();
  });

  async function refreshUsbpcap() {
    try {
      usbpcap = await ipc.getUsbpcapStatus();
    } catch {
      usbpcap = null;
    }
  }

  function addRow() {
    rows = [
      ...rows,
      {
        id: nextId++,
        name: "",
        savedAs: null,
        commands: [],
        status: "Idle",
        step: 0,
        capturedCount: 0,
      },
    ];
  }

  const canAddRow = $derived(
    recordingId === null &&
      rows.every((row) => row.name.trim() !== "" && row.commands.length > 0),
  );

  async function record(row: Row) {
    if (recordingId !== null) return;
    recordingId = row.id;
    row.status = "Recording";
    row.step = 0;
    row.commands = [];

    try {
      // Resolves once the capture is really running, after any UAC prompt.
      applyRecordingState(await ipc.beginCommandLabRecord());
    } catch {
      row.status = "Failed";
      recordingId = null;
    }
  }

  function applyRecordingState(state: CommandLabRecordingState) {
    const row = rows.find((candidate) => candidate.id === recordingId);
    if (!row) return;

    row.status = state.status;
    row.step = state.step;
    row.capturedCount = state.captured_commands;
    if (state.commands.length > 0) row.commands = state.commands;

    if (state.status === "Recording" || state.status === "Idle") return;

    recordingId = null;
    if (state.status === "Done" && row.name.trim() !== "") void save(row);
  }

  async function cancel() {
    await ipc.cancelCommandLabRecord();
    recordingId = null;
  }

  async function save(row: Row) {
    const name = row.name.trim();
    if (name === "" || row.commands.length === 0 || duplicateNames.has(name.toLowerCase())) return;

    if (row.savedAs && row.savedAs !== name) await ipc.removeCommandLabCommand(row.savedAs);
    await ipc.saveCommandLabCommands(name, $state.snapshot(row.commands));
    row.savedAs = name;
  }

  async function remove(row: Row) {
    if (row.savedAs) await ipc.removeCommandLabCommand(row.savedAs);
    rows = rows.filter((candidate) => candidate.id !== row.id);
  }

  const hex = (value: number, width: number) =>
    value.toString(16).toUpperCase().padStart(width, "0");

  const formatCommand = (command: CapturedCommand) =>
    `0x${hex(command.command, 4)}  ${command.args.map((arg) => hex(arg, 2)).join(" ")}`;

  function preview(commands: CapturedCommand[]): string {
    const first = commands[0];
    if (!first) return "";
    const bytes = [hex(first.command >> 8, 2), hex(first.command & 0xff, 2)]
      .concat(first.args.slice(0, 3).map((arg) => hex(arg, 2)))
      .join(" ");
    const more = commands.length > 1 ? ` +${commands.length - 1}` : "";
    return `${bytes}…${more}`;
  }

  function statusText(row: Row): string | null {
    switch (row.status) {
      case "Recording":
        return `Recording ${Math.max(0, TOTAL_STEPS - row.step)}…`;
      case "TooManyCommands":
        return `Failed — too many commands (${row.capturedCount})`;
      case "NoCommandsRecorded":
        return "No commands were recorded";
      case "Cancelled":
        return "Cancelled";
      case "Failed":
        return "Capture could not start";
      default:
        return null;
    }
  }
</script>

<Section title="Experimental">
  <p class="hint warn">
    ⚠ Experimental features may not work on every device, and some configurations can
    behave unexpectedly. You can turn them off again on the System page.
  </p>
  <p class="hint">
    Record the USB traffic your device receives while you change a setting in Synapse,
    then replay it from ControlHub. USBPcap is the Windows driver that captures those
    reports; starting a capture needs administrator privileges.
  </p>
  <div class="row-between">
    <span class="muted">{usbpcap?.label ?? "Checking USBPcap driver…"}</span>
    {#if usbpcap && !usbpcap.installed}
      <button type="button" onclick={() => openUrl(usbpcap!.download_url)}>
        Download USBPcap
      </button>
    {/if}
  </div>
</Section>

<Section title="Captures">
  <div class="table">
    <div class="head">
      <span>Name</span>
      <span>Captured</span>
      <span></span>
    </div>

    {#each rows as row (row.id)}
      <div class="line">
        <div class="cells">
          <input
            class="input"
            class:invalid={duplicateNames.has(row.name.trim().toLowerCase())}
            placeholder="Name this capture"
            bind:value={row.name}
            onchange={() => save(row)}
            aria-label="Capture name"
          />

          {#if row.commands.length > 0 && row.status !== "TooManyCommands"}
            <code class="code" title={row.commands.map(formatCommand).join("\n")}>
              {preview(row.commands)}
            </code>
          {:else}
            <span class="muted empty">{statusText(row) ?? "Not recorded"}</span>
          {/if}

          <div class="actions">
            {#if recordingId === row.id}
              <button type="button" onclick={cancel}>Cancel</button>
            {:else}
              <button type="button" disabled={recordingId !== null} onclick={() => record(row)}>
                Record
              </button>
            {/if}
            <button
              type="button"
              class="icon"
              aria-label="Replay capture"
              disabled={row.commands.length === 0 || recordingId !== null}
              onclick={() => ipc.playCommandLabCommands($state.snapshot(row.commands))}
            >
              ▶
            </button>
            <button
              type="button"
              class="icon danger"
              aria-label="Delete capture"
              disabled={recordingId !== null}
              onclick={() => remove(row)}
            >
              ✕
            </button>
          </div>
        </div>

        {#if recordingId === row.id}
          <div class="progress" aria-hidden="true">
            {#each Array.from({ length: TOTAL_STEPS }, (_, index) => index) as index (index)}
              <span class:on={index < row.step}></span>
            {/each}
          </div>
        {/if}
        {#if row.commands.length > 0 && statusText(row)}
          <span class="field-error">{statusText(row)}</span>
        {/if}
        {#if duplicateNames.has(row.name.trim().toLowerCase())}
          <span class="field-error">That name is already used.</span>
        {/if}
      </div>
    {/each}
  </div>

  <div class="row-between">
    <div class="row">
      <button type="button" disabled={!canAddRow} onclick={addRow}>+ New capture</button>
      {#if !canAddRow && recordingId === null}
        <span class="hint">Name and record the current row first.</span>
      {/if}
    </div>
    <button type="button" class="ghost" aria-expanded={showHelp} onclick={() => (showHelp = !showHelp)}>
      Help {showHelp ? "▴" : "▾"}
    </button>
  </div>

  {#if showHelp}
    <div class="help">
      {#each HELP as item (item.question)}
        <div>
          <strong>{item.question}</strong>
          <p class="hint">{item.answer}</p>
        </div>
      {/each}
    </div>
  {/if}
</Section>

<style>
  .warn {
    color: var(--warn);
  }

  .table {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
  }

  .head,
  .cells {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.2fr) auto;
    gap: var(--gap-sm);
    align-items: center;
  }

  .head {
    font-size: 11.5px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-faint);
  }

  .line {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .empty {
    font-size: 12.5px;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--gap-xs);
  }

  .progress {
    display: flex;
    gap: 4px;
  }

  .progress span {
    height: 3px;
    flex: 1 1 0;
    border-radius: 999px;
    background: var(--bg-sunken);
  }

  .progress span.on {
    background: var(--accent);
  }

  .help {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    padding: var(--gap);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--bg-sunken);
  }
</style>
