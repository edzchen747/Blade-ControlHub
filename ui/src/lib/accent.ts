// The accent colour is one setting shown in two places: Settings > Appearance,
// and Lighting, where the Static and Reactive keyboard effects light up in it.
// Both pickers go through this module, so they share one value, one pending
// write and one error. Two separate debounces would mean a change in one could
// be followed by a stale colour still waiting to be written by the other.

import { COLOR_COMMIT_MS, debounce } from "./debounce";
import * as ipc from "./ipc";
import { store } from "./store.svelte";
import type { RGBEffect, ThemeColor } from "./types";

/** The `store` control the accent's pending write and error are kept under. */
export const ACCENT_CONTROL = "accent";

/** Matches `ThemeColor::default()` in the runtime. */
export const DEFAULT_ACCENT: ThemeColor = { r: 0xff, g: 0xd7, b: 0x00 };

/** Effects the runtime draws in the accent (`set_rgb_effect` in keyboard.rs). */
export const ACCENT_EFFECTS: ReadonlySet<RGBEffect> = new Set(["Static", "Reactive"]);

export function toHex({ r, g, b }: ThemeColor): string {
  const part = (value: number) => value.toString(16).padStart(2, "0");
  return `#${part(r)}${part(g)}${part(b)}`;
}

export function fromHex(hex: string): ThemeColor {
  return {
    r: parseInt(hex.slice(1, 3), 16),
    g: parseInt(hex.slice(3, 5), 16),
    b: parseInt(hex.slice(5, 7), 16),
  };
}

// The picker fires continuously while dragging, and each commit is a HID
// write, so the colour previews locally and only the write is debounced.
const commitAccent = debounce(
  (color: ThemeColor) =>
    store.run(
      ACCENT_CONTROL,
      (next) => {
        next.theme_color = color;
      },
      () => ipc.setThemeColor(color),
    ),
  COLOR_COMMIT_MS,
);

export function previewAccent(hex: string) {
  const color = fromHex(hex);
  store.preview(ACCENT_CONTROL, (next) => {
    next.theme_color = color;
  });
  commitAccent(color);
}

export function resetAccent() {
  commitAccent.cancel();
  store.preview(ACCENT_CONTROL, (next) => {
    next.theme_color = DEFAULT_ACCENT;
  });
  commitAccent(DEFAULT_ACCENT);
  commitAccent.flush();
}
