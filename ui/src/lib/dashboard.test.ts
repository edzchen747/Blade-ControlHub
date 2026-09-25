import { describe, expect, it } from "vitest";

import {
  NOTHING_HIDDEN,
  captureToSync,
  dashboardItems,
  followRename,
  itemKey,
  itemSource,
  renamedInHidden,
  repointedControls,
  swappedCaptures,
  withItemShown,
  type DashboardItem,
} from "./dashboard";
import type { CapturedCommand, CustomToggle, HiddenDashboardControls } from "./types";

const command: CapturedCommand[] = [{ command: 0x0792, args: [0x01] }];

const captures: Record<string, CapturedCommand[]> = {
  "snap tap on": command,
  "snap tap off": command,
  "game mode on": command,
  "game mode off": command,
};

const control = (fields: Partial<CustomToggle> = {}): CustomToggle => ({
  name: "Snap Tap",
  on_capture: "snap tap on",
  off_capture: "snap tap off",
  ac_enabled: false,
  battery_enabled: false,
  ...fields,
});

const names = (items: DashboardItem[]) => items.map(itemKey);

describe("what the Dashboard lists", () => {
  // The whole point of storing the exceptions rather than the inclusions: a
  // capture recorded after the user last edited the section must not need them
  // to go back and admit it.
  it("shows everything by default, including one recorded later", () => {
    const items = dashboardItems(captures, [control()]);

    expect(items.every((item) => !item.hidden)).toBe(true);
    expect(names(items)).toEqual([
      "control:Snap Tap",
      "capture:game mode off",
      "capture:game mode on",
      "capture:snap tap off",
      "capture:snap tap on",
    ]);

    const later = dashboardItems({ ...captures, "lid logo": command }, [control()], {
      captures: ["snap tap on"],
      controls: [],
    });

    expect(later.find((item) => item.name === "lid logo")?.hidden).toBe(false);
  });

  it("puts the finished controls before the raw captures", () => {
    const items = dashboardItems(captures, [
      control(),
      control({ name: "Game Mode", on_capture: "game mode on", off_capture: "game mode off" }),
    ]);

    expect(items.slice(0, 2).map((item) => item.name)).toEqual(["Snap Tap", "Game Mode"]);
    expect(items.slice(2).every((item) => item.kind === "capture")).toBe(true);
  });

  // A switch that would do nothing is worse than no switch: the Command Lab
  // page is where an unfinished control gets explained.
  it("leaves out a control that is unfinished or points at a deleted capture", () => {
    const items = dashboardItems(captures, [
      control({ name: "   " }),
      control({ name: "No on", on_capture: "" }),
      control({ name: "No off", off_capture: "" }),
      control({ name: "Dangling", on_capture: "recorded then deleted" }),
      control({ name: "Fine" }),
    ]);

    expect(items.filter((item) => item.kind === "control").map((item) => item.name)).toEqual([
      "Fine",
    ]);
  });

  it("carries a control's remembered side, and gives a capture none", () => {
    const items = dashboardItems(captures, [control({ ac_enabled: true })], NOTHING_HIDDEN, "Ac");

    expect(items[0]).toMatchObject({ kind: "control", name: "Snap Tap", enabled: true });
    expect(items.filter((item) => item.kind === "capture").every((item) => !item.enabled)).toBe(
      true,
    );
  });

  // The point of splitting the side per profile: the switch must show what the
  // profile being edited was left on, not what the other one was.
  it("shows the side belonging to the profile it was asked for", () => {
    const split = [control({ ac_enabled: true, battery_enabled: false })];

    expect(dashboardItems(captures, split, NOTHING_HIDDEN, "Ac")[0]!.enabled).toBe(true);
    expect(dashboardItems(captures, split, NOTHING_HIDDEN, "Battery")[0]!.enabled).toBe(false);
  });

  // Hiding is a choice about the page, not about the machine, so it does not
  // follow the profile the way a control's side does.
  it("hides the same lines whichever profile is being edited", () => {
    const hidden: HiddenDashboardControls = { captures: [], controls: ["Snap Tap"] };
    const onAc = dashboardItems(captures, [control()], hidden, "Ac");
    const onBattery = dashboardItems(captures, [control()], hidden, "Battery");

    expect(onAc.map((item) => item.hidden)).toEqual(onBattery.map((item) => item.hidden));
    expect(onAc[0]!.hidden).toBe(true);
  });

  it("sorts the captures by name so the list does not reshuffle itself", () => {
    const shuffled = { zebra: command, apple: command, mango: command };

    expect(dashboardItems(shuffled, []).map((item) => item.name)).toEqual([
      "apple",
      "mango",
      "zebra",
    ]);
  });
});

describe("hiding a line", () => {
  // A capture and a control may share a name without being the same thing, so
  // the two lists must never be consulted for each other.
  it("keeps a capture and a control of the same name apart", () => {
    const sameName = { "Snap Tap": command };
    const hidden: HiddenDashboardControls = { captures: ["Snap Tap"], controls: [] };
    const items = dashboardItems(
      sameName,
      [control({ on_capture: "Snap Tap", off_capture: "Snap Tap" })],
      hidden,
    );

    expect(items.find((item) => item.kind === "control")?.hidden).toBe(false);
    expect(items.find((item) => item.kind === "capture")?.hidden).toBe(true);
    expect(itemKey(items[0]!)).not.toBe(itemKey(items[1]!));
  });

  // While editing, two lines of the same name are told apart by this alone.
  it("names where each line came from", () => {
    expect(itemSource(items("control", "Snap Tap"))).toBe("Control toggle");
    expect(itemSource(items("capture", "Snap Tap"))).toBe("Replay capture");
  });

  it("writes the name to the list its kind belongs to", () => {
    const asControl = withItemShown(NOTHING_HIDDEN, items("control", "Snap Tap"), false);
    const asCapture = withItemShown(NOTHING_HIDDEN, items("capture", "Snap Tap"), false);

    expect(asControl).toEqual({ captures: [], controls: ["Snap Tap"] });
    expect(asCapture).toEqual({ captures: ["Snap Tap"], controls: [] });
  });

  it("removes only the line being shown again", () => {
    const hidden: HiddenDashboardControls = {
      captures: ["a", "b"],
      controls: ["Snap Tap"],
    };

    expect(withItemShown(hidden, items("capture", "a"), true)).toEqual({
      captures: ["b"],
      controls: ["Snap Tap"],
    });
  });

  // Every write goes to the device thread and back, so a checkbox that already
  // said what it was set to must not queue one.
  it("reports no change when the list already says so", () => {
    const hidden: HiddenDashboardControls = { captures: ["a"], controls: [] };

    expect(withItemShown(hidden, items("capture", "a"), false)).toBeNull();
    expect(withItemShown(hidden, items("capture", "b"), true)).toBeNull();
  });

  it("does not mutate the list it was given", () => {
    const hidden: HiddenDashboardControls = { captures: ["a"], controls: [] };
    withItemShown(hidden, items("capture", "b"), false);

    expect(hidden).toEqual({ captures: ["a"], controls: [] });
  });

  function items(kind: DashboardItem["kind"], name: string): DashboardItem {
    return { kind, name, enabled: false, hidden: false };
  }
});

describe("following a capture through a rename", () => {
  // A control names its captures, so a rename that did not follow would leave
  // the control pointing at something that no longer exists.
  it("renames both sides, and only the side that matched", () => {
    const renamed = repointedControls([control()], "snap tap on", "snap tap engaged");

    expect(renamed?.[0]).toMatchObject({
      on_capture: "snap tap engaged",
      off_capture: "snap tap off",
    });
  });

  it("renames a control that used one capture for both sides", () => {
    const both = control({ on_capture: "same", off_capture: "same" });
    const renamed = repointedControls([both], "same", "renamed");

    expect(renamed?.[0]).toMatchObject({ on_capture: "renamed", off_capture: "renamed" });
  });

  it("clears the side when the capture was deleted", () => {
    const cleared = repointedControls([control()], "snap tap on", "");

    expect(cleared?.[0]).toMatchObject({ on_capture: "", off_capture: "snap tap off" });
  });

  it("reports no change when no control named it, and leaves the rest alone", () => {
    const controls = [control(), control({ name: "Other", on_capture: "game mode on" })];

    expect(repointedControls(controls, "never used", "x")).toBeNull();
    expect(repointedControls(controls, "snap tap on", "x")?.[1]).toEqual(controls[1]);
  });

  it("does not mutate the controls it was given", () => {
    const controls = [control()];
    repointedControls(controls, "snap tap on", "renamed");

    expect(controls[0]!.on_capture).toBe("snap tap on");
  });
});

describe("keeping the device where the switch says it is", () => {
  const on = (fields: Partial<CustomToggle> = {}) => control({ ac_enabled: true, ...fields });
  const off = (fields: Partial<CustomToggle> = {}) => control({ ac_enabled: false, ...fields });

  // The reported mismatch: a control switched on, given its captures, and left
  // reading on while the device was never told.
  it("replays the switch's side the moment a control becomes usable", () => {
    const unfinished = on({ off_capture: "" });

    expect(captureToSync(unfinished, on(), "Ac")).toBe("snap tap on");
    expect(captureToSync(off({ on_capture: "" }), off(), "Ac")).toBe("snap tap off");
  });

  it("replays when the capture for the switch's side changes", () => {
    expect(captureToSync(on(), on({ on_capture: "game mode on" }), "Ac")).toBe("game mode on");
  });

  // The device is already where the switch says, so nothing is sent.
  it("replays nothing when the other side changes", () => {
    expect(captureToSync(on(), on({ off_capture: "game mode off" }), "Ac")).toBeNull();
    expect(captureToSync(off(), off({ on_capture: "game mode on" }), "Ac")).toBeNull();
  });

  it("replays nothing while the control is still unusable", () => {
    expect(captureToSync(on({ on_capture: "" }), on({ on_capture: "" }), "Ac")).toBeNull();
    // Both sides the same capture is flagged on the page and cannot switch.
    const same = on({ on_capture: "snap tap on", off_capture: "snap tap on" });
    expect(captureToSync(on({ off_capture: "" }), same, "Ac")).toBeNull();
  });

  // The switch belongs to the running profile on this page, so it is that
  // profile's side the device is brought to.
  it("brings the device to the side of the profile it was asked about", () => {
    const split = control({ ac_enabled: true, battery_enabled: false });
    const unfinished = { ...split, off_capture: "" };

    expect(captureToSync(unfinished, split, "Ac")).toBe("snap tap on");
    expect(captureToSync(unfinished, split, "Battery")).toBe("snap tap off");
  });
});

describe("swapping a control's captures", () => {
  it("swaps the two captures and leaves the switch where it is", () => {
    const swapped = swappedCaptures(control({ ac_enabled: true, battery_enabled: false }));

    expect(swapped).toMatchObject({
      on_capture: "snap tap off",
      off_capture: "snap tap on",
      ac_enabled: true,
      battery_enabled: false,
    });
  });

  // A swap is an assignment like any other: the switch stays put, so the
  // device has to follow it onto the capture that now sits on that side.
  it("moves the device onto the capture now on the switch's side", () => {
    const before = control({ ac_enabled: true });

    expect(captureToSync(before, swappedCaptures(before), "Ac")).toBe("snap tap off");
    expect(captureToSync(control(), swappedCaptures(control()), "Ac")).toBe("snap tap on");
  });

  it("keeps anything else the row carries", () => {
    const row = { ...control(), id: 7 };

    expect(swappedCaptures(row).id).toBe(7);
  });
});

describe("hiding follows a line through a rename", () => {
  const hidden: HiddenDashboardControls = {
    captures: ["snap tap on", "game mode on"],
    controls: ["Snap Tap"],
  };

  // The reported bug: renaming a hidden capture put it back on the Dashboard.
  it("keeps a hidden capture hidden under its new name", () => {
    expect(renamedInHidden(hidden, "capture", "snap tap on", "snap tap engaged")).toEqual({
      captures: ["game mode on", "snap tap engaged"],
      controls: ["Snap Tap"],
    });
  });

  it("keeps a hidden control hidden under its new name", () => {
    expect(renamedInHidden(hidden, "control", "Snap Tap", "Snap Tap Pro")?.controls).toEqual([
      "Snap Tap Pro",
    ]);
  });

  // A deleted line's name left behind would hide whatever next took that name.
  it("drops the name when the line is deleted", () => {
    expect(renamedInHidden(hidden, "capture", "snap tap on", "")?.captures).toEqual([
      "game mode on",
    ]);
  });

  it("writes nothing when the renamed line was not hidden", () => {
    expect(renamedInHidden(hidden, "capture", "visible", "still visible")).toBeNull();
  });

  // The same name can be a capture and a control; a rename of one is not a
  // rename of the other.
  it("leaves the other kind's list alone, even under the same name", () => {
    const shared: HiddenDashboardControls = { captures: ["Snap Tap"], controls: ["Snap Tap"] };

    expect(renamedInHidden(shared, "capture", "Snap Tap", "renamed")).toEqual({
      captures: ["renamed"],
      controls: ["Snap Tap"],
    });
  });

  it("does not list a name twice when renamed onto one already hidden", () => {
    expect(renamedInHidden(hidden, "capture", "snap tap on", "game mode on")?.captures).toEqual([
      "game mode on",
    ]);
  });

  it("does not mutate the list it was given", () => {
    renamedInHidden(hidden, "capture", "snap tap on", "renamed");

    expect(hidden.captures).toEqual(["snap tap on", "game mode on"]);
  });
});

// End to end at the data level: the rename flow Command Lab runs, then the list
// the Dashboard builds from the result. This is the reported bug as the user
// saw it — the renamed line reappearing on the Dashboard.
describe("renaming in Command Lab, as the Dashboard then sees it", () => {
  const hidden: HiddenDashboardControls = { captures: ["snap tap on"], controls: ["Snap Tap"] };

  it("keeps a hidden capture off the Dashboard after it is renamed", () => {
    const { "snap tap on": _renamedAway, ...kept } = captures;
    const renamed = { ...kept, "snap tap engaged": command };
    const controls = repointedControls([control()], "snap tap on", "snap tap engaged")!;
    const nextHidden = renamedInHidden(hidden, "capture", "snap tap on", "snap tap engaged")!;

    const shown = dashboardItems(renamed, controls, nextHidden)
      .filter((item) => !item.hidden)
      .map(itemKey);

    expect(shown).not.toContain("capture:snap tap engaged");
    expect(shown).not.toContain("capture:snap tap on");
  });

  it("keeps a hidden control off the Dashboard after it is renamed", () => {
    const controls = [control({ name: "Snap Tap Pro" })];
    const nextHidden = renamedInHidden(hidden, "control", "Snap Tap", "Snap Tap Pro")!;

    const shown = dashboardItems(captures, controls, nextHidden)
      .filter((item) => !item.hidden)
      .map(itemKey);

    expect(shown).not.toContain("control:Snap Tap Pro");
  });

  // The reverse of the bug: a deleted hidden capture must not leave its name
  // behind to hide a new capture that happens to be given it.
  it("does not hide a new capture that reuses a deleted one's name", () => {
    const afterDelete = renamedInHidden(hidden, "capture", "snap tap on", "")!;
    const recorded = { ...captures, "snap tap on": command };

    const item = dashboardItems(recorded, [], afterDelete).find(
      (candidate) => candidate.name === "snap tap on",
    );

    expect(item?.hidden).toBe(false);
  });
});

describe("remembering a control's name through a rename", () => {
  it("carries a plain rename across", () => {
    expect(followRename("Snap Tap", "Snap Tap Pro", false)).toEqual({
      savedName: "Snap Tap Pro",
      rename: { from: "Snap Tap", to: "Snap Tap Pro" },
    });
  });

  it("carries nothing on a control's first name", () => {
    expect(followRename("", "Snap Tap", false)).toEqual({ savedName: "Snap Tap", rename: null });
  });

  it("carries nothing when the name did not change, ignoring stray spaces", () => {
    expect(followRename("Snap Tap", "  Snap Tap ", false)).toEqual({
      savedName: "Snap Tap",
      rename: null,
    });
  });

  // An empty or clashing name is only passing through; following it would put
  // the hidden entry on a name about to be replaced, or on another control.
  it("keeps the last good name through an empty or clashing one", () => {
    expect(followRename("Snap Tap", "   ", false)).toEqual({ savedName: "Snap Tap", rename: null });
    expect(followRename("Snap Tap", "Game Mode", true)).toEqual({
      savedName: "Snap Tap",
      rename: null,
    });
  });

  // The sequence the rule exists for: the user types a name that clashes, then
  // fixes it. The hidden entry must still move from the name that was actually
  // hidden, not from the clash in between.
  it("renames from the original name once a clash is fixed", () => {
    const clash = followRename("Snap Tap", "Game Mode", true);
    const fixed = followRename(clash.savedName, "Snap Tap Pro", false);

    expect(fixed.rename).toEqual({ from: "Snap Tap", to: "Snap Tap Pro" });
  });

  it("renames from the original name once an emptied name is filled back in", () => {
    const emptied = followRename("Snap Tap", "", false);
    const refilled = followRename(emptied.savedName, "Snap Tap Pro", false);

    expect(refilled.rename).toEqual({ from: "Snap Tap", to: "Snap Tap Pro" });
  });
});
