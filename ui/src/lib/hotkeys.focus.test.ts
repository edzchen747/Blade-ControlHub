// The top row has to keep working whatever the window has focused. It once went
// dead whenever a toggle, slider or text field was focused — they are all
// `<input>`s, and F1-F12 were held back from anything that looked editable —
// and only came back after clicking somewhere blank. These tests drive the real
// window listener with focused controls, so any check on the event's target that
// creeps back in shows up here rather than as a key that silently does nothing.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { forwardKey, fakeStore, fakeKeyMap } = vi.hoisted(() => ({
  forwardKey: vi.fn(async (_keyCode: number, _pressed: boolean) => true),
  fakeStore: {
    state: {
      primary_multimedia_keys: true,
      key_bindings: { hypershift: [] as { key_code: number }[] },
    },
  },
  fakeKeyMap: { capturing: false },
}));

vi.mock("./ipc", () => ({ forwardKey, onFnState: async () => () => {} }));
vi.mock("./store.svelte", () => ({ store: fakeStore }));
vi.mock("./keymap.svelte", () => ({ keyMap: fakeKeyMap }));

// The tests run without a DOM, so stand in just enough of one for a focused
// control to be told apart the way a browser would tell it apart.
class FakeHTMLElement {
  isContentEditable = false;
}
class FakeHTMLInputElement extends FakeHTMLElement {
  constructor(public type: string) {
    super();
  }
}
class FakeHTMLTextAreaElement extends FakeHTMLElement {}
class FakeHTMLSelectElement extends FakeHTMLElement {}
class FakeHTMLButtonElement extends FakeHTMLElement {}

type Listener = (event: unknown) => void;
const listeners = new Map<string, Listener[]>();
const fakeWindow = {
  addEventListener: (type: string, listener: Listener) => {
    listeners.set(type, [...(listeners.get(type) ?? []), listener]);
  },
  removeEventListener: (type: string, listener: Listener) => {
    listeners.set(type, (listeners.get(type) ?? []).filter((l) => l !== listener));
  },
};

vi.stubGlobal("window", fakeWindow);
vi.stubGlobal("HTMLElement", FakeHTMLElement);
vi.stubGlobal("HTMLInputElement", FakeHTMLInputElement);
vi.stubGlobal("HTMLTextAreaElement", FakeHTMLTextAreaElement);
vi.stubGlobal("HTMLSelectElement", FakeHTMLSelectElement);
vi.stubGlobal("HTMLButtonElement", FakeHTMLButtonElement);

import { VK_F1, VK_F12, hotkeys } from "./hotkeys.svelte";

/** Every kind of control the window can have focused while the user adjusts it. */
const FOCUSED: Array<[string, () => unknown]> = [
  ["nothing", () => null],
  ["a toggle", () => new FakeHTMLInputElement("checkbox")],
  ["a slider", () => new FakeHTMLInputElement("range")],
  ["a text field", () => new FakeHTMLInputElement("text")],
  ["a colour picker", () => new FakeHTMLInputElement("color")],
  ["a text area", () => new FakeHTMLTextAreaElement()],
  ["a dropdown", () => new FakeHTMLSelectElement()],
  ["a button", () => new FakeHTMLButtonElement()],
  [
    "an editable region",
    () => Object.assign(new FakeHTMLElement(), { isContentEditable: true }),
  ],
];

function press(type: "keydown" | "keyup", keyNumber: number, target: unknown) {
  const event = {
    type,
    code: `F${keyNumber}`,
    key: `F${keyNumber}`,
    repeat: false,
    altKey: false,
    target,
    preventDefault: vi.fn(),
  };
  for (const listener of listeners.get(type) ?? []) listener(event);
  return event;
}

beforeEach(async () => {
  forwardKey.mockClear();
  fakeStore.state.primary_multimedia_keys = true;
  fakeKeyMap.capturing = false;
  await hotkeys.start();
});

afterEach(() => {
  hotkeys.stop();
});

describe("the top row while a control has focus", () => {
  it.each(FOCUSED)("reaches the runtime with %s focused", (_name, target) => {
    for (let n = 1; n <= 12; n++) {
      forwardKey.mockClear();
      press("keydown", n, target());
      press("keyup", n, target());
      expect(forwardKey.mock.calls).toEqual([
        [VK_F1 + n - 1, true],
        [VK_F1 + n - 1, false],
      ]);
    }
  });

  it.each(FOCUSED)("is swallowed as a media key with %s focused", (_name, target) => {
    // Otherwise the control would also act on F-key defaults, like F5 reloading
    // the page out from under the user.
    const event = press("keydown", 12, target());
    expect(forwardKey).toHaveBeenCalledWith(VK_F12, true);
    expect(event.preventDefault).toHaveBeenCalled();
  });

  it.each(FOCUSED)("keeps its F-key default in function mode with %s focused", (_name, target) => {
    // The runtime will not act on it, so the window must not eat it either.
    fakeStore.state.primary_multimedia_keys = false;
    const event = press("keydown", 5, target());
    expect(forwardKey).toHaveBeenCalledWith(VK_F1 + 4, true);
    expect(event.preventDefault).not.toHaveBeenCalled();
  });

  it("still yields to a cell waiting for a key press", () => {
    // The one deliberate exception: the Keys page owns the keyboard while binding.
    fakeKeyMap.capturing = true;
    const event = press("keydown", 1, new FakeHTMLInputElement("text"));
    expect(forwardKey).not.toHaveBeenCalled();
    expect(event.preventDefault).not.toHaveBeenCalled();
  });
});
