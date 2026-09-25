// A jump must move only the page's scroll area. Scrolling anything above it
// shoves the whole window up and leaves the bottom of it blank.

import { afterEach, describe, expect, it, vi } from "vitest";

import { jumpTo } from "./scroll";

interface FakeOptions {
  targetTop: number;
  containerTop: number;
  scrollTop: number;
  scrollMarginTop?: string;
  reducedMotion?: boolean;
  inContainer?: boolean;
}

function setup({
  targetTop,
  containerTop,
  scrollTop,
  scrollMarginTop = "0px",
  reducedMotion = false,
  inContainer = true,
}: FakeOptions) {
  const container = {
    scrollTop,
    scrollTo: vi.fn(),
    getBoundingClientRect: () => ({ top: containerTop }),
  };
  const target = {
    scrollIntoView: vi.fn(),
    closest: (selector: string) => (selector === ".content" && inContainer ? container : null),
    getBoundingClientRect: () => ({ top: targetTop }),
  };

  vi.stubGlobal("document", {
    getElementById: (id: string) => (id === "section" ? target : null),
  });
  vi.stubGlobal("getComputedStyle", () => ({ scrollMarginTop }));
  vi.stubGlobal("window", { matchMedia: () => ({ matches: reducedMotion }) });

  return { container, target };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("jumpTo", () => {
  it("scrolls the page's scroll area, never every ancestor", () => {
    const { container, target } = setup({ targetTop: 900, containerTop: 60, scrollTop: 0 });

    jumpTo("section");

    expect(target.scrollIntoView).not.toHaveBeenCalled();
    expect(container.scrollTo).toHaveBeenCalledOnce();
  });

  it("lands the section at the top, keeping its scroll margin clear", () => {
    const { container } = setup({
      targetTop: 900,
      containerTop: 60,
      scrollTop: 200,
      scrollMarginTop: "16px",
    });

    jumpTo("section");

    // 900 - 60 + 200 - 16
    expect(container.scrollTo).toHaveBeenCalledWith({ top: 1024, behavior: "smooth" });
  });

  it("skips the animation when reduced motion is asked for", () => {
    const { container } = setup({
      targetTop: 500,
      containerTop: 0,
      scrollTop: 0,
      reducedMotion: true,
    });

    jumpTo("section");

    expect(container.scrollTo).toHaveBeenCalledWith({ top: 500, behavior: "auto" });
  });

  it("does nothing for a missing section", () => {
    const { container } = setup({ targetTop: 500, containerTop: 0, scrollTop: 0 });

    jumpTo("elsewhere");

    expect(container.scrollTo).not.toHaveBeenCalled();
  });

  it("does nothing outside a scroll area rather than scrolling the window", () => {
    const { target } = setup({
      targetTop: 500,
      containerTop: 0,
      scrollTop: 0,
      inContainer: false,
    });

    expect(() => jumpTo("section")).not.toThrow();
    expect(target.scrollIntoView).not.toHaveBeenCalled();
  });
});

describe("page sources", () => {
  // `scrollIntoView` scrolls the `overflow: hidden` document too, which is the
  // bug `jumpTo` exists to avoid. Any new jump has to go through it.
  const sources = import.meta.glob("./**/*.svelte", {
    query: "?raw",
    import: "default",
    eager: true,
  }) as Record<string, string>;

  it("finds the components to check", () => {
    expect(Object.keys(sources).length).toBeGreaterThan(0);
  });

  it.each(Object.entries(sources))("%s does not call scrollIntoView", (_path, source) => {
    expect(source).not.toMatch(/\.scrollIntoView\s*\(/);
  });
});
