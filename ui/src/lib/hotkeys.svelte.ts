// The window's own keyboard path.
//
// Windows does not invoke the runtime's `WH_KEYBOARD_LL` hook for input aimed at
// this window, so while it has focus every hook-sourced feature — the multimedia
// top row, the brightness keys, the Shift modifier, Hypershift — is dead unless
// the window reports the keys itself. It forwards them to `forward_key`, which
// runs the *same* dispatch the hook runs; none of the mapping logic lives here.
//
// Two things have to be decided locally, because a Tauri command is async and a
// keystroke cannot be swallowed after the fact:
//
//  - whether Fn is held. Fn is never a virtual key, so it arrives as a pushed
//    `fn-state` event from the HID reader rather than from a `keydown`.
//  - whether the runtime is going to consume this key. That is computed from the
//    same rules the runtime applies, so `preventDefault` happens in the same tick
//    and Fn+D does not also type "d".

import * as ipc from "./ipc";
import { keyMap } from "./keymap.svelte";
import { store } from "./store.svelte";

/** The top row, F1–F12, as Windows virtual-key codes. */
export const VK_F1 = 0x70;
export const VK_F12 = 0x7b;

/**
 * Modifiers the runtime tracks globally, as left/right virtual-key codes.
 *
 * These have to be forwarded even though this window never acts on them itself:
 * Shift is what makes the cycling controls run backwards, and Alt is what lets
 * Alt+F4 stay Alt+F4 instead of becoming the top row's action. Both are read by
 * the runtime when a *Razer* key arrives, which never passes through this window
 * at all — so leaving them stale breaks Shift+M1 just as surely as Shift+F11.
 */
export const VK_LSHIFT = 0xa0;
export const VK_RSHIFT = 0xa1;
export const VK_LALT = 0xa4;
export const VK_RALT = 0xa5;
const MODIFIERS: ReadonlySet<number> = new Set([VK_LSHIFT, VK_RSHIFT, VK_LALT, VK_RALT]);

/** Whether the runtime tracks this key as a modifier rather than an action. */
export const isModifier = (keyCode: number): boolean => MODIFIERS.has(keyCode);

/**
 * Everything the two decisions below depend on, gathered so they stay pure —
 * they mirror rules that live in the runtime, and a rule that cannot be tested
 * on its own is a rule that drifts.
 */
export interface KeyFacts {
  keyCode: number;
  /** Fn, as reported by the HID reader. Never a virtual key. */
  fnPressed: boolean;
  altPressed: boolean;
  primaryMultimediaKeys: boolean;
  hypershiftKeys: readonly number[];
}

/**
 * Whether this key is the runtime's business at all. Everything else is left to
 * the page, so typing a label or a command still works.
 *
 * None of this depends on what has focus. The hook sees these keys in every other
 * app, text fields included, so a focused toggle, slider or input here must not
 * be the one place the top row stops being media keys. Keyboard navigation is
 * unaffected: Tab, the arrows and Space are never the runtime's keys, and an F-key
 * the runtime does not consume keeps its default.
 */
export function isRuntimeKey(facts: KeyFacts): boolean {
  // Modifiers are state the runtime needs for keys that never reach this window,
  // and forwarding one never consumes it, so Shift still capitalises.
  if (isModifier(facts.keyCode)) return true;
  if (facts.fnPressed) return true;
  return facts.keyCode >= VK_F1 && facts.keyCode <= VK_F12;
}

/**
 * Mirrors the runtime's own decision, so a keystroke is swallowed in the same
 * tick it is forwarded — a Tauri command resolves far too late for that.
 *
 * Hypershift consumes a key only when the user bound it. The top row consumes one
 * when the media/function preference and Fn disagree, which is the `IsXOR` rule
 * in the runtime's key map, and is also why Fn+F5 deliberately passes through as
 * F5 when the top row already sends media keys.
 */
export function runtimeWillConsume(facts: KeyFacts): boolean {
  // Reporting a modifier must never eat it, or Shift would stop capitalising and
  // Alt would stop reaching menus.
  if (isModifier(facts.keyCode)) return false;

  if (facts.fnPressed && facts.hypershiftKeys.includes(facts.keyCode)) return true;
  if (facts.keyCode < VK_F1 || facts.keyCode > VK_F12) return false;
  // Mirrors the runtime's `IsFalse(ALT_PRESSED)` guard, so Alt+F4 stays Alt+F4
  // rather than being swallowed as the top row's action.
  if (facts.altPressed) return false;
  return facts.primaryMultimediaKeys !== facts.fnPressed;
}

class Hotkeys {
  /** Whether Fn is physically held, as reported by the HID reader. */
  fnPressed = $state(false);

  #unlisten: Array<() => void> = [];
  /** Modifiers reported as held, so losing focus cannot leave one stuck down. */
  #held = new Set<number>();
  #onKeyDown = (event: KeyboardEvent) => this.#handle(event, true);
  #onKeyUp = (event: KeyboardEvent) => this.#handle(event, false);
  #onBlur = () => this.#releaseModifiers();

  async start() {
    this.#unlisten.push(
      await ipc.onFnState((pressed) => {
        this.fnPressed = pressed;
      }),
    );
    // Capture phase, so a key destined for the runtime is claimed before a
    // control on the page can act on it.
    window.addEventListener("keydown", this.#onKeyDown, true);
    window.addEventListener("keyup", this.#onKeyUp, true);
    // A modifier released after focus moved away never reaches this window, and
    // a Shift the runtime still believes is held would cycle every control
    // backwards from then on.
    window.addEventListener("blur", this.#onBlur);
  }

  stop() {
    window.removeEventListener("keydown", this.#onKeyDown, true);
    window.removeEventListener("keyup", this.#onKeyUp, true);
    window.removeEventListener("blur", this.#onBlur);
    this.#releaseModifiers();
    for (const unlisten of this.#unlisten) unlisten();
    this.#unlisten = [];
    this.fnPressed = false;
  }

  #handle(event: KeyboardEvent, pressed: boolean) {
    // A cell waiting for a key press owns the keyboard, and the Keys page
    // captures Hypershift itself; forwarding would bind a key and run its old
    // action in the same breath.
    if (keyMap.capturing) return;
    // The runtime tracks auto-repeat itself, and a held key must not queue one
    // command per repeat.
    if (event.repeat) return;

    const keyCode = virtualKey(event);
    if (keyCode === null) return;

    const facts: KeyFacts = {
      keyCode,
      fnPressed: this.fnPressed,
      altPressed: event.altKey,
      primaryMultimediaKeys: store.state?.primary_multimedia_keys ?? false,
      hypershiftKeys: store.state?.key_bindings.hypershift.map((row) => row.key_code) ?? [],
    };
    if (!isRuntimeKey(facts)) return;

    // Decided here rather than from the command's result: the command resolves a
    // tick later, far too late to stop the character being typed.
    if (pressed && runtimeWillConsume(facts)) event.preventDefault();

    if (isModifier(keyCode)) {
      if (pressed) this.#held.add(keyCode);
      else this.#held.delete(keyCode);
    }

    void ipc.forwardKey(keyCode, pressed).catch(() => {
      // A dropped key is not worth a visible error; the next press retries.
    });
  }

  #releaseModifiers() {
    for (const keyCode of this.#held) {
      void ipc.forwardKey(keyCode, false).catch(() => {});
    }
    this.#held.clear();
  }

}

/**
 * The key as a Windows virtual-key code, or null if it has none.
 *
 * `code` is preferred over `key` for letters and digits because it is layout- and
 * modifier-independent: Shift+2 must still forward as `2`, not as `@`.
 */
export function virtualKey(event: Pick<KeyboardEvent, "code" | "key">): number | null {
  const { code, key } = event;
  if (code === "ShiftLeft") return VK_LSHIFT;
  if (code === "ShiftRight") return VK_RSHIFT;
  if (code === "AltLeft") return VK_LALT;
  if (code === "AltRight") return VK_RALT;
  if (/^F([1-9]|1[0-2])$/.test(key)) return VK_F1 + Number(key.slice(1)) - 1;
  if (/^Key[A-Z]$/.test(code)) return code.charCodeAt(3);
  if (/^Digit[0-9]$/.test(code)) return code.charCodeAt(5);
  if (/^[0-9]$/.test(key)) return key.charCodeAt(0);
  if (/^[a-zA-Z]$/.test(key)) return key.toUpperCase().charCodeAt(0);
  return null;
}

export const hotkeys = new Hotkeys();
