// Key-mapping tables.
//
// These rows are the runtime's `key_bindings`: they arrive in the same state
// snapshot as every other setting and are written back through
// `set_key_bindings`, so a mapping survives a restart rather than living only
// in this webview. Rows the user has not finished are kept and sent too — the
// runtime drops them when it installs the table — so a half-written row is
// still there after closing the window.

import { NO_ACTION, isActionComplete, normalKeyCode, renamedTarget } from "./actions";
import * as ipc from "./ipc";
import type { KeyAction, KeyBinding, KeyBindings } from "./types";

/** Where the tables lived before the runtime could act on them. */
const LEGACY_STORAGE_KEY = "blade.keymap.v1";

const SAVE_DEBOUNCE_MS = 250;

export interface Row {
  id: number;
  name: string;
  keyCode: number | null;
  action: KeyAction;
}

class KeyMap {
  razer = $state<Row[]>([blankRow(1)]);
  hypershift = $state<Row[]>([blankRow(2)]);

  /** Row id currently waiting for a Razer special key, if any. */
  listeningRazer = $state<number | null>(null);
  /** Row id currently waiting for a keyboard key, if any. */
  listeningHypershift = $state<number | null>(null);
  /** Row id whose capture was rejected as a duplicate. */
  duplicateRow = $state<number | null>(null);
  /**
   * The chord cell that currently owns the keyboard, as an opaque token. Held
   * here rather than inside each cell so that only one can listen at a time,
   * and so the rest of the window knows not to treat Esc as "close me".
   */
  chordCapture = $state<string | null>(null);
  /** Set when the last save was refused, so the page can say so. */
  saveError = $state<string | null>(null);

  /** Whether any cell on the page is waiting for a key press. */
  get capturing(): boolean {
    return (
      this.listeningRazer !== null ||
      this.listeningHypershift !== null ||
      this.chordCapture !== null
    );
  }

  #nextId = 3;
  #unlisten: (() => void) | null = null;
  #seeded = false;
  #saveTimer: ReturnType<typeof setTimeout> | null = null;

  async start() {
    this.#unlisten = await ipc.onRazerKey((keyCode) => this.#applyRazerKey(keyCode));
  }

  stop() {
    this.#flush();
    this.#unlisten?.();
    this.#unlisten = null;
  }

  /**
   * Adopts the runtime's tables the first time a state snapshot arrives. After
   * that the rows are the user's working copy: reconciling them against every
   * push would fight whatever they are typing.
   */
  seed(bindings: KeyBindings) {
    if (this.#seeded) return;
    this.#seeded = true;

    const legacy = this.#legacyRows();
    const razer = bindings.razer.length > 0 ? bindings.razer : legacy?.razer;
    const hypershift = bindings.hypershift.length > 0 ? bindings.hypershift : legacy?.hypershift;

    if (razer && razer.length > 0) this.razer = razer.map((row) => this.#toRow(row));
    if (hypershift && hypershift.length > 0) {
      this.hypershift = hypershift.map((row) => this.#toRow(row));
    }
    // A migrated table only exists in this webview until it is pushed.
    if (legacy && (bindings.razer.length === 0 || bindings.hypershift.length === 0)) {
      this.save();
    }
  }

  #toRow(binding: KeyBinding): Row {
    return {
      id: this.#nextId++,
      name: binding.label,
      keyCode: binding.key_code,
      action: binding.action ?? NO_ACTION,
    };
  }

  /**
   * Rows written by the version that kept them in `localStorage`, when the
   * Action column was still a placeholder. They carry a key and a label, so
   * they are worth keeping; their free-text action never meant anything.
   */
  #legacyRows(): { razer: KeyBinding[]; hypershift: KeyBinding[] } | null {
    let raw: string | null = null;
    try {
      raw = localStorage.getItem(LEGACY_STORAGE_KEY);
    } catch {
      return null;
    }
    if (!raw) return null;

    try {
      const parsed = JSON.parse(raw) as {
        razer?: Array<{ name?: string; keyCode?: number | null }>;
        hypershift?: Array<{ keyCode?: number | null }>;
      };
      const convert = (rows: Array<{ name?: string; keyCode?: number | null }> | undefined) =>
        (rows ?? [])
          .filter((row) => typeof row.keyCode === "number")
          .map((row) => ({
            key_code: row.keyCode as number,
            label: row.name ?? "",
            action: NO_ACTION,
          }));

      const migrated = {
        razer: convert(parsed.razer),
        hypershift: convert(parsed.hypershift),
      };
      localStorage.removeItem(LEGACY_STORAGE_KEY);
      return migrated;
    } catch {
      try {
        localStorage.removeItem(LEGACY_STORAGE_KEY);
      } catch {
        // Nothing left to do: the rows are unreadable either way.
      }
      return null;
    }
  }

  // ── Razer special keys ─────────────────────────────────────────────────────

  async listenRazer(id: number) {
    this.duplicateRow = null;
    this.chordCapture = null;
    this.listeningRazer = id;
    try {
      await ipc.beginRazerKeyCapture();
    } catch {
      this.listeningRazer = null;
    }
  }

  cancelRazer() {
    this.listeningRazer = null;
    void ipc.cancelRazerKeyCapture();
  }

  #applyRazerKey(keyCode: number) {
    const id = this.listeningRazer;
    this.listeningRazer = null;
    if (id === null) return;

    if (this.razer.some((row) => row.id !== id && row.keyCode === keyCode)) {
      this.duplicateRow = id;
      return;
    }

    const row = this.razer.find((candidate) => candidate.id === id);
    if (row) row.keyCode = keyCode;
    this.duplicateRow = null;
    this.save();
  }

  // ── Hypershift ─────────────────────────────────────────────────────────────

  listenHypershift(id: number) {
    this.duplicateRow = null;
    this.chordCapture = null;
    this.listeningHypershift = id;
  }

  cancelHypershift() {
    this.listeningHypershift = null;
  }

  applyHypershiftKey(key: string): boolean {
    const id = this.listeningHypershift;
    if (id === null) return false;

    const keyCode = normalKeyCode(key);
    if (keyCode === null) return false;
    this.listeningHypershift = null;

    if (this.hypershift.some((row) => row.id !== id && row.keyCode === keyCode)) {
      this.duplicateRow = id;
      return true;
    }

    const row = this.hypershift.find((candidate) => candidate.id === id);
    if (row) row.keyCode = keyCode;
    this.duplicateRow = null;
    this.save();
    return true;
  }

  // ── Rows ───────────────────────────────────────────────────────────────────

  /** A row is finished once it has a key and an action that can actually run. */
  rowComplete(row: Row, requireName: boolean): boolean {
    return (
      row.keyCode !== null &&
      (!requireName || row.name.trim() !== "") &&
      isActionComplete(row.action)
    );
  }

  get canAddRazer(): boolean {
    return (
      this.listeningRazer === null && this.razer.every((row) => this.rowComplete(row, true))
    );
  }

  get canAddHypershift(): boolean {
    return (
      this.listeningHypershift === null &&
      this.hypershift.every((row) => this.rowComplete(row, false))
    );
  }

  addRazer() {
    this.razer = [...this.razer, blankRow(this.#nextId++)];
  }

  addHypershift() {
    this.hypershift = [...this.hypershift, blankRow(this.#nextId++)];
  }

  removeRazer(id: number) {
    if (this.listeningRazer === id) this.cancelRazer();
    this.razer = this.razer.filter((row) => row.id !== id);
    this.save();
  }

  removeHypershift(id: number) {
    if (this.listeningHypershift === id) this.cancelHypershift();
    this.hypershift = this.hypershift.filter((row) => row.id !== id);
    this.save();
  }

  setAction(row: Row, action: KeyAction) {
    row.action = action;
    this.save();
  }

  /**
   * Follows a Command Lab capture or control through a rename, or clears the
   * rows bound to it when it was deleted.
   *
   * These rows are the page's working copy and are seeded only once, so this
   * has to go through them: a write to the config that went around them would
   * be saved over by the next edit on the Keys page. A cleared row keeps its
   * key and label and shows as unfinished, so the user decides what it becomes
   * rather than losing the mapping outright.
   *
   * Written at once rather than on the typing debounce: this follows an edit on
   * another page, and nothing here is being typed.
   */
  followCommandLab(kind: "replay_capture" | "toggle_custom_control", from: string, to: string) {
    let touched = false;
    for (const row of [...this.razer, ...this.hypershift]) {
      const next = renamedTarget(row.action, kind, from, to);
      if (!next) continue;
      row.action = next;
      touched = true;
    }
    if (!touched) return;

    this.save();
    this.#flush();
  }

  /**
   * Pushes both tables. Debounced because typing a command or a label would
   * otherwise write the config file on every keystroke.
   */
  save() {
    if (this.#saveTimer !== null) clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => {
      this.#saveTimer = null;
      void this.#push();
    }, SAVE_DEBOUNCE_MS);
  }

  #flush() {
    if (this.#saveTimer === null) return;
    clearTimeout(this.#saveTimer);
    this.#saveTimer = null;
    void this.#push();
  }

  async #push() {
    const bindings: KeyBindings = {
      razer: this.razer.filter(hasKey).map(toBinding),
      hypershift: this.hypershift.filter(hasKey).map(toBinding),
    };

    try {
      await ipc.setKeyBindings(bindings);
      this.saveError = null;
    } catch (error) {
      this.saveError = ipc.errorMessage(error);
    }
  }
}

function blankRow(id: number): Row {
  return { id, name: "", keyCode: null, action: NO_ACTION };
}

/** A row with no key yet is not a binding; it is a row the user is still on. */
function hasKey(row: Row): boolean {
  return row.keyCode !== null;
}

function toBinding(row: Row): KeyBinding {
  return {
    key_code: row.keyCode as number,
    label: row.name.trim(),
    action: $state.snapshot(row.action),
  };
}

export const keyMap = new KeyMap();
