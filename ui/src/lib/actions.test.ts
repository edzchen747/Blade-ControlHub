import { describe, expect, it } from "vitest";

import {
  ACTION_KINDS,
  actionKindOption,
  actionSummary,
  availableActionKinds,
  isCommandLabAction,
  renamedTarget,
  strayCommandLabTarget,
  chordLabel,
  defaultActionFor,
  hasActionDetail,
  isActionComplete,
  normalKeyCode,
  normalKeyLabel,
  razerKeyLabel,
  virtualKeyLabel,
} from "./actions";
import {
  KEY_CATALOG,
  MOD_ALT,
  MOD_CTRL,
  MOD_SHIFT,
  MOD_WIN,
  type ActionKind,
  type KeyAction,
} from "./types";

const ALL_KINDS = ACTION_KINDS.map((kind) => kind.value);

describe("availableActionKinds", () => {
  it("hides replaying a capture while experimental features are off", () => {
    const kinds = availableActionKinds(false, "none").map((kind) => kind.value);

    expect(kinds).not.toContain("replay_capture");
    expect(kinds).toEqual(ALL_KINDS.filter((kind) => kind !== "replay_capture"));
  });

  it("offers replaying a capture once experimental features are on", () => {
    const kinds = availableActionKinds(true, "none").map((kind) => kind.value);

    expect(kinds).toEqual(ALL_KINDS);
  });

  // Turning the flag off must not make an existing row's dropdown render blank,
  // and the binding itself keeps working because the captures are in the config.
  it("keeps the option on a row already using it, even with the flag off", () => {
    const kinds = availableActionKinds(false, "replay_capture").map((kind) => kind.value);

    expect(kinds).toContain("replay_capture");
    expect(kinds).toEqual(ALL_KINDS);
  });

  it("does not leak the option onto other rows when one row uses it", () => {
    expect(availableActionKinds(false, "device").map((kind) => kind.value)).not.toContain(
      "replay_capture",
    );
  });

  it("always offers every other kind either way", () => {
    for (const experimental of [true, false]) {
      const kinds = availableActionKinds(experimental, "none").map((kind) => kind.value);
      for (const kind of ALL_KINDS.filter((value) => value !== "replay_capture")) {
        expect(kinds).toContain(kind);
      }
    }
  });
});

describe("defaultActionFor", () => {
  it("builds every kind the dropdown offers", () => {
    for (const kind of ALL_KINDS) {
      expect(defaultActionFor(kind, "cycle_perf_mode", "Snap").kind).toBe(kind);
    }
  });

  // Regression: a row switched away from launching an application and back must
  // not still be holding the old path, which would look bound but point nowhere
  // the user chose.
  it("does not carry fields across a kind change", () => {
    const launch = defaultActionFor("launch_app", "cycle_perf_mode", undefined);
    expect(launch).toEqual({ kind: "launch_app", path: "", args: "", name: "" });

    const command = defaultActionFor("run_command", "cycle_perf_mode", undefined);
    expect(command).toEqual({ kind: "run_command", command: "" });
  });

  it("starts a macro with one empty step so there is something to click", () => {
    const action = defaultActionFor("macro", "cycle_perf_mode", undefined);

    expect(action).toEqual({ kind: "macro", steps: [{ modifiers: 0, key: 0 }] });
    expect(isActionComplete(action)).toBe(false);
  });

  it("starts a key with no key chosen, so the row reads as unfinished", () => {
    const action = defaultActionFor("key", "cycle_perf_mode", undefined);

    expect(action).toEqual({ kind: "key", chord: { modifiers: 0, key: 0 } });
    expect(isActionComplete(action)).toBe(false);
  });

  it("leaves a capture unnamed when none have been recorded", () => {
    const action = defaultActionFor("replay_capture", "cycle_perf_mode", undefined);

    expect(action).toEqual({ kind: "replay_capture", name: "" });
    expect(isActionComplete(action)).toBe(false);
  });
});

describe("isActionComplete", () => {
  it("accepts the kinds that need no configuring", () => {
    expect(isActionComplete({ kind: "none" })).toBe(true);
    expect(isActionComplete({ kind: "toggle_ui" })).toBe(true);
    expect(isActionComplete({ kind: "device", action: "cycle_perf_mode" })).toBe(true);
  });

  it("rejects a key with modifiers but no key", () => {
    expect(isActionComplete({ kind: "key", chord: { modifiers: MOD_CTRL, key: 0 } })).toBe(false);
    expect(isActionComplete({ kind: "key", chord: { modifiers: 0, key: 0x1b } })).toBe(true);
  });

  it("rejects an empty macro and one with any unset step", () => {
    expect(isActionComplete({ kind: "macro", steps: [] })).toBe(false);
    expect(
      isActionComplete({
        kind: "macro",
        steps: [
          { modifiers: MOD_CTRL, key: 0x43 },
          { modifiers: 0, key: 0 },
        ],
      }),
    ).toBe(false);
    expect(
      isActionComplete({
        kind: "macro",
        steps: [
          { modifiers: MOD_CTRL, key: 0x43 },
          { modifiers: MOD_CTRL, key: 0x56 },
        ],
      }),
    ).toBe(true);
  });

  it("treats whitespace-only text as missing", () => {
    expect(isActionComplete({ kind: "launch_app", path: "   ", args: "", name: "A" })).toBe(false);
    expect(isActionComplete({ kind: "run_command", command: "\t\n" })).toBe(false);
    expect(isActionComplete({ kind: "replay_capture", name: " " })).toBe(false);
  });

  it("ignores arguments and label when judging a launch", () => {
    expect(isActionComplete({ kind: "launch_app", path: "a.exe", args: "", name: "" })).toBe(true);
  });

  it("covers every kind, so a new one cannot default to complete by accident", () => {
    for (const kind of ALL_KINDS) {
      expect(typeof isActionComplete(defaultActionFor(kind, "cycle_perf_mode", "Snap"))).toBe(
        "boolean",
      );
    }
  });
});

describe("hasActionDetail", () => {
  it("is false only for the kinds with nothing to configure", () => {
    const withoutDetail = ALL_KINDS.filter(
      (kind) => !hasActionDetail(defaultActionFor(kind, "cycle_perf_mode", "Snap")),
    );

    expect(withoutDetail).toEqual(["none", "toggle_ui"]);
  });
});

describe("chordLabel", () => {
  it("orders modifiers consistently regardless of which are set", () => {
    expect(
      chordLabel({ modifiers: MOD_WIN | MOD_SHIFT | MOD_ALT | MOD_CTRL, key: 0x53 }),
    ).toBe("Ctrl+Alt+Shift+Win+S");
  });

  it("labels a bare key with no leading separator", () => {
    expect(chordLabel({ modifiers: 0, key: 0x70 })).toBe("F1");
  });

  it("shows an unset key as an ellipsis rather than as a code", () => {
    expect(chordLabel({ modifiers: MOD_CTRL, key: 0 })).toBe("Ctrl+\u2026");
  });
});

describe("virtualKeyLabel", () => {
  it("names keys the picker offers", () => {
    expect(virtualKeyLabel(0x41)).toBe("A");
    expect(virtualKeyLabel(0x30)).toBe("0");
    expect(virtualKeyLabel(0x7b)).toBe("F12");
    expect(virtualKeyLabel(0x87)).toBe("F24");
    expect(virtualKeyLabel(0x60)).toBe("Numpad 0");
    expect(virtualKeyLabel(0xaf)).toBe("Volume up");
  });

  it("falls back to a hex code for anything not in the catalogue", () => {
    expect(virtualKeyLabel(0x01)).toBe("0x01");
    expect(virtualKeyLabel(0xff)).toBe("0xFF");
  });
});

describe("KEY_CATALOG", () => {
  // The catalogue is the only way to bind a key the laptop does not have, so a
  // duplicate code would silently shadow an entry in the dropdown.
  it("has no duplicate key codes", () => {
    const codes = KEY_CATALOG.flatMap((group) => group.keys.map(([code]) => code));

    expect(new Set(codes).size).toBe(codes.length);
  });

  it("never offers 0, which is how an unset key is stored", () => {
    const codes = KEY_CATALOG.flatMap((group) => group.keys.map(([code]) => code));

    expect(codes).not.toContain(0);
  });

  it("omits bare modifiers, which a single press cannot hold down", () => {
    const codes = KEY_CATALOG.flatMap((group) => group.keys.map(([code]) => code));

    for (const modifier of [0x10, 0x11, 0x12]) {
      expect(codes).not.toContain(modifier);
    }
  });

  it("reaches the keys a laptop keyboard does not have", () => {
    const codes = new Set(KEY_CATALOG.flatMap((group) => group.keys.map(([code]) => code)));

    expect(codes.has(0x7c)).toBe(true); // F13
    expect(codes.has(0x65)).toBe(true); // Numpad 5
    expect(codes.has(0xa6)).toBe(true); // Browser back
  });

  it("labels every entry", () => {
    for (const group of KEY_CATALOG) {
      expect(group.group).not.toBe("");
      expect(group.keys.length).toBeGreaterThan(0);
      for (const [, label] of group.keys) expect(label.trim()).not.toBe("");
    }
  });
});

describe("actionSummary", () => {
  const labels = { cycle_perf_mode: "Cycle performance mode" };

  it("uses the runtime's own label for a device action", () => {
    expect(actionSummary({ kind: "device", action: "cycle_perf_mode" }, labels)).toBe(
      "Cycle performance mode",
    );
  });

  it("falls back to the raw value when the runtime sent no label", () => {
    expect(actionSummary({ kind: "device", action: "cycle_rgb_effect" }, labels)).toBe(
      "cycle_rgb_effect",
    );
  });

  it("says what is missing on an unfinished action", () => {
    expect(actionSummary({ kind: "key", chord: { modifiers: 0, key: 0 } }, labels)).toBe(
      "No key yet",
    );
    expect(actionSummary({ kind: "macro", steps: [] }, labels)).toBe("No steps yet");
    expect(
      actionSummary({ kind: "launch_app", path: "", args: "", name: "" }, labels),
    ).toBe("No application yet");
  });

  it("prefers an application's name over its path", () => {
    expect(
      actionSummary({ kind: "launch_app", path: "C:\\a\\b.exe", args: "", name: "B" }, labels),
    ).toBe("B");
    expect(
      actionSummary({ kind: "launch_app", path: "C:\\a\\b.exe", args: "", name: " " }, labels),
    ).toBe("C:\\a\\b.exe");
  });

  it("returns something for every kind", () => {
    for (const kind of ALL_KINDS) {
      const summary = actionSummary(defaultActionFor(kind, "cycle_perf_mode", "Snap"), labels);
      expect(summary.trim()).not.toBe("");
    }
  });
});

describe("Hypershift key capture", () => {
  it("accepts letters and digits, in either case", () => {
    expect(normalKeyCode("k")).toBe(0x4b);
    expect(normalKeyCode("K")).toBe(0x4b);
    expect(normalKeyCode("7")).toBe(0x37);
  });

  it("rejects anything else, so a modifier cannot be captured as the key", () => {
    for (const key of ["Shift", "F1", "Escape", "-", "", "ab", " "]) {
      expect(normalKeyCode(key)).toBeNull();
    }
  });

  it("round-trips through its label", () => {
    for (const key of ["A", "Z", "0", "9"]) {
      expect(normalKeyLabel(normalKeyCode(key))).toBe(key);
    }
  });

  it("labels an empty cell and an out-of-range code distinctly", () => {
    expect(normalKeyLabel(null)).toBe("None");
    expect(normalKeyLabel(0x70)).toBe("Unknown");
  });
});

describe("razerKeyLabel", () => {
  // Which key sends which code differs between Blades, so a code is never
  // given a name here: the row's own label is what says what the key is.
  it("shows every captured key as its padded code", () => {
    expect(razerKeyLabel(0x24)).toBe("0x24");
    expect(razerKeyLabel(0xd3)).toBe("0xD3");
    expect(razerKeyLabel(0x03)).toBe("0x03");
    expect(razerKeyLabel(0x99)).toBe("0x99");
  });

  it("shows an uncaptured cell as empty rather than as 0x00", () => {
    expect(razerKeyLabel(null)).toBe("None");
  });
});

describe("a device action the runtime did not offer", () => {
  // The label map is filtered per model, so a row carried over from a Blade
  // that had the hardware names an action this one does not list.
  it("summarises by its own value rather than as blank", () => {
    const labels = { cycle_perf_mode: "Cycle performance mode" };

    expect(actionSummary({ kind: "device", action: "toggle_underglow" }, labels)).toBe(
      "toggle_underglow",
    );
    expect(actionSummary({ kind: "device", action: "cycle_perf_mode" }, labels)).toBe(
      "Cycle performance mode",
    );
  });

  // Its binding still round-trips: the runtime is what refuses to run it, so
  // the row must survive being shown rather than being silently dropped.
  it("is still a complete action", () => {
    expect(isActionComplete({ kind: "device", action: "toggle_underglow" })).toBe(true);
  });
});

describe("the Command Lab option", () => {
  it("covers replaying a capture and flipping a custom control", () => {
    expect(isCommandLabAction("replay_capture")).toBe(true);
    expect(isCommandLabAction("toggle_custom_control")).toBe(true);
    expect(isCommandLabAction("run_command")).toBe(false);
  });

  // A custom control has no option of its own, so without this the dropdown on
  // a row bound to one would render blank.
  it("is the option a custom control reads back as", () => {
    expect(actionKindOption("toggle_custom_control")).toBe("replay_capture");
    expect(actionKindOption("replay_capture")).toBe("replay_capture");
    expect(actionKindOption("device")).toBe("device");
  });

  it("stays available to a row already flipping a control with the flag off", () => {
    const kinds = availableActionKinds(false, "toggle_custom_control").map((kind) => kind.value);

    expect(kinds).toContain("replay_capture");
  });

  it("names itself after the page it belongs to", () => {
    const option = ACTION_KINDS.find((kind) => kind.value === "replay_capture");

    expect(option?.label).toBe("Command Lab");
  });

  it("judges and summarises a custom control by its name", () => {
    expect(isActionComplete({ kind: "toggle_custom_control", name: "Snap Tap" })).toBe(true);
    expect(isActionComplete({ kind: "toggle_custom_control", name: "  " })).toBe(false);
    expect(actionSummary({ kind: "toggle_custom_control", name: "Snap Tap" }, {})).toBe("Snap Tap");
    expect(actionSummary({ kind: "toggle_custom_control", name: "" }, {})).toBe(
      "No custom control yet",
    );
    expect(hasActionDetail({ kind: "toggle_custom_control", name: "" })).toBe(true);
  });
});

describe("the action vocabulary", () => {
  // The runtime accepts these strings as serde tags, so a typo here would be a
  // command the runtime rejects rather than a compile error.
  it("matches the runtime's tags exactly", () => {
    const expected: ActionKind[] = [
      "none",
      "key",
      "macro",
      "device",
      "toggle_ui",
      "launch_app",
      "run_command",
      "replay_capture",
    ];

    expect([...ALL_KINDS].sort()).toEqual([...expected].sort());
  });

  // The dropdown is a list of options, not of runtime kinds: Command Lab covers
  // two, and the row's own control chooses between them.
  it("leaves the custom control kind out of the dropdown", () => {
    expect(ALL_KINDS).not.toContain("toggle_custom_control");
    expect(defaultActionFor("toggle_custom_control", "cycle_perf_mode", "Snap")).toEqual({
      kind: "toggle_custom_control",
      name: "",
    });
  });

  it("has a label for every kind and no duplicates", () => {
    const labels = ACTION_KINDS.map((kind) => kind.label);

    expect(new Set(labels).size).toBe(labels.length);
    for (const label of labels) expect(label.trim()).not.toBe("");
  });

  it("distinguishes sending a key from running a macro", () => {
    const key: KeyAction = { kind: "key", chord: { modifiers: 0, key: 0x41 } };
    const macro: KeyAction = { kind: "macro", steps: [{ modifiers: 0, key: 0x41 }] };

    expect(hasActionDetail(key)).toBe(true);
    expect(hasActionDetail(macro)).toBe(true);
    expect(key.kind).not.toBe(macro.kind);
  });
});

describe("a key binding following its Command Lab target", () => {
  const capture: KeyAction = { kind: "replay_capture", name: "snap tap on" };
  const control: KeyAction = { kind: "toggle_custom_control", name: "Snap Tap" };

  // The reported bug: renaming in Command Lab left the key bound to nothing.
  it("follows a renamed capture", () => {
    expect(renamedTarget(capture, "replay_capture", "snap tap on", "snap tap engaged")).toEqual({
      kind: "replay_capture",
      name: "snap tap engaged",
    });
  });

  it("follows a renamed control", () => {
    expect(renamedTarget(control, "toggle_custom_control", "Snap Tap", "Snap Tap Pro")).toEqual({
      kind: "toggle_custom_control",
      name: "Snap Tap Pro",
    });
  });

  // Cleared rather than removed: the row keeps its key and label, and shows as
  // unfinished so the user repoints it.
  it("clears the target when it was deleted, leaving the row unfinished", () => {
    const cleared = renamedTarget(capture, "replay_capture", "snap tap on", "");

    expect(cleared).toEqual({ kind: "replay_capture", name: "" });
    expect(isActionComplete(cleared!)).toBe(false);
  });

  // A capture and a control may share a name without being the same thing.
  it("leaves a control alone when a capture of the same name is renamed", () => {
    const sameName: KeyAction = { kind: "toggle_custom_control", name: "snap tap on" };

    expect(renamedTarget(sameName, "replay_capture", "snap tap on", "renamed")).toBeNull();
    expect(renamedTarget(capture, "toggle_custom_control", "snap tap on", "renamed")).toBeNull();
  });

  it("leaves a binding to something else alone", () => {
    expect(renamedTarget(capture, "replay_capture", "game mode on", "renamed")).toBeNull();
  });

  it("leaves every other kind of action alone", () => {
    const others: KeyAction[] = [
      { kind: "none" },
      { kind: "toggle_ui" },
      { kind: "run_command", command: "snap tap on" },
      { kind: "launch_app", path: "a.exe", args: "", name: "snap tap on" },
    ];

    for (const action of others) {
      expect(renamedTarget(action, "replay_capture", "snap tap on", "renamed")).toBeNull();
    }
  });
});

describe("what a Command Lab row's dropdown shows", () => {
  const captures = ["snap tap off", "snap tap on"];
  const controls = ["Snap Tap"];

  it("shows nothing extra when the target is on offer", () => {
    expect(
      strayCommandLabTarget({ kind: "replay_capture", name: "snap tap on" }, captures, controls),
    ).toBeNull();
    expect(
      strayCommandLabTarget({ kind: "toggle_custom_control", name: "Snap Tap" }, captures, controls),
    ).toBeNull();
  });

  // A binding cleared by a delete must read as unchosen, not as bound to
  // whichever entry the browser happens to show first.
  it("asks for a choice when the target was cleared", () => {
    expect(strayCommandLabTarget({ kind: "replay_capture", name: "" }, captures, controls)).toBe(
      "Choose a capture or control…",
    );
  });

  it("names a target that is no longer there", () => {
    expect(
      strayCommandLabTarget({ kind: "replay_capture", name: "renamed away" }, captures, controls),
    ).toBe("renamed away (missing)");
  });

  // A control and a capture may share a name; a control is looked for among
  // the controls, so a same-named capture does not make it look present.
  it("looks for the target among its own kind only", () => {
    expect(
      strayCommandLabTarget(
        { kind: "toggle_custom_control", name: "snap tap on" },
        captures,
        controls,
      ),
    ).toBe("snap tap on (missing)");
  });

  it("has nothing to say about other kinds of action", () => {
    expect(strayCommandLabTarget({ kind: "toggle_ui" }, captures, controls)).toBeNull();
  });
});
