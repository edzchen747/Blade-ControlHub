// The window's keyboard path mirrors rules that live in the runtime, so these
// tests pin the mirror. Each one names the hardware behaviour it protects:
// getting one of these wrong is silent, and shows up as a key that does nothing.

import { describe, expect, it } from "vitest";

import {
  VK_F1,
  VK_F12,
  VK_LALT,
  VK_LSHIFT,
  VK_RALT,
  VK_RSHIFT,
  isModifier,
  isRuntimeKey,
  runtimeWillConsume,
  virtualKey,
  type KeyFacts,
} from "./hotkeys.svelte";

const VK_D = 0x44;

function facts(overrides: Partial<KeyFacts> = {}): KeyFacts {
  return {
    keyCode: VK_D,
    fnPressed: false,
    altPressed: false,
    typing: false,
    primaryMultimediaKeys: true,
    hypershiftKeys: [],
    ...overrides,
  };
}

describe("virtualKey", () => {
  it("maps both halves of Shift and Alt to their own codes", () => {
    // The runtime tracks them separately: releasing one while the other is held
    // must not clear the modifier.
    expect(virtualKey({ code: "ShiftLeft", key: "Shift" })).toBe(VK_LSHIFT);
    expect(virtualKey({ code: "ShiftRight", key: "Shift" })).toBe(VK_RSHIFT);
    expect(virtualKey({ code: "AltLeft", key: "Alt" })).toBe(VK_LALT);
    expect(virtualKey({ code: "AltRight", key: "Alt" })).toBe(VK_RALT);
  });

  it("maps the whole top row", () => {
    expect(virtualKey({ code: "F1", key: "F1" })).toBe(VK_F1);
    expect(virtualKey({ code: "F9", key: "F9" })).toBe(0x78);
    expect(virtualKey({ code: "F12", key: "F12" })).toBe(VK_F12);
  });

  it("does not mistake F13 and above for the top row", () => {
    // F13–F24 exist as virtual keys but have no built-in action, and reading
    // "F13" as F1 would fire the wrong one.
    expect(virtualKey({ code: "F13", key: "F13" })).toBeNull();
    expect(virtualKey({ code: "F24", key: "F24" })).toBeNull();
  });

  it("reads letters and digits from the physical key, not the character", () => {
    // Shift+2 is "@" on a US layout; the runtime binds the key, not the glyph.
    expect(virtualKey({ code: "KeyD", key: "d" })).toBe(VK_D);
    expect(virtualKey({ code: "KeyD", key: "D" })).toBe(VK_D);
    expect(virtualKey({ code: "Digit2", key: "@" })).toBe(0x32);
    expect(virtualKey({ code: "Digit0", key: ")" })).toBe(0x30);
  });

  it("still resolves a letter when the layout reports no usable code", () => {
    expect(virtualKey({ code: "", key: "k" })).toBe(0x4b);
    expect(virtualKey({ code: "", key: "7" })).toBe(0x37);
  });

  it("has no code for keys the runtime never maps", () => {
    expect(virtualKey({ code: "Tab", key: "Tab" })).toBeNull();
    expect(virtualKey({ code: "Escape", key: "Escape" })).toBeNull();
    expect(virtualKey({ code: "Space", key: " " })).toBeNull();
  });
});

describe("isRuntimeKey", () => {
  it("forwards modifiers even while the user is typing", () => {
    // Shift is what makes the cycling controls run backwards, including for a
    // Razer key that never reaches this window at all — so the runtime needs it
    // whatever the focus is doing.
    for (const keyCode of [VK_LSHIFT, VK_RSHIFT, VK_LALT, VK_RALT]) {
      expect(isRuntimeKey(facts({ keyCode, typing: true }))).toBe(true);
    }
  });

  it("forwards anything pressed with Fn", () => {
    expect(isRuntimeKey(facts({ keyCode: VK_D, fnPressed: true }))).toBe(true);
  });

  it("leaves ordinary typing alone", () => {
    expect(isRuntimeKey(facts({ keyCode: VK_D }))).toBe(false);
  });

  it("does not steal the top row from a text field", () => {
    expect(isRuntimeKey(facts({ keyCode: VK_F1 }))).toBe(true);
    expect(isRuntimeKey(facts({ keyCode: VK_F1, typing: true }))).toBe(false);
  });

  it("forwards Fn combos even from a text field", () => {
    // Fn+D is a Hypershift press wherever the caret happens to be.
    expect(isRuntimeKey(facts({ keyCode: VK_D, fnPressed: true, typing: true }))).toBe(true);
  });
});

describe("runtimeWillConsume", () => {
  it("never consumes a modifier", () => {
    // Consuming Shift would stop it capitalising and stop Alt reaching menus.
    for (const keyCode of [VK_LSHIFT, VK_RSHIFT, VK_LALT, VK_RALT]) {
      expect(runtimeWillConsume(facts({ keyCode }))).toBe(false);
      expect(runtimeWillConsume(facts({ keyCode, fnPressed: true }))).toBe(false);
    }
  });

  it("swallows a bound Hypershift key so it does not also type", () => {
    const bound = facts({ keyCode: VK_D, fnPressed: true, hypershiftKeys: [VK_D] });
    expect(runtimeWillConsume(bound)).toBe(true);
  });

  it("lets an unbound Fn combo through", () => {
    // Fn+K with nothing bound to K must still type "k".
    const unbound = facts({ keyCode: 0x4b, fnPressed: true, hypershiftKeys: [VK_D] });
    expect(runtimeWillConsume(unbound)).toBe(false);
  });

  it("applies the media/function rule as an exclusive or", () => {
    // This is the runtime's `IsXOR(PRIMARY_MULTIMEDIA_KEYS, FN_PRESSED)`.
    const topRow = (primaryMultimediaKeys: boolean, fnPressed: boolean) =>
      runtimeWillConsume(facts({ keyCode: VK_F1, primaryMultimediaKeys, fnPressed }));

    expect(topRow(true, false)).toBe(true); // media primary: F1 is the action
    expect(topRow(true, true)).toBe(false); // Fn+F1 passes through as F1
    expect(topRow(false, false)).toBe(false); // F-keys primary: F1 is F1
    expect(topRow(false, true)).toBe(true); // Fn+F1 is the action
  });

  it("leaves Alt+F4 to Windows", () => {
    // Mirrors `IsFalse(ALT_PRESSED)`: without this, Alt+F4 would change a device
    // setting and be swallowed, so the window could not be closed.
    const altF4 = facts({ keyCode: 0x73, altPressed: true, primaryMultimediaKeys: true });
    expect(runtimeWillConsume(altF4)).toBe(false);
    expect(runtimeWillConsume({ ...altF4, altPressed: false })).toBe(true);
  });

  it("still runs a bound Hypershift key when Alt happens to be held", () => {
    // The Alt guard belongs to the top row, not to Hypershift.
    const withAlt = facts({
      keyCode: VK_D,
      fnPressed: true,
      altPressed: true,
      hypershiftKeys: [VK_D],
    });
    expect(runtimeWillConsume(withAlt)).toBe(true);
  });

  it("ignores a key outside the top row that nothing is bound to", () => {
    expect(runtimeWillConsume(facts({ keyCode: VK_D }))).toBe(false);
    expect(runtimeWillConsume(facts({ keyCode: 0x7c }))).toBe(false);
  });

  it("covers the whole top row, both ends included", () => {
    for (const keyCode of [VK_F1, 0x75, VK_F12]) {
      expect(runtimeWillConsume(facts({ keyCode, primaryMultimediaKeys: true }))).toBe(true);
    }
    // One past each end is not the top row.
    expect(runtimeWillConsume(facts({ keyCode: VK_F1 - 1 }))).toBe(false);
    expect(runtimeWillConsume(facts({ keyCode: VK_F12 + 1 }))).toBe(false);
  });
});

describe("isModifier", () => {
  it("names exactly the four keys the runtime tracks", () => {
    expect([VK_LSHIFT, VK_RSHIFT, VK_LALT, VK_RALT].every(isModifier)).toBe(true);
    // Generic VK_SHIFT/VK_MENU are never reported by `code`, and Ctrl and Win
    // are not modifiers the runtime tracks at all.
    expect(isModifier(0x10)).toBe(false);
    expect(isModifier(0xa2)).toBe(false);
    expect(isModifier(VK_D)).toBe(false);
  });
});
