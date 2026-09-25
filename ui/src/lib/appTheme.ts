// Keeps the page in the Windows app mode.
//
// WebView2's `prefers-color-scheme` did not follow a mode change made while the
// app was running, so the runtime listens for the change itself — Windows'
// `WM_SETTINGCHANGE` "ImmersiveColorSet" broadcast — and pushes it here. The
// palette is keyed on `data-theme` on the root element, which this sets.

import * as ipc from "./ipc";
import type { AppTheme } from "./types";

export function applyAppTheme(theme: AppTheme, root: HTMLElement = document.documentElement) {
  root.dataset.theme = theme;
}

/**
 * Applies the current mode and follows every change after it, for the life of
 * the page.
 */
export async function followAppTheme(): Promise<void> {
  // Subscribe before reading, so a change made in between is not lost; and once
  // a change has arrived, the read is older than it and must not win.
  let pushed = false;
  await ipc.onAppTheme((theme) => {
    pushed = true;
    applyAppTheme(theme);
  });

  let theme: AppTheme;
  try {
    theme = await ipc.getAppTheme();
  } catch {
    // Outside the app, as in a plain browser during development.
    theme = window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }
  if (!pushed) applyAppTheme(theme);
}
