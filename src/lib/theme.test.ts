import { beforeEach, describe, expect, it } from "vitest";
import { THEMES, applyTheme, getStoredTheme, initTheme } from "./theme";

describe("theme", () => {
  beforeEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
    delete document.documentElement.dataset.appearance;
  });

  it("exposes the four themes with ink-indigo first (default)", () => {
    expect(THEMES.map((t) => t.key)).toEqual([
      "ink-indigo", "forest-brass", "plum-rose", "manuscript",
    ]);
  });

  it("applyTheme sets data-theme, derives appearance, persists", () => {
    applyTheme("manuscript");
    expect(document.documentElement.dataset.theme).toBe("manuscript");
    expect(document.documentElement.dataset.appearance).toBe("light");
    expect(localStorage.getItem("meister-theme")).toBe("manuscript");
    applyTheme("forest-brass");
    expect(document.documentElement.dataset.appearance).toBe("dark");
  });

  it("getStoredTheme falls back to ink-indigo on missing/garbage", () => {
    expect(getStoredTheme()).toBe("ink-indigo");
    localStorage.setItem("meister-theme", "not-a-theme");
    expect(getStoredTheme()).toBe("ink-indigo");
  });

  it("initTheme applies the stored theme", () => {
    localStorage.setItem("meister-theme", "plum-rose");
    initTheme();
    expect(document.documentElement.dataset.theme).toBe("plum-rose");
    expect(document.documentElement.dataset.appearance).toBe("dark");
  });
});
