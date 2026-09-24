// Key-mapping tables.
//
// These rows are not part of the persisted device config: the runtime has no
// action vocabulary to bind them to yet, so the Action column stays a
// placeholder, exactly as it did before. They are kept in `localStorage` so a
// table the user has filled in survives closing the window — previously it was
// lost whenever the settings process exited.

import * as ipc from "./ipc";

const STORAGE_KEY = "blade.keymap.v1";

export interface RazerRow {
  id: number;
  name: string;
  keyCode: number | null;
  action: string;
}

export interface HypershiftRow {
  id: number;
  keyCode: number | null;
  action: string;
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

export function razerKeyLabel(keyCode: number | null): string {
  if (keyCode === null) return "None";
  return `0x${keyCode.toString(16).toUpperCase().padStart(2, "0")}`;
}

interface Persisted {
  razer: RazerRow[];
  hypershift: HypershiftRow[];
}

class KeyMap {
  razer = $state<RazerRow[]>([{ id: 1, name: "", keyCode: null, action: "" }]);
  hypershift = $state<HypershiftRow[]>([{ id: 1, keyCode: null, action: "" }]);

  /** Row id currently waiting for a Razer special key, if any. */
  listeningRazer = $state<number | null>(null);
  /** Row id currently waiting for a keyboard key, if any. */
  listeningHypershift = $state<number | null>(null);
  /** Row id whose capture was rejected as a duplicate. */
  duplicateRow = $state<number | null>(null);

  #nextId = 2;
  #unlisten: (() => void) | null = null;

  async start() {
    this.#load();
    this.#unlisten = await ipc.onRazerKey((keyCode) => this.#applyRazerKey(keyCode));
  }

  stop() {
    this.#unlisten?.();
    this.#unlisten = null;
  }

  // ── Razer special keys ─────────────────────────────────────────────────────

  async listenRazer(id: number) {
    this.duplicateRow = null;
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

  get canAddRazer(): boolean {
    return (
      this.listeningRazer === null &&
      this.razer.every((row) => row.name.trim() !== "" && row.keyCode !== null)
    );
  }

  get canAddHypershift(): boolean {
    return (
      this.listeningHypershift === null &&
      this.hypershift.every((row) => row.keyCode !== null)
    );
  }

  addRazer() {
    this.razer = [...this.razer, { id: this.#nextId++, name: "", keyCode: null, action: "" }];
    this.save();
  }

  addHypershift() {
    this.hypershift = [...this.hypershift, { id: this.#nextId++, keyCode: null, action: "" }];
    this.save();
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

  save() {
    const payload: Persisted = {
      razer: $state.snapshot(this.razer),
      hypershift: $state.snapshot(this.hypershift),
    };
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
    } catch {
      // A full or blocked store only costs persistence, not this session.
    }
  }

  #load() {
    let raw: string | null = null;
    try {
      raw = localStorage.getItem(STORAGE_KEY);
    } catch {
      return;
    }
    if (!raw) return;

    try {
      const parsed = JSON.parse(raw) as Persisted;
      if (Array.isArray(parsed.razer) && parsed.razer.length > 0) this.razer = parsed.razer;
      if (Array.isArray(parsed.hypershift) && parsed.hypershift.length > 0) {
        this.hypershift = parsed.hypershift;
      }
      this.#nextId =
        Math.max(0, ...this.razer.map((row) => row.id), ...this.hypershift.map((row) => row.id)) + 1;
    } catch {
      // Unreadable storage falls back to the empty tables already in place.
    }
  }
}

export const keyMap = new KeyMap();
