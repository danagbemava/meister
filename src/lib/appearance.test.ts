import { describe, it, expect, beforeEach, vi } from "vitest";
import { initAppearance, getAppearance, onAppearanceChange } from "./appearance";

type MediaQueryListener = (e: MediaQueryListEvent) => void;

function mockMatchMedia(initialMatches: boolean) {
  const listeners: MediaQueryListener[] = [];
  const mql = {
    matches: initialMatches,
    media: "(prefers-color-scheme: dark)",
    addEventListener: (_: string, l: MediaQueryListener) => listeners.push(l),
    removeEventListener: () => {},
    onchange: null,
    dispatchEvent: () => false,
    addListener: () => {},
    removeListener: () => {},
  } as unknown as MediaQueryList;

  window.matchMedia = () => mql;

  return {
    flipTo: (matches: boolean) => {
      (mql as { matches: boolean }).matches = matches;
      for (const l of listeners) {
        l({ matches } as MediaQueryListEvent);
      }
    },
  };
}

describe("appearance", () => {
  beforeEach(() => {
    document.documentElement.removeAttribute("data-appearance");
  });

  it("sets data-appearance to 'dark' when system is dark", () => {
    mockMatchMedia(true);
    initAppearance();
    expect(document.documentElement.dataset.appearance).toBe("dark");
    expect(getAppearance()).toBe("dark");
  });

  it("sets data-appearance to 'light' when system is light", () => {
    mockMatchMedia(false);
    initAppearance();
    expect(document.documentElement.dataset.appearance).toBe("light");
    expect(getAppearance()).toBe("light");
  });

  it("flips data-appearance when the media query changes", () => {
    const m = mockMatchMedia(false);
    initAppearance();
    expect(getAppearance()).toBe("light");
    m.flipTo(true);
    expect(document.documentElement.dataset.appearance).toBe("dark");
    expect(getAppearance()).toBe("dark");
  });

  it("notifies subscribers on change", () => {
    const m = mockMatchMedia(false);
    initAppearance();
    const cb = vi.fn();
    onAppearanceChange(cb);
    m.flipTo(true);
    expect(cb).toHaveBeenCalledWith("dark");
    m.flipTo(false);
    expect(cb).toHaveBeenCalledWith("light");
  });
});
