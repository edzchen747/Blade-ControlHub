import { describe, expect, it } from "vitest";

import {
  NOTHING_HIDDEN,
  dashboardItems,
  itemKey,
  itemSource,
  repointedControls,
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
  enabled: false,
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
    const items = dashboardItems(captures, [control({ enabled: true })]);

    expect(items[0]).toMatchObject({ kind: "control", name: "Snap Tap", enabled: true });
    expect(items.filter((item) => item.kind === "capture").every((item) => !item.enabled)).toBe(
      true,
    );
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
