// What the Dashboard's Custom Controls section lists, as plain functions over
// plain data.
//
// The section's two rules are the ones easiest to break by accident — that
// everything the user has made shows up unless they said otherwise, and that a
// capture and a control sharing a name are still two different things — so they
// live here rather than inside the page, where nothing could test them.

import type { CapturedCommand, CustomToggle, HiddenDashboardControls } from "./types";

/** One line of the section: a custom control, or a bare capture to replay. */
export interface DashboardItem {
  kind: "control" | "capture";
  name: string;
  /** A control's remembered side; a capture has no state of its own. */
  enabled: boolean;
  hidden: boolean;
}

export const NOTHING_HIDDEN: HiddenDashboardControls = { captures: [], controls: [] };

/** Identifies a line, so a capture and a control of the same name stay apart. */
export const itemKey = (item: DashboardItem): string => `${item.kind}:${item.name}`;

/**
 * Where a line comes from, named after what it does. Two lines can carry the
 * same name, so while editing this is the only thing telling them apart.
 */
export const itemSource = (item: DashboardItem): string =>
  item.kind === "control" ? "Control toggle" : "Replay capture";

/**
 * Everything the user has built, controls first: a control is the finished
 * thing, and a capture is the raw material it is made of. Captures are sorted
 * by name; controls keep the order they were created in, which is the order the
 * Command Lab page shows them.
 *
 * A half-built control, or one whose capture has since been deleted, is left
 * out — the Command Lab page explains those, and here it would only be a switch
 * that does nothing.
 *
 * Hiding is read as the exception rather than the rule, so anything the user
 * records later appears on its own instead of waiting to be added.
 */
export function dashboardItems(
  captures: Record<string, CapturedCommand[]>,
  controls: CustomToggle[],
  hidden: HiddenDashboardControls = NOTHING_HIDDEN,
): DashboardItem[] {
  const usable: DashboardItem[] = controls
    .filter(
      (toggle) =>
        toggle.name.trim() !== "" &&
        toggle.on_capture in captures &&
        toggle.off_capture in captures,
    )
    .map((toggle) => ({
      kind: "control",
      name: toggle.name,
      enabled: toggle.enabled,
      hidden: hidden.controls.includes(toggle.name),
    }));

  const replays: DashboardItem[] = Object.keys(captures)
    .sort()
    .map((name) => ({
      kind: "capture",
      name,
      enabled: false,
      hidden: hidden.captures.includes(name),
    }));

  return [...usable, ...replays];
}

/**
 * The hidden list with one line switched on or off, or `null` when it already
 * said that — so an unchanged list is never written back to the runtime.
 */
export function withItemShown(
  hidden: HiddenDashboardControls,
  item: DashboardItem,
  shown: boolean,
): HiddenDashboardControls | null {
  const next: HiddenDashboardControls = {
    captures: [...hidden.captures],
    controls: [...hidden.controls],
  };
  const list = item.kind === "control" ? next.controls : next.captures;
  const at = list.indexOf(item.name);

  if (shown && at !== -1) list.splice(at, 1);
  else if (!shown && at === -1) list.push(item.name);
  else return null;

  return next;
}

/**
 * The controls with a renamed capture followed through, or cleared when `to` is
 * empty because the capture was deleted. Returns `null` when none of them named
 * it, so a rename that concerns no control writes nothing.
 */
export function repointedControls(
  controls: CustomToggle[],
  from: string,
  to: string,
): CustomToggle[] | null {
  if (!controls.some((c) => c.on_capture === from || c.off_capture === from)) return null;

  return controls.map((control) => ({
    ...control,
    on_capture: control.on_capture === from ? to : control.on_capture,
    off_capture: control.off_capture === from ? to : control.off_capture,
  }));
}
