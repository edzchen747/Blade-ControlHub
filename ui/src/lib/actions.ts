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

/**
 * The options in the Action dropdown.
 *
 * One option can cover more than one runtime kind: "Command Lab" stands for
 * both replaying a capture and flipping a custom control, which the row then
 * chooses between in its own control. So these are option values, not the whole
 * `KeyAction` vocabulary.
 */
export const ACTION_KINDS: readonly ActionKindOption[] = [
  { value: "none", label: "Do nothing" },
  { value: "key", label: "Send a key" },
  { value: "macro", label: "Run a macro" },
  { value: "device", label: "Device control" },
  { value: "toggle_ui", label: "Open ControlHub" },
  { value: "launch_app", label: "Launch an application" },
  { value: "run_command", label: "Run a command" },
  { value: "replay_capture", label: "Command Lab" },
];

/** The kinds the Command Lab option stands for. */
export function isCommandLabAction(kind: ActionKind): boolean {
  return kind === "replay_capture" || kind === "toggle_custom_control";
}

/**
 * The action with a renamed Command Lab target followed through, or its target
 * cleared when `to` is empty because it was deleted. Returns `null` when the
 * action does not point at it, so nothing unrelated is rewritten.
 *
 * A binding names its capture or control rather than copying it, so without
 * this a rename in Command Lab would leave the key bound to nothing. A capture
 * and a control may share a name, so only the kind that was renamed follows.
 */
export function renamedTarget(
  action: KeyAction,
  kind: "replay_capture" | "toggle_custom_control",
  from: string,
  to: string,
): KeyAction | null {
  if (action.kind !== "replay_capture" && action.kind !== "toggle_custom_control") return null;
  if (action.kind !== kind || action.name !== from) return null;
  return { ...action, name: to };
}

/**
 * What a Command Lab row's dropdown shows when none of its options is the row's
 * own target — not chosen yet, or since deleted or renamed away — or `null`
 * when the target is on offer.
 *
 * Without an option of its own the browser shows the first entry as if it were
 * selected, so the row would read as bound to something it is not.
 */
export function strayCommandLabTarget(
  action: KeyAction,
  captures: readonly string[],
  controls: readonly string[],
): string | null {
  if (action.kind !== "replay_capture" && action.kind !== "toggle_custom_control") return null;
  if (action.name === "") return "Choose a capture or control…";
  const offered = action.kind === "toggle_custom_control" ? controls : captures;
  return offered.includes(action.name) ? null : `${action.name} (missing)`;
}

/**
 * The option a row's action is shown under. A custom control has no option of
 * its own — it is one of the things Command Lab offers — so it reads back as
 * that option rather than leaving the dropdown blank.
 */
export function actionKindOption(kind: ActionKind): ActionKind {
  return isCommandLabAction(kind) ? "replay_capture" : kind;
}

/**
 * The kinds a row may choose from.
 *
 * Command Lab is behind the advanced experimental features flag, so it is not
 * offered while that is off — the page must not advertise a feature the user
 * has not turned on.
 *
 * A row already set to it keeps the option, for two reasons: the binding still
 * works, because the saved captures and controls live in the config rather than
 * behind the flag, and a dropdown whose current value is missing renders blank.
 */
export function availableActionKinds(
  experimentalEnabled: boolean,
  current: ActionKind,
): ActionKindOption[] {
  return ACTION_KINDS.filter(
    (kind) =>
      kind.value !== "replay_capture" || experimentalEnabled || isCommandLabAction(current),
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
    case "toggle_custom_control":
      return { kind: "toggle_custom_control", name: "" };
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
    case "toggle_custom_control":
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
    case "toggle_custom_control":
      return action.name.trim() || "No custom control yet";
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

/**
 * A captured Razer special key, as its code.
 *
 * There is deliberately no table of names here. Which code a key sends differs
 * between Blades, so naming them would mean labelling the wrong keys on most
 * models — the row's own label, which the user writes, says what it is.
 */
export function razerKeyLabel(keyCode: number | null): string {
  if (keyCode === null) return "None";
  return `0x${keyCode.toString(16).toUpperCase().padStart(2, "0")}`;
}
