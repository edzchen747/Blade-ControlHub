// A rename in Command Lab has to reach the key bindings through the Keys page's
// own rows. Those rows are seeded once and are what every later save writes, so
// a fix that went around them would be saved over by the next edit.

import { beforeEach, describe, expect, it, vi } from "vitest";

import type { KeyBindings } from "./types";

const { setKeyBindings } = vi.hoisted(() => ({
  setKeyBindings: vi.fn(async (_bindings: KeyBindings) => {}),
}));

vi.mock("./ipc", () => ({ setKeyBindings, errorMessage: String }));

import { keyMap } from "./keymap.svelte";

beforeEach(() => {
  setKeyBindings.mockClear();
  keyMap.razer = [
    { id: 1, name: "M1", keyCode: 0x24, action: { kind: "replay_capture", name: "snap tap on" } },
    {
      id: 2,
      name: "M2",
      keyCode: 0x25,
      action: { kind: "toggle_custom_control", name: "Snap Tap" },
    },
  ];
  keyMap.hypershift = [
    { id: 3, name: "", keyCode: 0x4b, action: { kind: "replay_capture", name: "snap tap on" } },
  ];
});

describe("key bindings following Command Lab", () => {
  it("renames the rows themselves, in both tables", () => {
    keyMap.followCommandLab("replay_capture", "snap tap on", "snap tap engaged");

    expect(keyMap.razer[0]!.action).toEqual({ kind: "replay_capture", name: "snap tap engaged" });
    expect(keyMap.hypershift[0]!.action).toEqual({
      kind: "replay_capture",
      name: "snap tap engaged",
    });
    // Same-named control of the other kind is untouched.
    expect(keyMap.razer[1]!.action).toEqual({ kind: "toggle_custom_control", name: "Snap Tap" });
  });

  // Written straight away, not on the typing debounce: nothing here is being
  // typed, and the window may close before a debounce would fire.
  it("writes the renamed bindings to the runtime at once", () => {
    keyMap.followCommandLab("toggle_custom_control", "Snap Tap", "Snap Tap Pro");

    expect(setKeyBindings).toHaveBeenCalledTimes(1);
    expect(setKeyBindings.mock.calls[0]![0].razer[1]!.action).toEqual({
      kind: "toggle_custom_control",
      name: "Snap Tap Pro",
    });
  });

  // The row survives a delete with its key and label, cleared for repointing.
  it("clears the target on a delete but keeps the row", () => {
    keyMap.followCommandLab("replay_capture", "snap tap on", "");

    expect(keyMap.razer[0]).toMatchObject({
      name: "M1",
      keyCode: 0x24,
      action: { kind: "replay_capture", name: "" },
    });
  });

  it("writes nothing when no binding pointed at it", () => {
    keyMap.followCommandLab("replay_capture", "never bound", "renamed");

    expect(setKeyBindings).not.toHaveBeenCalled();
  });
});
