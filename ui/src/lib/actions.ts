// What a key can be bound to, as plain functions over plain data.
//
// Everything here mirrors `win::input::binding` in the runtime and holds no
// state of its own, which is what keeps `keymap.svelte.ts` to the reactive
// tables and lets these rules be tested directly.

import {
  KEY_CATALOG,
  MOD_ALT,
  MOD_CTRL,
  MOD_SHIFT,
  MOD_WIN,
  type ActionKind,
  type Chord,
  type DeviceAction,
  type KeyAction,
} from "./types";

export const NO_ACTION: KeyAction = { kind: "none" };

export interface ActionKindOption {
  value: ActionKind;
  label: string;
}

export const ACTION_KINDS: readonly ActionKindOption[] = [
  { value: "none", label: "Do nothing" },
  { value: "key", label: "Send a key" },
  { value: "macro", label: "Run a macro" },
  { value: "device", label: "Device control" },
  { value: "toggle_ui", label: "Open ControlHub" },
  { value: "launch_app", label: "Launch an application" },
  { value: "run_command", label: "Run a command" },
  { value: "replay_capture", label: "Replay a capture" },
];

/**
 * The kinds a row may choose from.
 *
 * Replaying a capture belongs to Command Lab, which is behind the advanced
 * experimental features flag, so it is not offered while that is off — the page
 * must not advertise a feature the user has not turned on.
 *
 * A row already set to it keeps the option, for two reasons: the binding still
 * works, because the saved captures live in the config rather than behind the
 * flag, and a dropdown whose current value is missing renders as blank.
 */
export function availableActionKinds(
  experimentalEnabled: boolean,
  current: ActionKind,
): ActionKindOption[] {
  return ACTION_KINDS.filter(
    (kind) =>
      kind.value !== "replay_capture" || experimentalEnabled || current === "replay_capture",
  );
}

/**
 * A fresh action of the given kind. Switching kind starts empty rather than
 * carrying fields across, so a row never keeps an application path from an
 * action it no longer is.
 */
export function defaultActionFor(
  kind: ActionKind,
  firstDeviceAction: DeviceAction,
  firstCapture: string | undefined,
): KeyAction {
  switch (kind) {
    case "none":
      return { kind: "none" };
    case "key":
      return { kind: "key", chord: { modifiers: 0, key: 0 } };
    case "macro":
      return { kind: "macro", steps: [{ modifiers: 0, key: 0 }] };
    case "device":
      return { kind: "device", action: firstDeviceAction };
    case "toggle_ui":
      return { kind: "toggle_ui" };
    case "launch_app":
      return { kind: "launch_app", path: "", args: "", name: "" };
    case "run_command":
      return { kind: "run_command", command: "" };
    case "replay_capture":
      return { kind: "replay_capture", name: firstCapture ?? "" };
  }
}

/**
 * Whether an action carries everything it needs to run. Mirrors
 * `KeyAction::is_complete` in the runtime: a row that fails this is stored but
 * never dispatched, so the page must not let it pass as finished.
 */
export function isActionComplete(action: KeyAction): boolean {
  switch (action.kind) {
    case "none":
    case "toggle_ui":
    case "device":
      return true;
    case "key":
      return action.chord.key !== 0;
    case "macro":
      return action.steps.length > 0 && action.steps.every((step) => step.key !== 0);
    case "launch_app":
      return action.path.trim() !== "";
    case "run_command":
      return action.command.trim() !== "";
    case "replay_capture":
      return action.name.trim() !== "";
  }
}

/**
 * Whether the action needs a control of its own. Those controls are given the
 * full width of the row rather than the Action column, so the page asks first
 * instead of rendering an empty second line for the kinds that need nothing.
 */
export function hasActionDetail(action: KeyAction): boolean {
  return action.kind !== "none" && action.kind !== "toggle_ui";
}

/** One line describing an action, for the error under an unfinished row. */
export function actionSummary(action: KeyAction, deviceLabels: Record<string, string>): string {
  switch (action.kind) {
    case "none":
      return "Do nothing";
    case "toggle_ui":
      return "Open ControlHub";
    case "device":
      return deviceLabels[action.action] ?? action.action;
    case "key":
      return action.chord.key === 0 ? "No key yet" : chordLabel(action.chord);
    case "macro":
      return action.steps.length === 0 ? "No steps yet" : action.steps.map(chordLabel).join(" , ");
    case "launch_app":
      return action.name.trim() || action.path.trim() || "No application yet";
    case "run_command":
      return action.command.trim() || "No command yet";
    case "replay_capture":
      return action.name.trim() || "No capture yet";
  }
}

export function chordLabel(chord: Chord): string {
  const parts: string[] = [];
  if (chord.modifiers & MOD_CTRL) parts.push("Ctrl");
  if (chord.modifiers & MOD_ALT) parts.push("Alt");
  if (chord.modifiers & MOD_SHIFT) parts.push("Shift");
  if (chord.modifiers & MOD_WIN) parts.push("Win");
  parts.push(virtualKeyLabel(chord.key));
  return parts.join("+");
}

/**
 * Every key in the catalogue, by code. Derived rather than written out again so
 * a key the dropdown offers and the same key in a macro always read the same.
 */
const KEY_NAMES: Map<number, string> = new Map(KEY_CATALOG.flatMap((group) => group.keys));

export function virtualKeyLabel(key: number): string {
  if (key === 0) return "…";
  return KEY_NAMES.get(key) ?? `0x${key.toString(16).toUpperCase().padStart(2, "0")}`;
}

/** Windows virtual-key codes the Hypershift table accepts. */
export function normalKeyCode(key: string): number | null {
  if (/^[0-9]$/.test(key)) return key.charCodeAt(0);
  if (/^[a-zA-Z]$/.test(key)) return key.toUpperCase().charCodeAt(0);
  return null;
}

export function normalKeyLabel(keyCode: number | null): string {
  if (keyCode === null) return "None";
  const isDigit = keyCode >= 0x30 && keyCode <= 0x39;
  const isLetter = keyCode >= 0x41 && keyCode <= 0x5a;
  return isDigit || isLetter ? String.fromCharCode(keyCode) : "Unknown";
}

/** The names printed on the keys, so a row reads as hardware, not as a byte. */
const RAZER_KEY_NAMES: Record<number, string> = {
  0x03: "Game",
  0x24: "M1",
  0x25: "M2",
  0x26: "M3",
  0x27: "M4",
  0xd2: "Copilot",
  0xd3: "Performance",
  0xd4: "Mic mute",
  0xd5: "Home",
  0xd6: "Up",
  0xd7: "Page up",
  0xd8: "Left",
  0xd9: "Right",
  0xda: "End",
  0xdb: "Down",
  0xdc: "Page down",
  0xdd: "Trackpad",
};

export function razerKeyLabel(keyCode: number | null): string {
  if (keyCode === null) return "None";
  return RAZER_KEY_NAMES[keyCode] ?? `0x${keyCode.toString(16).toUpperCase().padStart(2, "0")}`;
}
