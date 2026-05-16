import { describe, it, expect, vi, beforeEach } from "vitest";
import { listen } from "@tauri-apps/api/event";
import {
  initMenuEvents,
  setMenuHandler,
  __resetForTests,
} from "./menuEvents";

const listenMock = listen as unknown as ReturnType<typeof vi.fn>;

describe("menuEvents", () => {
  beforeEach(() => {
    listenMock.mockReset().mockResolvedValue(() => {});
    __resetForTests();
  });

  it("subscribes to all menu events on init", async () => {
    await initMenuEvents();
    const subscribed = listenMock.mock.calls.map((c) => c[0]);
    expect(subscribed).toEqual(
      expect.arrayContaining([
        "menu:new-project",
        "menu:import-audio",
        "menu:run-pipeline",
        "menu:toggle-sidebar",
      ]),
    );
  });

  it("invokes the registered handler when an event fires", async () => {
    const handlers: Record<string, ((e: { payload: unknown }) => void) | undefined> = {};
    listenMock.mockImplementation(async (event: string, cb: typeof handlers[string]) => {
      handlers[event] = cb;
      return () => {};
    });
    await initMenuEvents();

    const fn = vi.fn();
    setMenuHandler("menu:new-project", fn);
    handlers["menu:new-project"]?.({ payload: null });
    expect(fn).toHaveBeenCalled();
  });

  it("is idempotent — calling init twice does not double-subscribe", async () => {
    await initMenuEvents();
    await initMenuEvents();
    expect(listenMock).toHaveBeenCalledTimes(7);
  });
});
