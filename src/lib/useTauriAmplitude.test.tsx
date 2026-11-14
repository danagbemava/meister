import { act, renderHook } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import { describe, expect, it, vi, beforeEach, type Mock } from "vitest";
import { useTauriAmplitude } from "./useTauriAmplitude";

const listenMock = listen as unknown as Mock;

describe("useTauriAmplitude", () => {
  beforeEach(() => {
    listenMock.mockReset();
  });

  it("subscribes on mount and unsubscribes on unmount", async () => {
    const unlisten = vi.fn();
    listenMock.mockResolvedValue(unlisten);

    const { unmount } = renderHook(() => useTauriAmplitude("session-1"));
    // Wait for the listen promise to resolve.
    await act(async () => {});
    expect(listenMock).toHaveBeenCalledWith("call-capture-amplitude", expect.any(Function));
    unmount();
    expect(unlisten).toHaveBeenCalled();
  });

  it("only pushes events whose sessionId matches", async () => {
    let handler: ((e: { payload: { sessionId: string; source: string; rms: number } }) => void) | null = null;
    listenMock.mockImplementation(async (_name: string, cb: (e: unknown) => void) => {
      handler = cb as typeof handler;
      return () => {};
    });

    const { result } = renderHook(() => useTauriAmplitude("session-1"));
    await act(async () => {});
    expect(handler).not.toBeNull();

    await act(async () => {
      handler!({ payload: { sessionId: "session-1", source: "system", rms: 0.4 } });
      handler!({ payload: { sessionId: "other", source: "system", rms: 0.9 } });
      handler!({ payload: { sessionId: "session-1", source: "mic", rms: 0.9 } });
      handler!({ payload: { sessionId: "session-1", source: "system", rms: 0.6 } });
    });

    const { samples } = result.current;
    // The two matching pushes are the most recent two values; the mismatched
    // 0.9 must not appear.
    const last = Array.from(samples.slice(-2));
    expect(last[0]).toBeCloseTo(0.4, 5);
    expect(last[1]).toBeCloseTo(0.6, 5);
    expect(Array.from(samples).every((v) => Math.abs(v - 0.9) > 0.01)).toBe(true);
  });

  it("returns an empty buffer when sessionId is null", () => {
    const { result } = renderHook(() => useTauriAmplitude(null));
    expect(Array.from(result.current.samples).every((v) => v === 0)).toBe(true);
  });
});
