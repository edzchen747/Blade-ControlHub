// The accent is edited from Settings and from Lighting. These tests pin that
// both pickers go through one value and one pending write, so a change made in
// one place is never overwritten by a stale colour queued from the other.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ThemeColor } from "./types";

const { setThemeColor, fakeStore } = vi.hoisted(() => {
  const state = { theme_color: { r: 0, g: 0, b: 0 } as ThemeColor };
  return {
    setThemeColor: vi.fn(async (_color: ThemeColor) => {}),
    fakeStore: {
      state,
      errors: {} as Record<string, string>,
      preview: vi.fn((_control: string, apply: (next: typeof state) => void) => apply(state)),
      run: vi.fn(
        (_control: string, apply: (next: typeof state) => void, command: () => Promise<unknown>) => {
          apply(state);
          return command();
        },
      ),
    },
  };
});

vi.mock("./ipc", () => ({ setThemeColor }));
vi.mock("./store.svelte", () => ({ store: fakeStore }));

import {
  ACCENT_CONTROL,
  ACCENT_EFFECTS,
  DEFAULT_ACCENT,
  fromHex,
  previewAccent,
  resetAccent,
  toHex,
} from "./accent";
import { COLOR_COMMIT_MS } from "./debounce";

beforeEach(() => {
  vi.useFakeTimers();
  setThemeColor.mockClear();
  fakeStore.preview.mockClear();
  fakeStore.run.mockClear();
  fakeStore.state.theme_color = { r: 0, g: 0, b: 0 };
});

afterEach(() => {
  vi.useRealTimers();
});

describe("hex conversion", () => {
  it("round-trips a colour", () => {
    const color = { r: 0x12, g: 0xab, b: 0x07 };
    expect(toHex(color)).toBe("#12ab07");
    expect(fromHex(toHex(color))).toEqual(color);
  });

  it("pads single-digit channels", () => {
    expect(toHex({ r: 0, g: 1, b: 15 })).toBe("#00010f");
  });
});

describe("shared accent", () => {
  it("previews at once and writes once the drag settles", () => {
    previewAccent("#ff0000");
    previewAccent("#00ff00");

    expect(fakeStore.state.theme_color).toEqual({ r: 0, g: 0xff, b: 0 });
    expect(setThemeColor).not.toHaveBeenCalled();

    vi.advanceTimersByTime(COLOR_COMMIT_MS);

    expect(setThemeColor).toHaveBeenCalledOnce();
    expect(setThemeColor).toHaveBeenCalledWith({ r: 0, g: 0xff, b: 0 });
  });

  it("keeps the preview, write and error under one control for both pickers", () => {
    previewAccent("#0000ff");
    vi.advanceTimersByTime(COLOR_COMMIT_MS);

    expect(fakeStore.preview.mock.calls.every(([control]) => control === ACCENT_CONTROL)).toBe(
      true,
    );
    expect(fakeStore.run.mock.calls.every(([control]) => control === ACCENT_CONTROL)).toBe(true);
  });

  it("drops a drag still waiting to be written when the colour is reset", () => {
    // A drag on one page followed by Reset on the other must not let the
    // dragged colour land afterwards and undo the reset.
    previewAccent("#123456");
    resetAccent();
    vi.advanceTimersByTime(COLOR_COMMIT_MS * 2);

    expect(setThemeColor).toHaveBeenCalledOnce();
    expect(setThemeColor).toHaveBeenCalledWith(DEFAULT_ACCENT);
    expect(fakeStore.state.theme_color).toEqual(DEFAULT_ACCENT);
  });
});

describe("accent effects", () => {
  it("are the ones the runtime draws in the accent", () => {
    // `set_rgb_effect` in src/razer/handlers/keyboard.rs sends the accent
    // for these two effects only.
    expect([...ACCENT_EFFECTS].sort()).toEqual(["Reactive", "Static"]);
  });
});
