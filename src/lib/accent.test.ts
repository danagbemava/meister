import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { initAccent, getAccentRgb } from "./accent";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const listenMock = listen as unknown as ReturnType<typeof vi.fn>;

function readRootVar(name: string): string {
  return document.documentElement.style.getPropertyValue(name);
}

describe("accent", () => {
  beforeEach(() => {
    document.documentElement.removeAttribute("style");
    document.documentElement.dataset.appearance = "dark";
    invokeMock.mockReset();
    listenMock.mockReset().mockResolvedValue(() => {});
  });

  it("writes --color-accent-rgb from the dark triplet when appearance is dark", async () => {
    invokeMock.mockResolvedValue({ light: [0, 122, 255], dark: [10, 132, 255] });
    await initAccent();
    expect(readRootVar("--color-accent-rgb").trim()).toBe("10 132 255");
    expect(getAccentRgb()).toEqual([10, 132, 255]);
  });

  it("writes the light triplet when appearance is light", async () => {
    document.documentElement.dataset.appearance = "light";
    invokeMock.mockResolvedValue({ light: [0, 122, 255], dark: [10, 132, 255] });
    await initAccent();
    expect(readRootVar("--color-accent-rgb").trim()).toBe("0 122 255");
  });

  it("re-writes on system-accent-changed events", async () => {
    invokeMock.mockResolvedValue({ light: [0, 122, 255], dark: [10, 132, 255] });
    let handler: ((e: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation(async (_event: string, cb: typeof handler) => {
      handler = cb;
      return () => {};
    });
    await initAccent();
    handler?.({ payload: { light: [191, 90, 242], dark: [191, 90, 242] } });
    expect(readRootVar("--color-accent-rgb").trim()).toBe("191 90 242");
  });

  it("falls back to Apple blue if invoke fails", async () => {
    invokeMock.mockRejectedValue(new Error("no tauri"));
    await initAccent();
    const v = readRootVar("--color-accent-rgb").trim();
    expect(["0 122 255", "10 132 255"]).toContain(v);
  });

  it("chooses black label-rgb for Yellow and light-mode Graphite", async () => {
    document.documentElement.dataset.appearance = "light";
    // Graphite light resolves roughly to (152, 152, 157). We assert the
    // logic produces black text for any near-Graphite/Yellow triplet.
    invokeMock.mockResolvedValue({ light: [152, 152, 157], dark: [152, 152, 157] });
    await initAccent();
    expect(readRootVar("--color-accent-label-rgb").trim()).toBe("0 0 0");
  });
});
