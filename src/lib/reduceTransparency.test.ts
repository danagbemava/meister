import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  initReduceTransparency,
  isReduceTransparency,
  __resetForTests,
} from "./reduceTransparency";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const listenMock = listen as unknown as ReturnType<typeof vi.fn>;

describe("reduceTransparency", () => {
  beforeEach(() => {
    document.documentElement.removeAttribute("data-reduce-transparency");
    invokeMock.mockReset();
    listenMock.mockReset().mockResolvedValue(() => {});
    __resetForTests();
  });

  it("sets data-reduce-transparency='true' when the system setting is on", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_reduce_transparency") return true;
      if (cmd === "is_vibrancy_active") return true;
      return undefined;
    });
    await initReduceTransparency();
    expect(
      document.documentElement.dataset.reduceTransparency,
    ).toBe("true");
    expect(isReduceTransparency()).toBe(true);
  });

  it("removes the attribute when the system setting is off", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_reduce_transparency") return false;
      if (cmd === "is_vibrancy_active") return true;
      return undefined;
    });
    await initReduceTransparency();
    expect(
      document.documentElement.dataset.reduceTransparency,
    ).toBeUndefined();
    expect(isReduceTransparency()).toBe(false);
  });

  it("re-applies on system-reduce-transparency-changed events", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_reduce_transparency") return false;
      if (cmd === "is_vibrancy_active") return true;
      return undefined;
    });
    let handler: ((e: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation(async (event: string, cb: typeof handler) => {
      if (event === "system-reduce-transparency-changed") handler = cb;
      return () => {};
    });
    await initReduceTransparency();
    handler?.({ payload: true });
    expect(
      document.documentElement.dataset.reduceTransparency,
    ).toBe("true");
    handler?.({ payload: false });
    expect(
      document.documentElement.dataset.reduceTransparency,
    ).toBeUndefined();
  });

  it("falls back to false (vibrancy on) if invoke fails", async () => {
    invokeMock.mockRejectedValue(new Error("no tauri"));
    await initReduceTransparency();
    expect(isReduceTransparency()).toBe(false);
  });

  it("forces reduce-transparency mode when vibrancy-unavailable fires", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_reduce_transparency") return false;
      if (cmd === "is_vibrancy_active") return true;
      return undefined;
    });
    const handlers: Record<string, ((e: { payload: unknown }) => void) | undefined> = {};
    listenMock.mockImplementation(async (event: string, cb: typeof handlers[string]) => {
      handlers[event] = cb;
      return () => {};
    });
    await initReduceTransparency();
    expect(isReduceTransparency()).toBe(false);
    handlers["vibrancy-unavailable"]?.({ payload: null });
    expect(
      document.documentElement.dataset.reduceTransparency,
    ).toBe("true");
    expect(isReduceTransparency()).toBe(true);
  });

  it("forces reduce-transparency mode if vibrancy did not attach at boot", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_reduce_transparency") return false;
      if (cmd === "is_vibrancy_active") return false;
      return undefined;
    });
    await initReduceTransparency();
    expect(isReduceTransparency()).toBe(true);
    expect(document.documentElement.dataset.reduceTransparency).toBe("true");
  });

  it("is idempotent — calling init twice does not double-register", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_reduce_transparency") return false;
      if (cmd === "is_vibrancy_active") return true;
      return undefined;
    });
    await initReduceTransparency();
    await initReduceTransparency();
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });
});
