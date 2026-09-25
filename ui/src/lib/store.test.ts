import { describe, expect, it } from "vitest";

import { followLiveProfile } from "./store.svelte";

describe("which profile the window edits", () => {
  it("follows the live profile until the user picks one", () => {
    expect(followLiveProfile(null, "Ac", "Ac", false)).toEqual({ editing: "Ac", pinned: false });
    expect(followLiveProfile("Ac", "Ac", "Ac", false)).toEqual({ editing: "Ac", pinned: false });
  });

  // A snapshot for some unrelated change must not pull the user off the
  // profile they chose to edit.
  it("keeps the user's pick while the power source stays the same", () => {
    expect(followLiveProfile("Ac", "Ac", "Battery", true)).toEqual({
      editing: "Battery",
      pinned: true,
    });
  });

  // The reported behaviour: the machine changed power source under an open
  // window, so it moves to the profile now running, whatever was picked.
  it("switches to the live profile when the power source changes", () => {
    expect(followLiveProfile("Ac", "Battery", "Ac", true)).toEqual({
      editing: "Battery",
      pinned: false,
    });
    expect(followLiveProfile("Battery", "Ac", "Battery", true)).toEqual({
      editing: "Ac",
      pinned: false,
    });
  });

  // Once moved, the window goes back to following, so the next change is
  // followed too rather than only the first.
  it("forgets the pick once the power source has changed", () => {
    const afterUnplug = followLiveProfile("Ac", "Battery", "Battery", true);
    const afterReplug = followLiveProfile("Battery", "Ac", afterUnplug.editing, afterUnplug.pinned);

    expect(afterReplug).toEqual({ editing: "Ac", pinned: false });
  });
});
