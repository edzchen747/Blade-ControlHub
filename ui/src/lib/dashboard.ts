// What the Dashboard's Custom Controls section lists, as plain functions over
// plain data.
//
// The section's two rules are the ones easiest to break by accident — that
// everything the user has made shows up unless they said otherwise, and that a
// capture and a control sharing a name are still two different things — so they
// live here rather than inside the page, where nothing could test them.

import { toggleEnabled } from "./types";
import type {
  CapturedCommand,
  CustomToggle,
  HiddenDashboardControls,
  PowerProfile,
} from "./types";

/** One line of the section: a custom control, or a bare capture to replay. */
export interface DashboardItem {
  kind: "control" | "capture";
  name: string;
  /** A control's side for the profile asked for; a capture has no state. */
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
 * records later appears on its own instead of waiting to be added. Hiding is
 * not per profile: it is a choice about the page, not about the machine.
 *
 * A control's side comes from `profile`, so the switch shows what that profile
 * was left on rather than what the device happens to be doing.
 */
export function dashboardItems(
  captures: Record<string, CapturedCommand[]>,
  controls: CustomToggle[],
  hidden: HiddenDashboardControls = NOTHING_HIDDEN,
  profile: PowerProfile = "Ac",
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
      enabled: toggleEnabled(toggle, profile),
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
 * What a control's rename carries over: the name to remember from now on, and
 * the rename to apply to the hidden list, if any.
 *
 * `savedName` is the last good name the control was saved under. An empty or
 * clashing name is saved as it stands — the page flags it — but not followed,
 * so the last good name is kept and a later fix still renames from what was
 * actually hidden. A control's first name has nothing to carry over.
 */
export function followRename(
  savedName: string,
  typed: string,
  clashes: boolean,
): { savedName: string; rename: { from: string; to: string } | null } {
  const to = typed.trim();
  if (to === "" || clashes) return { savedName, rename: null };
  if (savedName === "" || savedName === to) return { savedName: to, rename: null };
  return { savedName: to, rename: { from: savedName, to } };
}

/**
 * The hidden list with a renamed line followed through, or the name dropped
 * when `to` is empty because the line was deleted. Returns `null` when the
 * line was not hidden, so renaming a visible line writes nothing.
 *
 * Hiding is keyed by name. Without this, renaming a hidden line would bring it
 * back onto the Dashboard, and deleting one would leave its name behind to hide
 * whatever is next given that name.
 */
export function renamedInHidden(
  hidden: HiddenDashboardControls,
  kind: DashboardItem["kind"],
  from: string,
  to: string,
): HiddenDashboardControls | null {
  const list = kind === "control" ? hidden.controls : hidden.captures;
  if (!list.includes(from)) return null;

  const next = list.filter((name) => name !== from);
  if (to !== "" && !next.includes(to)) next.push(to);

  return kind === "control" ? { ...hidden, controls: next } : { ...hidden, captures: next };
}

/**
 * The controls with a renamed capture followed through, or cleared when `to` is
 * empty because the capture was deleted. Returns `null` when none of them named
 * it, so a rename that concerns no control writes nothing.
 */
export function repointedControls<T extends CustomToggle>(
  controls: T[],
  from: string,
  to: string,
): T[] | null {
  if (!controls.some((c) => c.on_capture === from || c.off_capture === from)) return null;

  return controls.map((control) => ({
    ...control,
    on_capture: control.on_capture === from ? to : control.on_capture,
    off_capture: control.off_capture === from ? to : control.off_capture,
  }));
}

/** Whether a control has two different captures, which is what makes it usable. */
const sidesChosen = (control: CustomToggle): boolean =>
  control.on_capture !== "" &&
  control.off_capture !== "" &&
  control.on_capture !== control.off_capture;

/** The capture that puts the device where the control's switch says it is. */
const captureForSide = (control: CustomToggle, profile: PowerProfile): string =>
  toggleEnabled(control, profile) ? control.on_capture : control.off_capture;

/**
 * The capture to replay after a control's captures were reassigned, so the
 * device ends up where the switch says it is — or `null` when it already does.
 *
 * The switch is the truth: assigning captures never moves it, it moves the
 * device. That covers choosing either side, swapping them, and the moment a
 * control first becomes usable, when nothing has ever been replayed for it.
 * Changing the side the switch is *not* on replays nothing, since the device is
 * already in the state the switch shows.
 */
export function captureToSync(
  before: CustomToggle,
  after: CustomToggle,
  profile: PowerProfile,
): string | null {
  if (!sidesChosen(after)) return null;

  const wanted = captureForSide(after, profile);
  if (!sidesChosen(before)) return wanted;

  return wanted === captureForSide(before, profile) ? null : wanted;
}

/**
 * The control with its two captures swapped, for a pair recorded the wrong way
 * round. The switch stays where it is; [`captureToSync`] then says what to
 * replay so the device follows it.
 */
export function swappedCaptures<T extends CustomToggle>(control: T): T {
  return { ...control, on_capture: control.off_capture, off_capture: control.on_capture };
}
