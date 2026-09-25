// The window has to follow the Windows app mode while it runs, not only pick it
// up at startup. These pin that the pushed change is applied, and that the
// first read cannot overwrite a change that arrived while it was in flight.

import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AppTheme } from "./types";

const { ipc } = vi.hoisted(() => ({
  ipc: {
    handler: null as ((theme: AppTheme) => void) | null,
    getAppTheme: vi.fn<() => Promise<AppTheme>>(),
    onAppTheme: vi.fn(async (handler: (theme: AppTheme) => void) => {
      ipc.handler = handler;
      return () => {};
    }),
  },
}));

vi.mock("./ipc", () => ipc);

const root = { dataset: {} as Record<string, string> };
vi.stubGlobal("document", { documentElement: root });
vi.stubGlobal("window", {
  matchMedia: (query: string) => ({ matches: query === "(prefers-color-scheme: light)" }),
});

import { followAppTheme } from "./appTheme";

beforeEach(() => {
  ipc.handler = null;
  ipc.getAppTheme.mockReset();
  ipc.onAppTheme.mockClear();
  delete root.dataset.theme;
});

describe("followAppTheme", () => {
  it("starts in the mode Windows has now", async () => {
    ipc.getAppTheme.mockResolvedValue("dark");
    await followAppTheme();
    expect(root.dataset.theme).toBe("dark");
  });

  it("switches live when Windows changes mode", async () => {
    ipc.getAppTheme.mockResolvedValue("dark");
    await followAppTheme();

    ipc.handler!("light");
    expect(root.dataset.theme).toBe("light");
    ipc.handler!("dark");
    expect(root.dataset.theme).toBe("dark");
  });

  it("subscribes before it reads, so no change can fall between the two", async () => {
    ipc.getAppTheme.mockImplementation(async () => {
      expect(ipc.onAppTheme).toHaveBeenCalled();
      return "dark";
    });
    await followAppTheme();
    expect(ipc.getAppTheme).toHaveBeenCalled();
  });

  it("does not let a slow first read undo a change pushed meanwhile", async () => {
    let resolveRead!: (theme: AppTheme) => void;
    ipc.getAppTheme.mockReturnValue(new Promise((resolve) => (resolveRead = resolve)));

    const following = followAppTheme();
    await vi.waitFor(() => expect(ipc.handler).not.toBeNull());
    ipc.handler!("light");
    resolveRead("dark");
    await following;

    expect(root.dataset.theme).toBe("light");
  });

  it("falls back to the browser's preference outside the app", async () => {
    ipc.getAppTheme.mockRejectedValue(new Error("no runtime"));
    await followAppTheme();
    expect(root.dataset.theme).toBe("light");
  });
});
